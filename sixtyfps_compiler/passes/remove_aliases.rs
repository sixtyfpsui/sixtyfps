/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! This pass removes the property used in a two ways bindings

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{Expression, NamedReference};
use crate::object_tree::*;
use std::cell::RefCell;
use std::collections::{btree_map::Entry, HashMap, HashSet};
use std::rc::Rc;

type Mapping = HashMap<NamedReference, NamedReference>;

#[derive(Default, Debug)]
struct PropertySets {
    map: HashMap<NamedReference, Rc<RefCell<HashSet<NamedReference>>>>,
    all_sets: Vec<Rc<RefCell<HashSet<NamedReference>>>>,
}

impl PropertySets {
    fn add_link(&mut self, p1: NamedReference, p2: NamedReference) {
        if !std::rc::Weak::ptr_eq(
            &p1.element().borrow().enclosing_component,
            &p2.element().borrow().enclosing_component,
        ) {
            // We can  only merge aliases if they are in the same Component.
            // TODO: actually we could still merge two alias in a component pointing to the same
            // property in a parent component
            return;
        }

        if let Some(s1) = self.map.get(&p1).cloned() {
            if let Some(s2) = self.map.get(&p2).cloned() {
                if Rc::ptr_eq(&s1, &s2) {
                    return;
                }
                for x in s1.borrow().iter() {
                    self.map.insert(x.clone(), s2.clone());
                    s2.borrow_mut().insert(x.clone());
                }
                *s1.borrow_mut() = HashSet::new();
            } else {
                s1.borrow_mut().insert(p2.clone());
                self.map.insert(p2, s1);
            }
        } else if let Some(s2) = self.map.get(&p2).cloned() {
            s2.borrow_mut().insert(p1.clone());
            self.map.insert(p1, s2);
        } else {
            let mut set = HashSet::new();
            set.insert(p1.clone());
            set.insert(p2.clone());
            let set = Rc::new(RefCell::new(set));
            self.map.insert(p1, set.clone());
            self.map.insert(p2, set.clone());
            self.all_sets.push(set)
        }
    }
}

pub fn remove_aliases(component: &Rc<Component>, diag: &mut BuildDiagnostics) {
    // collect all sets that are linked together
    let mut property_sets = PropertySets::default();
    recurse_elem_including_sub_components(component, &(), &mut |e, _| {
        'bindings: for (name, binding) in &e.borrow().bindings {
            let mut exp = &binding.expression;
            while let Expression::TwoWayBinding(nr, next) = exp {
                let other_e = nr.element();
                if name == nr.name() && Rc::ptr_eq(e, &other_e) {
                    diag.push_error("Property cannot alias to itself".into(), binding);
                    continue 'bindings;
                }
                property_sets.add_link(NamedReference::new(e, name), nr.clone());

                exp = match next {
                    Some(x) => &*x,
                    None => break,
                };
            }
        }
    });

    // The key will be removed and replaced by the named reference
    let mut aliases_to_remove = Mapping::new();

    // For each set, find a "master" property. Only reference to this master property will be kept,
    // and only the master property will keep its binding
    for set in property_sets.all_sets {
        let set = set.borrow();
        let mut set_iter = set.iter();
        if let Some(mut best) = set_iter.next().cloned() {
            for candidate in set_iter {
                best = best_property(component, best.clone(), candidate.clone());
            }
            for x in set.iter() {
                if *x != best {
                    aliases_to_remove.insert(x.clone(), best.clone());
                }
            }
        }
    }

    // Do the replacements
    visit_all_named_references(&component, &mut |nr: &mut NamedReference| {
        if let Some(new) = aliases_to_remove.get(nr) {
            *nr = new.clone();
        }
    });

    // Remove the properties
    for (remove, to) in aliases_to_remove {
        let elem = remove.element();

        // adjust the bindings
        let old_binding = elem.borrow_mut().bindings.remove(remove.name());
        let must_simplify = if let Some(mut binding) = old_binding {
            simplify_expression(&mut binding.expression, &to);
            if !matches!(binding.expression, Expression::Invalid) {
                let to_elem = to.element();
                match to_elem.borrow_mut().bindings.entry(to.name().to_owned()) {
                    Entry::Occupied(mut e) => {
                        simplify_expression(e.get_mut(), &to);
                        if matches!(e.get().expression, Expression::Invalid) {
                            *e.get_mut() = binding;
                        } else if e.get().priority < binding.priority {
                            crate::passes::inlining::maybe_merge_two_ways(
                                &mut e.get_mut().expression,
                                &mut 0,
                                &binding,
                            );
                        } else {
                            crate::passes::inlining::maybe_merge_two_ways(
                                &mut binding.expression,
                                &mut 0,
                                e.get(),
                            );
                            *e.get_mut() = binding;
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert(binding);
                    }
                };
                false
            } else {
                true
            }
        } else {
            true
        };

        if must_simplify {
            let to_elem = to.element();
            let mut to_elem = to_elem.borrow_mut();
            if let Some(b) = to_elem.bindings.get_mut(to.name()) {
                simplify_expression(&mut b.expression, &to);
                if matches!(b.expression, Expression::Invalid) {
                    to_elem.bindings.remove(to.name());
                }
            }
        }

        // Remove the declaration
        {
            let mut elem = elem.borrow_mut();
            if let Some(d) = elem.property_declarations.get_mut(remove.name()) {
                if d.expose_in_public_api {
                    d.is_alias = Some(to.clone());
                    drop(elem);
                    // one must mark the aliased property as settable from outside
                    to.element()
                        .borrow()
                        .property_analysis
                        .borrow_mut()
                        .entry(to.name().into())
                        .or_default()
                        .is_set = true;
                } else {
                    elem.property_declarations.remove(remove.name());
                }
            } else {
                // This is not a declaration, we must re-create the binding
                elem.bindings
                    .insert(remove.name().to_owned(), Expression::TwoWayBinding(to, None).into());
            }
        }
    }
}

fn is_declaration(x: &NamedReference) -> bool {
    x.element().borrow().property_declarations.contains_key(x.name())
}

/// Out of two named reference, return the one which is the best to keep.
fn best_property<'a>(
    component: &Rc<Component>,
    p1: NamedReference,
    p2: NamedReference,
) -> NamedReference {
    // Try to find which is the more canonical property
    macro_rules! canonical_order {
        ($x: expr) => {{
            (
                is_declaration(&$x),
                !Rc::ptr_eq(&component.root_element, &$x.element()),
                $x.element().borrow().id.clone(),
                $x.name(),
            )
        }};
    }

    if canonical_order!(p1) < canonical_order!(p2) {
        p1
    } else {
        p2
    }
}

/// Remove the `TwoWayBinding(to, _)` from the chain of TwoWayBinding
fn simplify_expression(expression: &mut Expression, to: &NamedReference) {
    if let Expression::TwoWayBinding(nr, rest) = expression {
        if let Some(ref mut r) = rest {
            simplify_expression(&mut *r, to);
        }
        if nr == to {
            if let Some(r) = std::mem::take(rest) {
                *expression = *r;
            } else {
                *expression = Expression::Invalid;
            }
        }
    }
}
