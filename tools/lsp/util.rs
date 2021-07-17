/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use sixtyfps_compilerlib::diagnostics::Spanned;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::lookup::LookupCtx;
use sixtyfps_compilerlib::object_tree;
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxKind, SyntaxNode};
use sixtyfps_compilerlib::typeregister::TypeRegister;

use crate::DocumentCache;

/// Given a node within an element, return the Type for the Element under that node.
/// (If node is an element, return the Type for that element, otherwise the type of the element under it)
/// Will return `Foo` in the following example where `|` is the cursor.
///
/// ```text
/// Hello := A {
///   B {
///      Foo {
///        |
///      }
///   }
/// }
/// ```
pub fn lookup_current_element_type(mut node: SyntaxNode, tr: &TypeRegister) -> Option<Type> {
    while node.kind() != SyntaxKind::Element {
        if let Some(parent) = node.parent() {
            node = parent
        } else {
            return None;
        }
    }
    let parent = node
        .parent()
        .and_then(|parent| lookup_current_element_type(parent, tr))
        .unwrap_or_default();
    let qualname = object_tree::QualifiedTypeName::from_node(
        syntax_nodes::Element::from(node).QualifiedName()?,
    );
    parent.lookup_type_for_child_element(&qualname.to_string(), tr).ok()
}

/// Run the function with the LoookupCtx associated with the token
pub fn with_lookup_ctx<R>(
    document_cache: &DocumentCache,
    node: SyntaxNode,
    f: impl FnOnce(&mut LookupCtx) -> R,
) -> Option<R> {
    let (element, prop_name) = lookup_expression_context(node.clone())?;
    let global_tr = document_cache.documents.global_type_registry.borrow();
    let tr = node
        .source_file()
        .and_then(|sf| document_cache.documents.get_document(sf.path()))
        .map(|doc| &doc.local_registry)
        .unwrap_or(&global_tr);

    let ty = element
        .PropertyDeclaration()
        .find_map(|p| {
            (sixtyfps_compilerlib::parser::identifier_text(&p.DeclaredIdentifier())? == prop_name)
                .then(|| p)
        })
        .and_then(|p| p.Type())
        .map(|n| object_tree::type_from_node(n, &mut Default::default(), tr))
        .or_else(|| {
            lookup_current_element_type((*element).clone(), tr)
                .map(|el_ty| el_ty.lookup_property(&prop_name).property_type)
        });

    // FIXME: we need also to fill in the repeated element
    let component = {
        let mut n = element.parent()?;
        loop {
            if let Some(component) = syntax_nodes::Component::new(n.clone()) {
                break component;
            }
            n = n.parent()?;
        }
    };

    let component = sixtyfps_compilerlib::parser::identifier_text(&component.DeclaredIdentifier())
        .map(|component_name| tr.lookup(&component_name))?;
    let scope =
        if let Type::Component(c) = component { vec![c.root_element.clone()] } else { Vec::new() };

    let mut build_diagnostics = Default::default();
    let mut lookup_context = LookupCtx::empty_context(tr, &mut build_diagnostics);
    lookup_context.property_name = Some(&prop_name);
    lookup_context.property_type = ty.unwrap_or_default();
    lookup_context.component_scope = &scope;
    Some(f(&mut lookup_context))
}

/// Return the element and property name in which we are
fn lookup_expression_context(mut n: SyntaxNode) -> Option<(syntax_nodes::Element, String)> {
    let (element, prop_name) = loop {
        if let Some(decl) = syntax_nodes::PropertyDeclaration::new(n.clone()) {
            let prop_name =
                sixtyfps_compilerlib::parser::identifier_text(&decl.DeclaredIdentifier())?;
            let element = syntax_nodes::Element::new(n.parent()?)?;
            break (element, prop_name.to_string());
        }
        match n.kind() {
            SyntaxKind::Binding
            | SyntaxKind::TwoWayBinding
            // FIXME: arguments of the callback
            | SyntaxKind::CallbackConnection => {
                let prop_name = sixtyfps_compilerlib::parser::identifier_text(&n)?;
                let element = syntax_nodes::Element::new(n.parent()?)?;
                break (element, prop_name.to_string());
            }
            SyntaxKind::ConditionalElement | SyntaxKind::RepeatedElement => {
                let element = syntax_nodes::Element::new(n.parent()?)?;
                break (element, "$model".to_string());
            }
            SyntaxKind::Element => {
                // oops: missed it
                let element = syntax_nodes::Element::new(n)?;
                break (element, String::new());
            }
            _ => n = n.parent()?,

        }
    };
    Some((element, prop_name))
}

pub fn to_lsp_diag(d: &sixtyfps_compilerlib::diagnostics::Diagnostic) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic::new(
        to_range(d.line_column()),
        Some(to_lsp_diag_level(d.level())),
        None,
        None,
        d.message().to_owned(),
        None,
        None,
    )
}

fn to_range(span: (usize, usize)) -> lsp_types::Range {
    let pos = lsp_types::Position::new(
        (span.0 as u32).saturating_sub(1),
        (span.1 as u32).saturating_sub(1),
    );
    lsp_types::Range::new(pos, pos)
}

fn to_lsp_diag_level(
    level: sixtyfps_compilerlib::diagnostics::DiagnosticLevel,
) -> lsp_types::DiagnosticSeverity {
    match level {
        sixtyfps_interpreter::DiagnosticLevel::Error => lsp_types::DiagnosticSeverity::Error,
        sixtyfps_interpreter::DiagnosticLevel::Warning => lsp_types::DiagnosticSeverity::Warning,
    }
}
