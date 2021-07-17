/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::io::Write;

use sixtyfps_compilerlib::object_tree;
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxNode};

/// Replace the 'color' type with 'brush', and the 'resource' type with 'image', and the 'logical_length' to 'length'
pub(crate) fn fold_node(
    node: &SyntaxNode,
    file: &mut impl Write,
    _state: &mut crate::State,
) -> std::io::Result<bool> {
    if let Some(type_node) = syntax_nodes::Type::new(node.clone()) {
        if let Some(qn) = type_node.QualifiedName() {
            match object_tree::QualifiedTypeName::from_node(qn).to_string().as_str() {
                "color" => {
                    return write!(file, "brush").map(|_| true);
                }
                "resource" => {
                    return write!(file, "image").map(|_| true);
                }
                "logical_length" | "logical-length" => {
                    return write!(file, "length").map(|_| true);
                }
                _ => (),
            }
        }
    };
    Ok(false)
}
