/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Pass that create a state property, and change all the binding to depend on that property

use crate::diagnostics::BuildDiagnostics;
use crate::diagnostics::SourceLocation;
use crate::expression_tree::*;
use crate::langtype::Type;
use crate::object_tree::*;
use std::collections::HashSet;
use std::{collections::HashMap, rc::Rc};

pub fn lower_states(
    component: &Rc<Component>,
    tr: &crate::typeregister::TypeRegister,
    diag: &mut BuildDiagnostics,
) {
    let state_info_type = tr.lookup("StateInfo");
    assert!(matches!(state_info_type, Type::Struct { name: Some(_), .. }));
    recurse_elem(&component.root_element, &(), &mut |elem, _| {
        lower_state_in_element(elem, &state_info_type, diag)
    });
}

fn lower_state_in_element(
    root_element: &ElementRc,
    state_info_type: &Type,
    diag: &mut BuildDiagnostics,
) {
    if root_element.borrow().states.is_empty() {
        return;
    }
    let has_transitions = !root_element.borrow().transitions.is_empty();
    let state_property_name = compute_state_property_name(root_element);
    let state_property =
        Expression::PropertyReference(NamedReference::new(root_element, &state_property_name));
    let state_property_ref = if has_transitions {
        Expression::StructFieldAccess {
            base: Box::new(state_property.clone()),
            name: "current_state".into(),
        }
    } else {
        state_property.clone()
    };
    let mut affected_properties = HashSet::new();
    // Maps State name string -> integer id
    let mut states_id = HashMap::new();
    let mut state_value = Expression::NumberLiteral(0., Unit::None);
    let states = std::mem::take(&mut root_element.borrow_mut().states);
    for (idx, state) in states.into_iter().enumerate().rev() {
        if let Some(condition) = &state.condition {
            state_value = Expression::Condition {
                condition: Box::new(condition.clone()),
                true_expr: Box::new(Expression::NumberLiteral((idx + 1) as _, Unit::None)),
                false_expr: Box::new(std::mem::take(&mut state_value)),
            };
        }
        for (ne, expr) in state.property_changes {
            affected_properties.insert(ne.clone());
            let e = ne.element();
            let property_expr = expression_for_property(&e, ne.name());
            let new_expr = Expression::Condition {
                condition: Box::new(Expression::BinaryExpression {
                    lhs: Box::new(state_property_ref.clone()),
                    rhs: Box::new(Expression::NumberLiteral((idx + 1) as _, Unit::None)),
                    op: '=',
                }),
                true_expr: Box::new(expr),
                false_expr: Box::new(property_expr),
            };
            match e.borrow_mut().bindings.entry(ne.name().to_owned()) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    e.get_mut().expression = new_expr
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(new_expr.into());
                }
            };
        }
        states_id.insert(state.id, idx as i32 + 1);
    }

    root_element.borrow_mut().property_declarations.insert(
        state_property_name.clone(),
        PropertyDeclaration {
            property_type: if has_transitions { state_info_type.clone() } else { Type::Int32 },
            ..PropertyDeclaration::default()
        },
    );
    root_element.borrow_mut().bindings.insert(state_property_name, state_value.into());

    lower_transitions_in_element(
        root_element,
        state_property,
        states_id,
        affected_properties,
        diag,
    );
}

fn lower_transitions_in_element(
    elem: &ElementRc,
    state_property: Expression,
    states_id: HashMap<String, i32>,
    affected_properties: HashSet<NamedReference>,
    diag: &mut BuildDiagnostics,
) {
    let transitions = std::mem::take(&mut elem.borrow_mut().transitions);
    let mut props =
        HashMap::<NamedReference, (SourceLocation, Vec<TransitionPropertyAnimation>)>::new();
    for transition in transitions {
        let state = states_id.get(&transition.state_id).unwrap_or_else(|| {
            diag.push_error(
                format!("State '{}' does not exist", transition.state_id),
                &transition.node,
            );
            &0
        });

        for (p, span, animation) in transition.property_animations {
            if !affected_properties.contains(&p) {
                diag.push_error(
                    "The property is not changed as part of this transition".into(),
                    &span,
                );
            }

            let t = TransitionPropertyAnimation {
                state_id: *state,
                is_out: transition.is_out,
                animation,
            };
            props.entry(p).or_insert_with(|| (span.clone(), vec![])).1.push(t);
        }
    }
    for (ne, (span, animations)) in props {
        let e = ne.element();
        match e.borrow_mut().property_animations.entry(ne.name().to_owned()) {
            std::collections::hash_map::Entry::Occupied(e) => {
                diag.push_error(
                    format!(
                    "The property '{}' cannot have transition because it already has an animation",
                    e.key()
                ),
                    &span,
                );
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(PropertyAnimation::Transition {
                    state_ref: state_property.clone(),
                    animations,
                });
            }
        };
    }
}

/// Returns a suitable unique name for the "state" property
fn compute_state_property_name(root_element: &ElementRc) -> String {
    let mut property_name = "state".to_owned();
    while root_element.borrow().lookup_property(property_name.as_ref()).property_type
        != Type::Invalid
    {
        property_name += "_";
    }
    property_name
}

/// Return the expression binding currently associated to the given property
fn expression_for_property(element: &ElementRc, name: &str) -> Expression {
    let mut element_it = Some(element.clone());
    while let Some(element) = element_it {
        if let Some(e) = element.borrow().bindings.get(name) {
            return e.expression.clone();
        }
        element_it = if let Type::Component(base) = &element.borrow().base_type {
            Some(base.root_element.clone())
        } else {
            None
        };
    }
    Expression::default_value_for_type(&element.borrow().lookup_property(name).property_type)
}
