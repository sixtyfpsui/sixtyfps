/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Do not read twice the same property, store in a local variable instead

use crate::expression_tree::*;
use crate::langtype::Type;
use crate::object_tree::*;
use std::cell::RefCell;
use std::collections::HashMap;

pub fn deduplicate_property_read(component: &Component) {
    visit_all_expressions(component, |expr, ty| {
        if matches!(ty(), Type::Callback { .. }) {
            // Callback handler can't be optimizes because they can have side effect.
            // But that's fine as they also do not register dependencies
            return;
        }
        process_expression(expr, &DedupPropState::default());
    });
}

#[derive(Default)]
struct PropertyReadCounts {
    counts: HashMap<NamedReference, usize>,
    /// If at least one element of the map has duplicates
    has_duplicate: bool,
}

#[derive(Default)]
struct DedupPropState<'a> {
    parent_state: Option<&'a DedupPropState<'a>>,
    counts: RefCell<PropertyReadCounts>,
}

impl<'a> DedupPropState<'a> {
    fn add(&self, nr: &NamedReference) {
        if self.parent_state.map_or(false, |pc| pc.add_from_children(nr)) {
            return;
        }
        let mut use_counts = self.counts.borrow_mut();
        let use_counts = &mut *use_counts;
        let has_duplicate = &mut use_counts.has_duplicate;
        use_counts
            .counts
            .entry(nr.clone())
            .and_modify(|c| {
                if *c == 1 {
                    *has_duplicate = true;
                }
                *c += 1
            })
            .or_insert(1);
    }

    fn add_from_children(&self, nr: &NamedReference) -> bool {
        if self.parent_state.map_or(false, |pc| pc.add_from_children(nr)) {
            return true;
        }
        let mut use_counts = self.counts.borrow_mut();
        let use_counts = &mut *use_counts;
        if let Some(c) = use_counts.counts.get_mut(nr) {
            if *c == 1 {
                use_counts.has_duplicate = true;
            }
            *c += 1;
            true
        } else {
            false
        }
    }

    fn get_mapping(&self, nr: &NamedReference) -> Option<String> {
        self.parent_state.and_then(|pr| pr.get_mapping(nr)).or_else(|| {
            if self.counts.borrow().counts.get(nr).map_or(false, |c| *c > 1) {
                Some(format!("tmp_{}_{}", nr.element().borrow().id, nr.name()))
            } else {
                None
            }
        })
    }
}

fn process_expression(expr: &mut Expression, old_state: &DedupPropState) {
    if let Expression::TwoWayBinding(_, expr) = expr {
        if let Some(expr) = expr {
            process_expression(&mut *expr, old_state)
        }
        return;
    }

    let new_state = DedupPropState { parent_state: Some(old_state), ..DedupPropState::default() };
    collect_unconditional_read_count(expr, &new_state);
    process_conditional_expressions(expr, &new_state);
    do_replacements(expr, &new_state);
    if new_state.counts.borrow().has_duplicate {
        let mut stores = vec![];
        for (nr, count) in &new_state.counts.borrow().counts {
            if *count > 1 {
                let new_name = new_state.get_mapping(nr).unwrap();
                stores.push(Expression::StoreLocalVariable {
                    name: new_name,
                    value: Box::new(Expression::PropertyReference(nr.clone())),
                });
            }
        }
        stores.push(std::mem::take(expr));
        *expr = Expression::CodeBlock(stores);
    }
}

// Collect all use of variable and their count, only in non conditional expression
fn collect_unconditional_read_count(expr: &Expression, result: &DedupPropState) {
    match expr {
        Expression::PropertyReference(nr) => {
            result.add(nr);
        }
        //Expression::RepeaterIndexReference { element } => {}
        //Expression::RepeaterModelReference { element } => {}
        Expression::BinaryExpression { lhs, rhs: _, op } if matches!(op, '|' | '&') => {
            lhs.visit(|sub| collect_unconditional_read_count(sub, result))
        }
        Expression::Condition { condition, .. } => {
            condition.visit(|sub| collect_unconditional_read_count(sub, result))
        }
        _ => expr.visit(|sub| collect_unconditional_read_count(sub, result)),
    }
}

fn process_conditional_expressions(expr: &mut Expression, state: &DedupPropState) {
    match expr {
        Expression::BinaryExpression { lhs, rhs, op } if matches!(op, '|' | '&') => {
            lhs.visit_mut(|sub| process_conditional_expressions(sub, state));
            process_expression(rhs, state);
        }
        Expression::Condition { condition, true_expr, false_expr } => {
            condition.visit_mut(|sub| process_conditional_expressions(sub, state));
            process_expression(true_expr, state);
            process_expression(false_expr, state);
        }
        _ => expr.visit_mut(|sub| process_conditional_expressions(sub, state)),
    }
}

fn do_replacements(expr: &mut Expression, state: &DedupPropState) {
    match expr {
        Expression::PropertyReference(nr) => {
            if let Some(name) = state.get_mapping(nr) {
                let ty = expr.ty();
                *expr = Expression::ReadLocalVariable { name, ty };
            }
        }
        Expression::BinaryExpression { lhs, rhs: _, op } if matches!(op, '|' | '&') => {
            lhs.visit_mut(|sub| do_replacements(sub, state));
        }
        Expression::Condition { condition, .. } => {
            condition.visit_mut(|sub| do_replacements(sub, state));
        }
        _ => expr.visit_mut(|sub| do_replacements(sub, state)),
    }
}
