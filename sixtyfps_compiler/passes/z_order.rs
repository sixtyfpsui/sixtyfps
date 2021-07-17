/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

/*! re-order the children by their z-order
*/

use std::rc::Rc;

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{Expression, Unit};
use crate::langtype::Type;
use crate::object_tree::{Component, ElementRc};

pub fn reorder_by_z_order(root_component: &Rc<Component>, diag: &mut BuildDiagnostics) {
    crate::object_tree::recurse_elem_including_sub_components(
        root_component,
        &(),
        &mut |elem: &ElementRc, _| {
            reorder_children_by_zorder(elem, diag);
        },
    )
}

fn reorder_children_by_zorder(
    elem: &Rc<std::cell::RefCell<crate::object_tree::Element>>,
    diag: &mut BuildDiagnostics,
) {
    // maps indexes to their z order
    let mut children_z_order = vec![];
    for (idx, child_elm) in elem.borrow().children.iter().enumerate() {
        let z = child_elm
            .borrow_mut()
            .bindings
            .remove("z")
            .and_then(|e| eval_const_expr(&e.expression, "z", &e, diag));
        let z = z.or_else(|| {
            child_elm.borrow().repeated.as_ref()?;
            if let Type::Component(c) = &child_elm.borrow().base_type {
                c.root_element
                    .borrow_mut()
                    .bindings
                    .remove("z")
                    .and_then(|e| eval_const_expr(&e.expression, "z", &e, diag))
            } else {
                None
            }
        });

        if let Some(z) = z {
            if children_z_order.is_empty() {
                for i in 0..idx {
                    children_z_order.push((i, 0.));
                }
            }
            children_z_order.push((idx, z));
        } else if !children_z_order.is_empty() {
            children_z_order.push((idx, 0.));
        }
    }

    if !children_z_order.is_empty() {
        children_z_order.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());

        let new_children = children_z_order
            .into_iter()
            .map(|(idx, _)| elem.borrow().children[idx].clone())
            .collect();
        elem.borrow_mut().children = new_children;
    }
}

fn eval_const_expr(
    expression: &Expression,
    name: &str,
    span: &dyn crate::diagnostics::Spanned,
    diag: &mut BuildDiagnostics,
) -> Option<f64> {
    match expression {
        Expression::NumberLiteral(v, Unit::None) => Some(*v),
        Expression::Cast { from, .. } => eval_const_expr(from, name, span, diag),
        Expression::UnaryOp { sub, op: '-' } => eval_const_expr(sub, name, span, diag).map(|v| -v),
        Expression::UnaryOp { sub, op: '+' } => eval_const_expr(sub, name, span, diag),
        _ => {
            diag.push_error(format!("'{}' must be an number literal", name), span);
            None
        }
    }
}
