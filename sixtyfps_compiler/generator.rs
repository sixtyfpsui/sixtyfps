/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
The module responsible for the code generation.

There is one sub module for every language
*/

use std::collections::HashSet;
use std::rc::{Rc, Weak};

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{BindingExpression, Expression};
use crate::namedreference::NamedReference;
use crate::object_tree::{Component, Document, ElementRc};

#[cfg(feature = "cpp")]
mod cpp;

#[cfg(feature = "rust")]
pub mod rust;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OutputFormat {
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "rust")]
    Rust,
    Interpreter,
}

impl OutputFormat {
    pub fn guess_from_extension(path: &std::path::Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            #[cfg(feature = "cpp")]
            Some("cpp") | Some("cxx") | Some("h") | Some("hpp") => Some(Self::Cpp),
            #[cfg(feature = "rust")]
            Some("rs") => Some(Self::Rust),
            _ => None,
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "cpp")]
            "cpp" => Ok(Self::Cpp),
            #[cfg(feature = "rust")]
            "rust" => Ok(Self::Rust),
            _ => Err(format!("Unknown outpout format {}", s)),
        }
    }
}

pub fn generate(
    format: OutputFormat,
    destination: &mut impl std::io::Write,
    doc: &Document,
    diag: &mut BuildDiagnostics,
) -> std::io::Result<()> {
    #![allow(unused_variables)]
    #![allow(unreachable_code)]
    match format {
        #[cfg(feature = "cpp")]
        OutputFormat::Cpp => {
            if let Some(output) = cpp::generate(doc, diag) {
                write!(destination, "{}", output)?;
            }
        }
        #[cfg(feature = "rust")]
        OutputFormat::Rust => {
            if let Some(output) = rust::generate(doc, diag) {
                write!(destination, "{}", output)?;
            }
        }
        OutputFormat::Interpreter => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unsupported output format: The interpreter is not a valid output format yet.",
            )); // Perhaps byte code in the future?
        }
    }
    Ok(())
}

/// Visit each item in order in which they should appear in the children tree array.
/// The parameter of the visitor are
///  1. the item
///  2. the first_children_offset,
///  3. the parent index
#[allow(dead_code)]
pub fn build_array_helper(component: &Component, mut visit_item: impl FnMut(&ElementRc, u32, u32)) {
    visit_item(&component.root_element, 1, 0);
    visit_children(&component.root_element, &mut 0, 1, &mut visit_item);

    fn sub_children_count(e: &ElementRc) -> usize {
        let mut count = e.borrow().children.len();
        for i in &e.borrow().children {
            count += sub_children_count(i);
        }
        count
    }

    fn visit_children(
        item: &ElementRc,
        index: &mut u32,
        children_offset: u32,
        visit_item: &mut impl FnMut(&ElementRc, u32, u32),
    ) {
        let mut offset = children_offset + item.borrow().children.len() as u32;

        for i in &item.borrow().children {
            visit_item(i, offset, *index);
            offset += sub_children_count(i) as u32;
        }

        *index += 1;

        let mut offset = children_offset + item.borrow().children.len() as u32;

        for e in &item.borrow().children {
            visit_children(e, index, offset, visit_item);
            offset += sub_children_count(e) as u32;
        }
    }
}

/// Will call the `handle_property` callback for every property that needs to be initialized.
/// This function makes sure to call them in order so that if constant binding need to access
/// constant properties, these are already initialized
pub fn handle_property_bindings_init(
    component: &Rc<Component>,
    mut handle_property: impl FnMut(&ElementRc, &str, &BindingExpression),
) {
    fn handle_property_inner(
        component: &Weak<Component>,
        elem: &ElementRc,
        prop_name: &str,
        binding_expression: &BindingExpression,
        handle_property: &mut impl FnMut(&ElementRc, &str, &BindingExpression),
        processed: &mut HashSet<NamedReference>,
    ) {
        let nr = NamedReference::new(elem, prop_name);
        if processed.contains(&nr) {
            return;
        }
        processed.insert(nr);
        if binding_expression.analysis.borrow().as_ref().map_or(false, |a| a.is_const) {
            // We must first handle all dependent properties in case it is a constant property
            binding_expression.expression.visit_recursive(&mut |e| match e {
                Expression::PropertyReference(nr) => {
                    let elem = nr.element();
                    if Weak::ptr_eq(&elem.borrow().enclosing_component, component) {
                        if let Some(be) = elem.borrow().bindings.get(nr.name()) {
                            handle_property_inner(
                                component,
                                &elem,
                                nr.name(),
                                be,
                                handle_property,
                                processed,
                            );
                        }
                    }
                }
                _ => {}
            })
        }
        handle_property(elem, prop_name, binding_expression);
    }

    let mut processed = HashSet::new();
    crate::object_tree::recurse_elem(&component.root_element, &(), &mut |elem: &ElementRc, ()| {
        for (prop_name, binding_expression) in &elem.borrow().bindings {
            handle_property_inner(
                &Rc::downgrade(component),
                elem,
                prop_name,
                binding_expression,
                &mut handle_property,
                &mut processed,
            );
        }
    });
}
