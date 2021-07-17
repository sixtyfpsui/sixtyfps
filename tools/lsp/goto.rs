/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::path::Path;

use super::DocumentCache;
use lsp_types::{GotoDefinitionResponse, LocationLink, Range, Url};
use sixtyfps_compilerlib::diagnostics::Spanned;
use sixtyfps_compilerlib::expression_tree::Expression;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::lookup::LookupObject;
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxKind, SyntaxNode, SyntaxToken};

pub fn goto_definition(
    document_cache: &mut DocumentCache,
    token: SyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let mut node = token.parent();
    loop {
        if let Some(n) = syntax_nodes::QualifiedName::new(node.clone()) {
            let parent = n.parent()?;
            return match parent.kind() {
                SyntaxKind::Element | SyntaxKind::Type => {
                    let qual = sixtyfps_compilerlib::object_tree::QualifiedTypeName::from_node(n);
                    let doc = document_cache.documents.get_document(node.source_file.path())?;
                    match doc.local_registry.lookup_qualified(&qual.members) {
                        sixtyfps_compilerlib::langtype::Type::Component(c) => {
                            goto_node(document_cache, &*c.root_element.borrow().node.as_ref()?)
                        }
                        sixtyfps_compilerlib::langtype::Type::Struct {
                            node: Some(node), ..
                        } => goto_node(document_cache, node.parent().as_ref()?),
                        _ => None,
                    }
                }
                SyntaxKind::Expression => {
                    if token.kind() != SyntaxKind::Identifier {
                        return None;
                    }
                    let expr = crate::util::with_lookup_ctx(document_cache, node, |ctx| {
                        let mut it = n
                            .children_with_tokens()
                            .filter_map(|t| t.into_token())
                            .filter(|t| t.kind() == SyntaxKind::Identifier);
                        let mut cur_tok = it.next()?;
                        let first_str =
                            sixtyfps_compilerlib::parser::normalize_identifier(cur_tok.text());
                        let global = sixtyfps_compilerlib::lookup::global_lookup();
                        let mut expr_it = global.lookup(ctx, &first_str)?.expression;
                        while cur_tok.token != token.token {
                            cur_tok = it.next()?;
                            let str =
                                sixtyfps_compilerlib::parser::normalize_identifier(cur_tok.text());
                            expr_it = expr_it.lookup(ctx, &str)?.expression;
                        }
                        Some(expr_it)
                    })?;
                    let gn = match expr? {
                        Expression::ElementReference(e) => {
                            e.upgrade()?.borrow().node.clone()?.into()
                        }
                        Expression::CallbackReference(nr) | Expression::PropertyReference(nr) => {
                            let mut el = nr.element();
                            loop {
                                if let Some(x) = el.borrow().property_declarations.get(nr.name()) {
                                    break (**x.node.as_ref()?).clone();
                                }
                                let base = el.borrow().base_type.clone();
                                if let Type::Component(c) = base {
                                    el = c.root_element.clone();
                                } else {
                                    return None;
                                }
                            }
                        }
                        _ => return None,
                    };
                    goto_node(document_cache, &gn)
                }
                _ => None,
            };
        } else if let Some(n) = syntax_nodes::ImportIdentifier::new(node.clone()) {
            let doc = document_cache.documents.get_document(node.source_file.path())?;
            let imp_name = sixtyfps_compilerlib::typeloader::ImportedName::from_node(n);
            return match doc.local_registry.lookup(&imp_name.internal_name) {
                sixtyfps_compilerlib::langtype::Type::Component(c) => {
                    goto_node(document_cache, &*c.root_element.borrow().node.as_ref()?)
                }
                _ => None,
            };
        } else if let Some(n) = syntax_nodes::ImportSpecifier::new(node.clone()) {
            let import_file = node
                .source_file
                .path()
                .parent()
                .unwrap_or(Path::new("/"))
                .join(n.child_text(SyntaxKind::StringLiteral)?.trim_matches('\"'));
            let import_file = dunce::canonicalize(&import_file).unwrap_or(import_file);
            let doc = document_cache.documents.get_document(&import_file)?;
            let doc_node = doc.node.clone()?;
            return goto_node(document_cache, &*doc_node);
        } else if let Some(_) = syntax_nodes::BindingExpression::new(node.clone()) {
            // don't fallback to the Binding
            return None;
        } else if let Some(n) = syntax_nodes::Binding::new(node.clone()) {
            if token.kind() != SyntaxKind::Identifier {
                return None;
            }
            let prop_name = token.text();
            let element = syntax_nodes::Element::new(n.parent()?)?;
            if let Some(p) = element.PropertyDeclaration().find_map(|p| {
                (sixtyfps_compilerlib::parser::identifier_text(&p.DeclaredIdentifier())?
                    == prop_name)
                    .then(|| p)
            }) {
                return goto_node(document_cache, &p);
            }
            let n = find_property_declaration_in_base(&document_cache, element, prop_name)?;
            return goto_node(document_cache, &n);
        } else if let Some(n) = syntax_nodes::TwoWayBinding::new(node.clone()) {
            if token.kind() != SyntaxKind::Identifier {
                return None;
            }
            let prop_name = token.text();
            if prop_name != n.child_text(SyntaxKind::Identifier)? {
                return None;
            }
            let element = syntax_nodes::Element::new(n.parent()?)?;
            if let Some(p) = element.PropertyDeclaration().find_map(|p| {
                (sixtyfps_compilerlib::parser::identifier_text(&p.DeclaredIdentifier())?
                    == prop_name)
                    .then(|| p)
            }) {
                return goto_node(document_cache, &p);
            }
            let n = find_property_declaration_in_base(&document_cache, element, prop_name)?;
            return goto_node(document_cache, &n);
        } else if let Some(n) = syntax_nodes::CallbackConnection::new(node.clone()) {
            if token.kind() != SyntaxKind::Identifier {
                return None;
            }
            let prop_name = token.text();
            if prop_name != n.child_text(SyntaxKind::Identifier)? {
                return None;
            }
            let element = syntax_nodes::Element::new(n.parent()?)?;
            if let Some(p) = element.CallbackDeclaration().find_map(|p| {
                (sixtyfps_compilerlib::parser::identifier_text(&p.DeclaredIdentifier())?
                    == prop_name)
                    .then(|| p)
            }) {
                return goto_node(document_cache, &p);
            }
            let n = find_property_declaration_in_base(&document_cache, element, prop_name)?;
            return goto_node(document_cache, &n);
        }
        node = node.parent()?;
    }
}

/// Try to lookup the property `prop_name` in the base of the given Element
fn find_property_declaration_in_base(
    document_cache: &DocumentCache,
    element: syntax_nodes::Element,
    prop_name: &str,
) -> Option<SyntaxNode> {
    let global_tr = document_cache.documents.global_type_registry.borrow();
    let tr = element
        .source_file()
        .and_then(|sf| document_cache.documents.get_document(sf.path()))
        .map(|doc| &doc.local_registry)
        .unwrap_or(&global_tr);

    let mut element_type = crate::util::lookup_current_element_type((*element).clone(), tr)?;
    while let Type::Component(com) = element_type {
        if let Some(p) = com.root_element.borrow().property_declarations.get(prop_name) {
            return p.node.as_ref().map(|x| (**x).clone());
        }
        element_type = com.root_element.borrow().base_type.clone();
    }
    None
}

fn goto_node(
    document_cache: &mut DocumentCache,
    node: &SyntaxNode,
) -> Option<GotoDefinitionResponse> {
    let path = node.source_file.path();
    let target_uri = Url::from_file_path(path).ok()?;
    let offset = node.span().offset as u32;
    let pos = document_cache.byte_offset_to_position(offset, &target_uri)?;
    let range = Range::new(pos, pos);
    Some(GotoDefinitionResponse::Link(vec![LocationLink {
        origin_selection_range: None,
        target_uri,
        target_range: range,
        target_selection_range: range,
    }]))
}
