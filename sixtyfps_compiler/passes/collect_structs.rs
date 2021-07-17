/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Passes that fills the root component used_types.structs

use crate::expression_tree::Expression;
use crate::object_tree::*;
use crate::{diagnostics::BuildDiagnostics, langtype::Type};
use std::collections::BTreeMap;
use std::rc::Rc;

/// Fill the root_component's used_types.structs
pub fn collect_structs(root_component: &Rc<Component>, _diag: &mut BuildDiagnostics) {
    let mut hash = BTreeMap::new();

    let mut maybe_collect_object = |ty: &Type| {
        visit_named_object(ty, &mut |name, sub_ty| {
            hash.entry(name.clone()).or_insert_with(|| sub_ty.clone());
        });
    };

    recurse_elem_including_sub_components_no_borrow(root_component, &(), &mut |elem, _| {
        for x in elem.borrow().property_declarations.values() {
            maybe_collect_object(&x.property_type);
        }
    });

    visit_all_expressions(root_component, |expr, _| {
        expr.visit_recursive(&mut |expr| {
            if let Expression::Struct { ty, .. } = expr {
                maybe_collect_object(ty)
            }
        })
    });

    let mut used_types = root_component.used_types.borrow_mut();
    let used_struct = &mut used_types.structs;
    *used_struct = Vec::with_capacity(hash.len());
    while let Some(next) = hash.iter().next() {
        // Here, using BTreeMap::pop_first would be great when it is stable
        let key = next.0.clone();
        sort_struct(&mut hash, used_struct, &key);
    }
}

/// Move the object named `key` from hash to vector, making sure that all object used by
/// it are placed before in the vector
fn sort_struct(hash: &mut BTreeMap<String, Type>, vec: &mut Vec<Type>, key: &str) {
    let ty = if let Some(ty) = hash.remove(key) { ty } else { return };
    if let Type::Struct { fields, name: Some(name), .. } = &ty {
        if name.contains("::") {
            // This is a builtin type.
            // FIXME! there should be a better way to handle builtin struct
            return;
        }

        for sub_ty in fields.values() {
            visit_named_object(sub_ty, &mut |name, _| sort_struct(hash, vec, name));
        }
    }
    vec.push(ty)
}

fn visit_named_object(ty: &Type, visitor: &mut impl FnMut(&String, &Type)) {
    match ty {
        Type::Struct { fields, name, .. } => {
            if let Some(struct_name) = name.as_ref() {
                visitor(struct_name, ty);
            }
            for sub_ty in fields.values() {
                visit_named_object(sub_ty, visitor);
            }
        }
        Type::Array(x) => visit_named_object(x, visitor),
        Type::Callback { return_type, args } => {
            if let Some(rt) = return_type {
                visit_named_object(&rt, visitor);
            }
            for a in args {
                visit_named_object(a, visitor);
            }
        }
        _ => {}
    }
}
