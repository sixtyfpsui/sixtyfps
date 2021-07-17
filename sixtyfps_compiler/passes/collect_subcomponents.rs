/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Passes that fills the root component used_types.sub_components

use by_address::ByAddress;

use crate::langtype::Type;
use crate::object_tree::*;
use std::collections::HashSet;
use std::rc::Rc;

/// Fill the root_component's used_types.sub_components
pub fn collect_subcomponents(root_component: &Rc<Component>) {
    let mut result = vec![];
    let mut hash = HashSet::new();

    collect_subcomponents_recursive(root_component, &mut result, &mut hash);

    root_component.used_types.borrow_mut().sub_components = result;
}

fn collect_subcomponents_recursive(
    component: &Rc<Component>,
    result: &mut Vec<Rc<Component>>,
    hash: &mut HashSet<ByAddress<Rc<Component>>>,
) {
    hash.insert(ByAddress(component.clone()));
    recurse_elem(&component.root_element, &(), &mut |elem: &ElementRc, &()| {
        let base_comp = match &elem.borrow().base_type {
            Type::Component(base_comp) => {
                if hash.contains(&ByAddress(base_comp.clone())) {
                    return;
                }
                base_comp.clone()
            }
            _ => return,
        };
        collect_subcomponents_recursive(&base_comp, result, hash);
        result.push(base_comp);
    });
}
