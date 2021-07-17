/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Remove the properties which are not used

use crate::object_tree::Component;
use std::collections::HashSet;

pub fn remove_unused_properties(component: &Component) {
    crate::object_tree::recurse_elem_including_sub_components_no_borrow(
        &component,
        &(),
        &mut |elem, _| {
            let mut to_remove = HashSet::new();
            for (prop, decl) in &elem.borrow().property_declarations {
                if !decl.expose_in_public_api && !elem.borrow().named_references.is_referenced(prop)
                {
                    to_remove.insert(prop.to_owned());
                }
            }
            let mut elem = elem.borrow_mut();
            for x in &to_remove {
                elem.property_declarations.remove(x);
                elem.property_analysis.borrow_mut().remove(x);
                elem.property_animations.remove(x);
                elem.bindings.remove(x);
            }
        },
    )
}
