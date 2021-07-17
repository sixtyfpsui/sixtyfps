/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::rc::Rc;

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{BuiltinFunction, Expression};
use crate::object_tree::{visit_all_expressions, Component};

/// Check the validity of expressions
///
/// - Make sure that there is no uncalled member function or macro
pub fn check_expressions(doc: &crate::object_tree::Document, diag: &mut BuildDiagnostics) {
    for component in &doc.inner_components {
        visit_all_expressions(component, |e, _| check_expression(component, e, diag));
    }
}

fn check_expression(component: &Rc<Component>, e: &Expression, diag: &mut BuildDiagnostics) {
    match e {
        Expression::MemberFunction { base_node, .. } => {
            diag.push_error("Member function must be called".into(), base_node);
        }
        Expression::BuiltinMacroReference(_, node) => {
            diag.push_error("Builtin function must be called".into(), node);
        }
        Expression::BuiltinFunctionReference(BuiltinFunction::GetWindowScaleFactor, loc) => {
            if component.is_global() {
                diag.push_error("Cannot convert between logical and physical length in a global component, because the scale factor is not known".into(), loc);
            }
        }
        _ => e.visit(|e| check_expression(component, e, diag)),
    }
}
