/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::path::Path;

use super::util::lookup_current_element_type;
use super::DocumentCache;
use lsp_types::{CompletionCapability, CompletionItem, CompletionItemKind, InsertTextFormat};
use sixtyfps_compilerlib::diagnostics::Spanned;
use sixtyfps_compilerlib::expression_tree::Expression;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::lookup::{LookupCtx, LookupObject};
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxKind, SyntaxToken};

pub(crate) fn completion_at(
    document_cache: &DocumentCache,
    token: SyntaxToken,
    offset: u32,
    client_caps: Option<&CompletionCapability>,
) -> Option<Vec<CompletionItem>> {
    let node = token.parent();

    if token.kind() == SyntaxKind::StringLiteral {
        if matches!(node.kind(), SyntaxKind::ImportSpecifier | SyntaxKind::AtImageUrl) {
            return complete_path_in_string(
                token.source_file()?.path(),
                token.text(),
                offset.checked_sub(token.text_range().start().into())?,
            );
        }
    } else if let Some(element) = syntax_nodes::Element::new(node.clone()) {
        return resolve_element_scope(element, document_cache).map(|mut r| {
            // add snipets
            for c in r.iter_mut() {
                if client_caps
                    .and_then(|caps| caps.completion_item.as_ref())
                    .and_then(|caps| caps.snippet_support)
                    .unwrap_or(false)
                {
                    c.insert_text_format = Some(InsertTextFormat::Snippet);
                    match c.kind {
                        Some(CompletionItemKind::Property) => {
                            c.insert_text = Some(format!("{}: $1;", c.label))
                        }
                        Some(CompletionItemKind::Method) => {
                            c.insert_text = Some(format!("{} => {{ $1 }}", c.label))
                        }
                        Some(CompletionItemKind::Class) => {
                            c.insert_text = Some(format!("{} {{ $1 }}", c.label))
                        }
                        _ => (),
                    }
                }
            }

            // add keywords
            r.extend(
                [
                    ("property", "property <$1> $2;"),
                    ("callback", "callback $1();"),
                    ("animate", "animate $1 { $2 }"),
                    ("states", "states [ $1 ]"),
                    ("transitions", "transitions [ $1 ]"),
                    ("for", "for $1 in $2: $3 {}"),
                    ("if", "if ($1) : $2 {}"),
                    ("@children", "@children"),
                ]
                .iter()
                .map(|(kw, ins_tex)| {
                    let mut c = CompletionItem::new_simple(kw.to_string(), String::new());
                    c.kind = Some(CompletionItemKind::Keyword);
                    with_insert_text(c, ins_tex, client_caps)
                }),
            );
            r
        });
    } else if let Some(n) = syntax_nodes::Binding::new(node.clone()) {
        if token.kind() != SyntaxKind::Identifier {
            return None;
        }
        let all = resolve_element_scope(syntax_nodes::Element::new(n.parent()?)?, document_cache)?;
        return Some(
            all.into_iter().filter(|ce| ce.kind == Some(CompletionItemKind::Property)).collect(),
        );
    } else if let Some(n) = syntax_nodes::TwoWayBinding::new(node.clone()) {
        if token.kind() != SyntaxKind::Identifier {
            return None;
        }
        let all = resolve_element_scope(syntax_nodes::Element::new(n.parent()?)?, document_cache)?;
        return Some(
            all.into_iter().filter(|ce| ce.kind == Some(CompletionItemKind::Property)).collect(),
        );
    } else if let Some(n) = syntax_nodes::CallbackConnection::new(node.clone()) {
        if token.kind() != SyntaxKind::Identifier {
            return None;
        }
        let all = resolve_element_scope(syntax_nodes::Element::new(n.parent()?)?, document_cache)?;
        return Some(
            all.into_iter().filter(|ce| ce.kind == Some(CompletionItemKind::Method)).collect(),
        );
    } else if matches!(
        node.kind(),
        SyntaxKind::Type | SyntaxKind::ArrayType | SyntaxKind::ObjectType | SyntaxKind::ReturnType
    ) {
        return resolve_type_scope(token, document_cache);
    } else if let Some(_) = syntax_nodes::PropertyDeclaration::new(node.clone()) {
        if token.kind() == SyntaxKind::LAngle {
            return resolve_type_scope(token, document_cache);
        }
    } else if let Some(n) = syntax_nodes::CallbackDeclaration::new(node.clone()) {
        let paren = n.child_token(SyntaxKind::LParent)?;
        if token.token.text_range().start() >= paren.token.text_range().end() {
            return resolve_type_scope(token, document_cache);
        }
    } else if matches!(
        node.kind(),
        SyntaxKind::BindingExpression
            | SyntaxKind::CodeBlock
            | SyntaxKind::ReturnStatement
            | SyntaxKind::Expression
            | SyntaxKind::FunctionCallExpression
            | SyntaxKind::SelfAssignment
            | SyntaxKind::ConditionalExpression
            | SyntaxKind::BinaryExpression
            | SyntaxKind::UnaryOpExpression
            | SyntaxKind::Array
    ) {
        return crate::util::with_lookup_ctx(document_cache, node, |ctx| {
            resolve_expression_scope(ctx)
        })?;
    } else if let Some(q) = syntax_nodes::QualifiedName::new(node.clone()) {
        match q.parent()?.kind() {
            SyntaxKind::Element => {
                // auto-complete the types
                let global_tr = document_cache.documents.global_type_registry.borrow();
                let tr = q
                    .source_file()
                    .and_then(|sf| document_cache.documents.get_document(sf.path()))
                    .map(|doc| &doc.local_registry)
                    .unwrap_or(&global_tr);
                return Some(
                    tr.all_types()
                        .into_iter()
                        .filter_map(|(k, t)| {
                            match t {
                                Type::Component(c) if !c.is_global() => (),
                                Type::Builtin(b) if !b.is_internal && !b.is_global => (),
                                _ => return None,
                            };
                            let mut c = CompletionItem::new_simple(k, "element".into());
                            c.kind = Some(CompletionItemKind::Class);
                            Some(c)
                        })
                        .collect(),
                );
            }
            SyntaxKind::Type => {
                return resolve_type_scope(token, document_cache);
            }
            SyntaxKind::Expression => {
                return crate::util::with_lookup_ctx(document_cache, node, |ctx| {
                    let it = q.children_with_tokens().filter_map(|t| t.into_token());
                    let mut it = it.skip_while(|t| {
                        t.kind() != SyntaxKind::Identifier && t.token != token.token
                    });
                    let first = it.next();
                    if first.as_ref().map_or(true, |f| f.token == token.token) {
                        return resolve_expression_scope(ctx);
                    }
                    let first = sixtyfps_compilerlib::parser::normalize_identifier(first?.text());
                    let global = sixtyfps_compilerlib::lookup::global_lookup();
                    let mut expr_it = global.lookup(ctx, &first)?.expression;
                    let mut has_dot = false;
                    loop {
                        let t = if let Some(t) = it.next() { t } else { break };
                        has_dot |= t.kind() == SyntaxKind::Dot;
                        if t.token == token.token {
                            break;
                        };
                        if t.kind() != SyntaxKind::Identifier {
                            continue;
                        }
                        has_dot = false;
                        let str = sixtyfps_compilerlib::parser::normalize_identifier(t.text());
                        expr_it = expr_it.lookup(ctx, &str)?.expression;
                    }
                    has_dot.then(|| {
                        let mut r = Vec::new();
                        expr_it.for_each_entry(ctx, &mut |str, expr| -> Option<()> {
                            r.push(completion_item_from_expression(str, expr));
                            None
                        });
                        r
                    })
                })?;
            }
            _ => (),
        }
    }
    return None;
}

fn with_insert_text(
    mut c: CompletionItem,
    ins_text: &str,
    client_caps: Option<&CompletionCapability>,
) -> CompletionItem {
    if client_caps
        .and_then(|caps| caps.completion_item.as_ref())
        .and_then(|caps| caps.snippet_support)
        .unwrap_or(false)
    {
        c.insert_text_format = Some(InsertTextFormat::Snippet);
        c.insert_text = Some(ins_text.to_string());
    }
    c
}

fn resolve_element_scope(
    element: syntax_nodes::Element,
    document_cache: &DocumentCache,
) -> Option<Vec<CompletionItem>> {
    let global_tr = document_cache.documents.global_type_registry.borrow();
    let tr = element
        .source_file()
        .and_then(|sf| document_cache.documents.get_document(sf.path()))
        .map(|doc| &doc.local_registry)
        .unwrap_or(&global_tr);
    let element_type = lookup_current_element_type((*element).clone(), tr).unwrap_or_default();
    Some(
        element_type
            .property_list()
            .into_iter()
            .map(|(k, t)| {
                let mut c = CompletionItem::new_simple(k, t.to_string());
                c.kind = Some(if matches!(t, Type::InferredCallback | Type::Callback { .. }) {
                    CompletionItemKind::Method
                } else {
                    CompletionItemKind::Property
                });
                c
            })
            .chain(element.PropertyDeclaration().map(|pr| {
                let mut c = CompletionItem::new_simple(
                    sixtyfps_compilerlib::parser::identifier_text(&pr.DeclaredIdentifier())
                        .unwrap_or_default(),
                    pr.Type().map(|t| t.text().into()).unwrap_or_else(|| "property".to_owned()),
                );
                c.kind = Some(CompletionItemKind::Property);
                c
            }))
            .chain(element.CallbackDeclaration().map(|cd| {
                let mut c = CompletionItem::new_simple(
                    sixtyfps_compilerlib::parser::identifier_text(&cd.DeclaredIdentifier())
                        .unwrap_or_default(),
                    "callback".into(),
                );
                c.kind = Some(CompletionItemKind::Method);
                c
            }))
            .chain(sixtyfps_compilerlib::typeregister::reserved_properties().filter_map(
                |(k, t)| {
                    if matches!(t, Type::Function { .. }) {
                        return None;
                    }
                    let mut c = CompletionItem::new_simple(k.into(), t.to_string());
                    c.kind = Some(if matches!(t, Type::InferredCallback | Type::Callback { .. }) {
                        CompletionItemKind::Method
                    } else {
                        CompletionItemKind::Property
                    });
                    Some(c)
                },
            ))
            .chain(tr.all_types().into_iter().filter_map(|(k, t)| {
                match t {
                    Type::Component(c) if !c.is_global() => (),
                    Type::Builtin(b) if !b.is_internal && !b.is_global => (),
                    _ => return None,
                };
                let mut c = CompletionItem::new_simple(k, "element".into());
                c.kind = Some(CompletionItemKind::Class);
                Some(c)
            }))
            .collect(),
    )
}

fn resolve_expression_scope(lookup_context: &LookupCtx) -> Option<Vec<CompletionItem>> {
    let mut r = Vec::new();
    let global = sixtyfps_compilerlib::lookup::global_lookup();
    global.for_each_entry(&lookup_context, &mut |str, expr| -> Option<()> {
        r.push(completion_item_from_expression(str, expr));
        None
    });
    Some(r)
}

fn completion_item_from_expression(str: &str, expr: Expression) -> CompletionItem {
    let mut c = CompletionItem::new_simple(str.to_string(), expr.ty().to_string());
    c.kind = match expr {
        Expression::BoolLiteral(_) => Some(CompletionItemKind::Constant),
        Expression::CallbackReference(_) => Some(CompletionItemKind::Method),
        Expression::PropertyReference(_) => Some(CompletionItemKind::Property),
        Expression::BuiltinFunctionReference(..) => Some(CompletionItemKind::Function),
        Expression::BuiltinMacroReference(..) => Some(CompletionItemKind::Function),
        Expression::ElementReference(_) => Some(CompletionItemKind::Class),
        Expression::RepeaterIndexReference { .. } => Some(CompletionItemKind::Variable),
        Expression::RepeaterModelReference { .. } => Some(CompletionItemKind::Variable),
        Expression::FunctionParameterReference { .. } => Some(CompletionItemKind::Variable),
        Expression::Cast { .. } => Some(CompletionItemKind::Constant),
        Expression::EasingCurve(_) => Some(CompletionItemKind::Constant),
        Expression::EnumerationValue(ev) => Some(if ev.value == usize::MAX {
            CompletionItemKind::Enum
        } else {
            CompletionItemKind::EnumMember
        }),
        _ => None,
    };
    c
}

fn resolve_type_scope(
    token: SyntaxToken,
    document_cache: &DocumentCache,
) -> Option<Vec<CompletionItem>> {
    let global_tr = document_cache.documents.global_type_registry.borrow();
    let tr = token
        .source_file()
        .and_then(|sf| document_cache.documents.get_document(sf.path()))
        .map(|doc| &doc.local_registry)
        .unwrap_or(&global_tr);
    Some(
        tr.all_types()
            .into_iter()
            .filter_map(|(k, t)| {
                t.is_property_type().then(|| {
                    let mut c = CompletionItem::new_simple(k, String::new());
                    c.kind = Some(CompletionItemKind::TypeParameter);
                    c
                })
            })
            .collect(),
    )
}

fn complete_path_in_string(base: &Path, text: &str, offset: u32) -> Option<Vec<CompletionItem>> {
    if offset as usize > text.len() || offset == 0 {
        return None;
    }
    let mut text = text.strip_prefix("\"")?;
    text = &text[..(offset - 1) as usize];
    let path = if let Some(last_slash) = text.rfind('/') {
        base.parent()?.join(Path::new(&text[..last_slash]))
    } else {
        base.parent()?.to_owned()
    };
    let dir = std::fs::read_dir(path).ok()?;
    Some(
        dir.filter_map(|x| {
            let entry = x.ok()?;
            let mut c =
                CompletionItem::new_simple(entry.file_name().into_string().ok()?, String::new());
            if entry.file_type().ok()?.is_dir() {
                c.kind = Some(CompletionItemKind::Folder);
                c.insert_text = Some(format!("{}/", c.label));
            } else {
                c.kind = Some(CompletionItemKind::File);
            }
            Some(c)
        })
        .collect(),
    )
}
