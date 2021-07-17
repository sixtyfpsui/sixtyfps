/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Passes that fills the root component used_types.globals

use by_address::ByAddress;

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::NamedReference;
use crate::object_tree::*;
use std::collections::HashSet;
use std::rc::Rc;

/// Fill the root_component's used_types.globals
pub fn collect_globals(root_component: &Rc<Component>, _diag: &mut BuildDiagnostics) {
    let mut set = HashSet::new();

    let mut maybe_collect_global = |nr: &mut NamedReference| {
        let element = nr.element();
        let global_component = element.borrow().enclosing_component.upgrade().unwrap();
        if global_component.is_global() {
            set.insert(ByAddress(global_component.clone()));
        }
    };
    visit_all_named_references(root_component, &mut maybe_collect_global);
    root_component.used_types.borrow_mut().globals = set.into_iter().map(|x| x.0).collect();
    root_component.used_types.borrow_mut().globals.sort_by(|a, b| a.id.cmp(&b.id));
}
