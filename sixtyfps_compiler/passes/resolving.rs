/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Passes that resolve the property binding expression.
//!
//! Before this pass, all the expression are of type Expression::Uncompiled,
//! and there should no longer be Uncompiled expression after this pass.
//!
//! Most of the code for the resolving actually lies in the expression_tree module

use crate::diagnostics::{BuildDiagnostics, Spanned};
use crate::expression_tree::*;
use crate::langtype::{PropertyLookupResult, Type};
use crate::lookup::{LookupCtx, LookupObject};
use crate::object_tree::*;
use crate::parser::{identifier_text, syntax_nodes, NodeOrToken, SyntaxKind, SyntaxNode};
use crate::typeregister::TypeRegister;
use std::collections::HashMap;
use std::rc::Rc;

/// This represents a scope for the Component, where Component is the repeated component, but
/// does not represent a component in the .60 file
#[derive(Clone)]
struct ComponentScope(Vec<ElementRc>);

fn resolve_expression(
    expr: &mut Expression,
    property_name: Option<&str>,
    property_type: Type,
    scope: &ComponentScope,
    type_register: &TypeRegister,
    type_loader: &crate::typeloader::TypeLoader,
    diag: &mut BuildDiagnostics,
) {
    if let Expression::Uncompiled(node) = expr {
        let mut lookup_ctx = LookupCtx {
            property_name,
            property_type,
            component_scope: &scope.0,
            diag,
            arguments: vec![],
            type_register,
            type_loader: Some(type_loader),
            current_token: None,
        };

        let new_expr = match node.kind() {
            SyntaxKind::CallbackConnection => {
                //FIXME: proper callback support (node is a codeblock)
                Expression::from_callback_connection(node.clone().into(), &mut lookup_ctx)
            }
            SyntaxKind::Expression => {
                //FIXME again: this happen for non-binding expression (i.e: model)
                Expression::from_expression_node(node.clone().into(), &mut lookup_ctx)
                    .maybe_convert_to(lookup_ctx.property_type.clone(), node, diag)
            }
            SyntaxKind::BindingExpression => {
                Expression::from_binding_expression_node(node.clone(), &mut lookup_ctx)
            }
            SyntaxKind::TwoWayBinding => {
                if lookup_ctx.property_type == Type::Invalid {
                    // An attempt to resolve this already failed when trying to resolve the property type
                    assert!(diag.has_error());
                    return;
                }
                Expression::from_two_way_binding(node.clone().into(), &mut lookup_ctx)
            }
            _ => {
                debug_assert!(diag.has_error());
                Expression::Invalid
            }
        };
        *expr = new_expr;
    }
}

pub fn resolve_expressions(
    doc: &Document,
    type_loader: &crate::typeloader::TypeLoader,
    diag: &mut BuildDiagnostics,
) {
    for component in doc.inner_components.iter() {
        let scope = ComponentScope(vec![component.root_element.clone()]);

        recurse_elem(&component.root_element, &scope, &mut |elem, scope| {
            let mut new_scope = scope.clone();
            let mut is_repeated = elem.borrow().repeated.is_some();
            if is_repeated {
                new_scope.0.push(elem.clone())
            }
            new_scope.0.push(elem.clone());
            visit_element_expressions(elem, |expr, property_name, property_type| {
                if is_repeated {
                    // The first expression is always the model and it needs to be resolved with the parent scope
                    debug_assert!(elem.borrow().repeated.as_ref().is_none()); // should be none because it is taken by the visit_element_expressions function
                    resolve_expression(
                        expr,
                        property_name,
                        property_type(),
                        scope,
                        &doc.local_registry,
                        type_loader,
                        diag,
                    );
                    is_repeated = false;
                } else {
                    resolve_expression(
                        expr,
                        property_name,
                        property_type(),
                        &new_scope,
                        &doc.local_registry,
                        type_loader,
                        diag,
                    )
                }
            });
            new_scope.0.pop();
            new_scope
        })
    }
}

impl Expression {
    pub fn from_binding_expression_node(node: SyntaxNode, ctx: &mut LookupCtx) -> Self {
        debug_assert_eq!(node.kind(), SyntaxKind::BindingExpression);
        let e = node
            .child_node(SyntaxKind::Expression)
            .map(|n| Self::from_expression_node(n.into(), ctx))
            .or_else(|| {
                node.child_node(SyntaxKind::CodeBlock)
                    .map(|c| Self::from_codeblock_node(c.into(), ctx))
            })
            .unwrap_or(Self::Invalid);
        if ctx.property_type == Type::LogicalLength && e.ty() == Type::Percent {
            // See if a conversion from percentage to length is allowed
            const RELATIVE_TO_PARENT_PROPERTIES: [&str; 2] = ["width", "height"];
            let property_name = ctx.property_name.unwrap_or_default();
            if RELATIVE_TO_PARENT_PROPERTIES.contains(&property_name) {
                return e;
            } else {
                ctx.diag.push_error(
                    format!(
                        "Automatic conversion from percentage to length is only possible for the properties {}",
                        RELATIVE_TO_PARENT_PROPERTIES.join(" and ")
                    ),
                    &node
                );
                return Expression::Invalid;
            }
        };
        e.maybe_convert_to(ctx.property_type.clone(), &node, &mut ctx.diag)
    }

    fn from_codeblock_node(node: syntax_nodes::CodeBlock, ctx: &mut LookupCtx) -> Expression {
        debug_assert_eq!(node.kind(), SyntaxKind::CodeBlock);

        let mut statements_or_exprs = node
            .children()
            .filter_map(|n| match n.kind() {
                SyntaxKind::Expression => Some(Self::from_expression_node(n.into(), ctx)),
                SyntaxKind::ReturnStatement => Some(Self::from_return_statement(n.into(), ctx)),
                _ => None,
            })
            .collect::<Vec<_>>();

        let exit_points_and_return_types = statements_or_exprs
            .iter()
            .enumerate()
            .filter_map(|(index, statement_or_expr)| {
                if index == statements_or_exprs.len()
                    || matches!(statement_or_expr, Expression::ReturnStatement(..))
                {
                    Some((index, statement_or_expr.ty()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let common_return_type = Self::common_target_type_for_type_list(
            exit_points_and_return_types.iter().map(|(_, ty)| ty.clone()),
        );

        exit_points_and_return_types.into_iter().for_each(|(index, _)| {
            let mut expr = std::mem::replace(&mut statements_or_exprs[index], Expression::Invalid);
            expr = expr.maybe_convert_to(common_return_type.clone(), &node, &mut ctx.diag);
            statements_or_exprs[index] = expr;
        });

        Expression::CodeBlock(statements_or_exprs)
    }

    fn from_return_statement(
        node: syntax_nodes::ReturnStatement,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let return_type = ctx.return_type().clone();
        Expression::ReturnStatement(node.Expression().map(|n| {
            Box::new(Self::from_expression_node(n, ctx).maybe_convert_to(
                return_type,
                &node,
                &mut ctx.diag,
            ))
        }))
    }

    fn from_callback_connection(
        node: syntax_nodes::CallbackConnection,
        ctx: &mut LookupCtx,
    ) -> Expression {
        ctx.arguments =
            node.DeclaredIdentifier().map(|x| identifier_text(&x).unwrap_or_default()).collect();
        Self::from_codeblock_node(node.CodeBlock(), ctx).maybe_convert_to(
            ctx.return_type().clone(),
            &node,
            &mut ctx.diag,
        )
    }

    pub fn from_two_way_binding(
        node: syntax_nodes::TwoWayBinding,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let e = Self::from_expression_node(node.Expression(), ctx);
        let ty = e.ty();
        match e {
            Expression::PropertyReference(n) => {
                if ty != ctx.property_type && ctx.property_type != Type::InferredProperty {
                    ctx.diag.push_error(
                        "The property does not have the same type as the bound property".into(),
                        &node,
                    );
                }
                Expression::TwoWayBinding(n, None)
            }
            Expression::CallbackReference(n) => {
                if ctx.property_type != Type::InferredCallback && ty != ctx.property_type {
                    ctx.diag.push_error("Cannot bind to a callback".into(), &node);
                    Expression::Invalid
                } else {
                    Expression::TwoWayBinding(n, None)
                }
            }
            _ => {
                ctx.diag.push_error(
                    "The expression in a two way binding must be a property reference".into(),
                    &node,
                );
                Expression::Invalid
            }
        }
    }

    fn from_expression_node(node: syntax_nodes::Expression, ctx: &mut LookupCtx) -> Self {
        node.Expression()
            .map(|n| Self::from_expression_node(n, ctx))
            .or_else(|| node.AtImageUrl().map(|n| Self::from_at_image_url_node(n, ctx)))
            .or_else(|| node.AtLinearGradient().map(|n| Self::from_at_linear_gradient(n, ctx)))
            .or_else(|| node.QualifiedName().map(|s| Self::from_qualified_name_node(s.into(), ctx)))
            .or_else(|| {
                node.child_text(SyntaxKind::StringLiteral).map(|s| {
                    crate::literals::unescape_string(&s).map(Self::StringLiteral).unwrap_or_else(
                        || {
                            ctx.diag.push_error("Cannot parse string literal".into(), &node);
                            Self::Invalid
                        },
                    )
                })
            })
            .or_else(|| {
                node.child_text(SyntaxKind::NumberLiteral)
                    .map(crate::literals::parse_number_literal)
                    .transpose()
                    .unwrap_or_else(|e| {
                        ctx.diag.push_error(e, &node);
                        Some(Self::Invalid)
                    })
            })
            .or_else(|| {
                node.child_text(SyntaxKind::ColorLiteral).map(|s| {
                    crate::literals::parse_color_literal(&s)
                        .map(|i| Expression::Cast {
                            from: Box::new(Expression::NumberLiteral(i as _, Unit::None)),
                            to: Type::Color,
                        })
                        .unwrap_or_else(|| {
                            ctx.diag.push_error("Invalid color literal".into(), &node);
                            Self::Invalid
                        })
                })
            })
            .or_else(|| {
                node.FunctionCallExpression().map(|n| Self::from_function_call_node(n, ctx))
            })
            .or_else(|| node.SelfAssignment().map(|n| Self::from_self_assignment_node(n, ctx)))
            .or_else(|| node.BinaryExpression().map(|n| Self::from_binary_expression_node(n, ctx)))
            .or_else(|| {
                node.UnaryOpExpression().map(|n| Self::from_unaryop_expression_node(n, ctx))
            })
            .or_else(|| {
                node.ConditionalExpression().map(|n| Self::from_conditional_expression_node(n, ctx))
            })
            .or_else(|| node.ObjectLiteral().map(|n| Self::from_object_literal_node(n, ctx)))
            .or_else(|| node.Array().map(|n| Self::from_array_node(n, ctx)))
            .or_else(|| node.CodeBlock().map(|n| Self::from_codeblock_node(n, ctx)))
            .or_else(|| node.StringTemplate().map(|n| Self::from_string_template_node(n, ctx)))
            .unwrap_or(Self::Invalid)
    }

    fn from_at_image_url_node(node: syntax_nodes::AtImageUrl, ctx: &mut LookupCtx) -> Self {
        let s = match node
            .child_text(SyntaxKind::StringLiteral)
            .and_then(|x| crate::literals::unescape_string(&x))
        {
            Some(s) => s,
            None => {
                ctx.diag.push_error("Cannot parse string literal".into(), &node);
                return Self::Invalid;
            }
        };

        if s.is_empty() {
            return Expression::ImageReference(ImageReference::None);
        }

        let absolute_source_path = {
            let path = std::path::Path::new(&s);
            if path.is_absolute() || s.starts_with("http://") || s.starts_with("https://") {
                s
            } else {
                ctx.type_loader
                    .map(|loader| {
                        loader
                            .resolve_import_path(Some(&(*node).clone().into()), &s)
                            .0
                            .to_string_lossy()
                            .to_string()
                    })
                    .unwrap_or_else(|| s.into())
            }
        };

        Expression::ImageReference(ImageReference::AbsolutePath(absolute_source_path))
    }

    fn from_at_linear_gradient(node: syntax_nodes::AtLinearGradient, ctx: &mut LookupCtx) -> Self {
        let mut subs = node
            .children_with_tokens()
            .filter(|n| matches!(n.kind(), SyntaxKind::Comma | SyntaxKind::Expression));
        let angle_expr = match subs.next() {
            Some(e) if e.kind() == SyntaxKind::Expression => {
                syntax_nodes::Expression::from(e.into_node().unwrap())
            }
            _ => {
                ctx.diag.push_error("Expected angle expression".into(), &node);
                return Expression::Invalid;
            }
        };
        if subs.next().map_or(false, |s| s.kind() != SyntaxKind::Comma) {
            ctx.diag
                .push_error("Angle expression must be an angle followed by a comma".into(), &node);
            return Expression::Invalid;
        }
        let angle =
            Box::new(Expression::from_expression_node(angle_expr.clone(), ctx).maybe_convert_to(
                Type::Angle,
                &angle_expr,
                &mut ctx.diag,
            ));

        let mut stops = vec![];
        enum Stop {
            Empty,
            Color(Expression),
            Finished,
        }
        let mut current_stop = Stop::Empty;
        for n in subs {
            if n.kind() == SyntaxKind::Comma {
                match std::mem::replace(&mut current_stop, Stop::Empty) {
                    Stop::Empty => {
                        ctx.diag.push_error("Expected expression".into(), &n);
                        break;
                    }
                    Stop::Finished => {}
                    Stop::Color(col) => stops.push((
                        col,
                        if stops.len() == 0 {
                            Expression::NumberLiteral(0., Unit::None)
                        } else {
                            Expression::Invalid
                        },
                    )),
                }
            } else {
                // To facilitate color literal conversion, adjust the expected return type.
                let e = {
                    let old_property_type = std::mem::replace(&mut ctx.property_type, Type::Color);
                    let e =
                        Expression::from_expression_node(n.as_node().unwrap().clone().into(), ctx);
                    ctx.property_type = old_property_type;
                    e
                };
                match std::mem::replace(&mut current_stop, Stop::Finished) {
                    Stop::Empty => {
                        current_stop =
                            Stop::Color(e.maybe_convert_to(Type::Color, &n, &mut ctx.diag))
                    }
                    Stop::Finished => {
                        ctx.diag.push_error("Expected comma".into(), &n);
                        break;
                    }
                    Stop::Color(col) => {
                        stops.push((col, e.maybe_convert_to(Type::Float32, &n, &mut ctx.diag)))
                    }
                }
            }
        }
        if let Stop::Color(col) = current_stop {
            stops.push((col, Expression::NumberLiteral(1., Unit::None)))
        };

        // Fix the stop so each has a position.
        let mut start = 0;
        while start < stops.len() {
            start += match stops[start..].iter().position(|s| matches!(s.1, Expression::Invalid)) {
                Some(p) => p,
                None => break,
            };
            let (before, rest) = stops.split_at_mut(start);
            let pos =
                rest.iter().position(|s| !matches!(s.1, Expression::Invalid)).unwrap_or(rest.len());
            if pos > 0 {
                let (middle, after) = rest.split_at_mut(pos);
                let begin = &before.last().expect("The first should never be invalid").1;
                let end = &after.last().expect("The last should never be invalid").1;
                for (i, (_, e)) in middle.iter_mut().enumerate() {
                    debug_assert!(matches!(e, Expression::Invalid));
                    // e = begin + (i+1) * (end - begin) / (pos+1)
                    *e = Expression::BinaryExpression {
                        lhs: Box::new(begin.clone()),
                        rhs: Box::new(Expression::BinaryExpression {
                            lhs: Box::new(Expression::BinaryExpression {
                                lhs: Box::new(Expression::NumberLiteral(i as f64 + 1., Unit::None)),
                                rhs: Box::new(Expression::BinaryExpression {
                                    lhs: Box::new(end.clone()),
                                    rhs: Box::new(begin.clone()),
                                    op: '-',
                                }),
                                op: '*',
                            }),
                            rhs: Box::new(Expression::NumberLiteral(pos as f64 + 1., Unit::None)),
                            op: '/',
                        }),
                        op: '+',
                    };
                }
            }
            start += pos + 1;
        }

        Expression::LinearGradient { angle, stops }
    }

    /// Perform the lookup
    fn from_qualified_name_node(node: SyntaxNode, ctx: &mut LookupCtx) -> Self {
        debug_assert_eq!(node.kind(), SyntaxKind::QualifiedName);

        let mut it = node
            .children_with_tokens()
            .filter(|n| n.kind() == SyntaxKind::Identifier)
            .filter_map(|n| n.into_token());

        let first = if let Some(first) = it.next() {
            first
        } else {
            // There must be at least one member (parser should ensure that)
            debug_assert!(ctx.diag.has_error());
            return Self::Invalid;
        };

        ctx.current_token = Some(first.clone().into());
        let first_str = crate::parser::normalize_identifier(first.text());
        let global_lookup = crate::lookup::global_lookup();
        let result = match global_lookup.lookup(ctx, &first_str) {
            None => {
                if let Some(minus_pos) = first.text().find('-') {
                    // Attempt to recover if the user wanted to write "-"
                    let first_str = &first.text()[0..minus_pos];
                    if global_lookup.lookup(ctx, first_str).is_some() {
                        ctx.diag.push_error(format!("Unknown unqualified identifier '{}'. Use space before the '-' if you meant a subtraction", first.text()), &node);
                        return Expression::Invalid;
                    }
                }

                if it.next().is_some() {
                    ctx.diag.push_error(format!("Cannot access id '{}'", first.text()), &node);
                } else {
                    ctx.diag.push_error(
                        format!("Unknown unqualified identifier '{}'", first.text()),
                        &node,
                    );
                }
                return Expression::Invalid;
            }
            Some(x) => x,
        };

        if let Some(depr) = result.deprecated {
            ctx.diag.push_property_deprecation_warning(&first_str, &depr, &first);
        }

        match result.expression {
            Expression::ElementReference(e) => {
                continue_lookup_within_element(&e.upgrade().unwrap(), &mut it, node, ctx)
            }
            r @ Expression::CallbackReference(..) => {
                if let Some(x) = it.next() {
                    ctx.diag.push_error("Cannot access fields of callback".into(), &x)
                }
                r
            }
            Expression::EnumerationValue(ev) => {
                // Special meaning for the type itself
                if ev.value == usize::MAX {
                    if let Some(next_identifier) = it.next() {
                        match ev.enumeration.lookup(
                            ctx,
                            &crate::parser::normalize_identifier(next_identifier.text()),
                        ) {
                            None => {
                                ctx.diag.push_error(
                                    format!(
                                        "'{}' is not a member of the enum {}",
                                        next_identifier.text(),
                                        ev.enumeration.name
                                    ),
                                    &next_identifier,
                                );
                                return Expression::Invalid;
                            }
                            Some(r) => return maybe_lookup_object(r.expression, it, ctx),
                        }
                    } else {
                        ctx.diag.push_error("Cannot take reference to an enum".to_string(), &node);
                        return Expression::Invalid;
                    }
                }
                maybe_lookup_object(Expression::EnumerationValue(ev), it, ctx)
            }
            other => maybe_lookup_object(other, it, ctx),
        }
    }

    fn from_function_call_node(
        node: syntax_nodes::FunctionCallExpression,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let mut sub_expr = node.Expression().map(|n| {
            (Self::from_expression_node(n.clone(), ctx), Some(NodeOrToken::from((*n).clone())))
        });

        let mut arguments = Vec::new();

        let (function, _) =
            sub_expr.next().unwrap_or_else(|| (Expression::Invalid, Some((*node).clone().into())));

        let function = match function {
            Expression::BuiltinMacroReference(mac, n) => {
                arguments.extend(sub_expr);
                return crate::builtin_macros::lower_macro(
                    mac,
                    n,
                    arguments.into_iter(),
                    &mut ctx.diag,
                );
            }
            Expression::MemberFunction { base, base_node, member } => {
                arguments.push((*base, base_node));
                member
            }
            _ => Box::new(function),
        };
        arguments.extend(sub_expr);

        let arguments = match function.ty() {
            Type::Function { args, .. } | Type::Callback { args, .. } => {
                if arguments.len() != args.len() {
                    ctx.diag.push_error(
                        format!(
                            "The callback or function expects {} arguments, but {} are provided",
                            args.len(),
                            arguments.len()
                        ),
                        &node,
                    );
                    arguments.into_iter().map(|x| x.0).collect()
                } else {
                    arguments
                        .into_iter()
                        .zip(args.iter())
                        .map(|((e, node), ty)| e.maybe_convert_to(ty.clone(), &node, &mut ctx.diag))
                        .collect()
                }
            }
            _ => {
                ctx.diag.push_error("The expression is not a function".into(), &node);
                arguments.into_iter().map(|x| x.0).collect()
            }
        };

        Expression::FunctionCall {
            function,
            arguments,
            source_location: Some(node.to_source_location()),
        }
    }

    fn from_self_assignment_node(
        node: syntax_nodes::SelfAssignment,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let (lhs_n, rhs_n) = node.Expression();
        let mut lhs = Self::from_expression_node(lhs_n, ctx);
        let op = None
            .or(node.child_token(SyntaxKind::PlusEqual).and(Some('+')))
            .or(node.child_token(SyntaxKind::MinusEqual).and(Some('-')))
            .or(node.child_token(SyntaxKind::StarEqual).and(Some('*')))
            .or(node.child_token(SyntaxKind::DivEqual).and(Some('/')))
            .or(node.child_token(SyntaxKind::Equal).and(Some('=')))
            .unwrap_or('_');
        if !lhs.try_set_rw() && lhs.ty() != Type::Invalid {
            ctx.diag.push_error(
                format!(
                    "{} needs to be done on a property",
                    if op == '=' { "Assignment" } else { "Self assignment" }
                ),
                &node,
            );
        }
        let rhs = Self::from_expression_node(rhs_n.clone(), ctx).maybe_convert_to(
            lhs.ty(),
            &rhs_n,
            &mut ctx.diag,
        );
        Expression::SelfAssignment { lhs: Box::new(lhs), rhs: Box::new(rhs), op }
    }

    fn from_binary_expression_node(
        node: syntax_nodes::BinaryExpression,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let op = None
            .or(node.child_token(SyntaxKind::Plus).and(Some('+')))
            .or(node.child_token(SyntaxKind::Minus).and(Some('-')))
            .or(node.child_token(SyntaxKind::Star).and(Some('*')))
            .or(node.child_token(SyntaxKind::Div).and(Some('/')))
            .or(node.child_token(SyntaxKind::LessEqual).and(Some('≤')))
            .or(node.child_token(SyntaxKind::GreaterEqual).and(Some('≥')))
            .or(node.child_token(SyntaxKind::LAngle).and(Some('<')))
            .or(node.child_token(SyntaxKind::RAngle).and(Some('>')))
            .or(node.child_token(SyntaxKind::EqualEqual).and(Some('=')))
            .or(node.child_token(SyntaxKind::NotEqual).and(Some('!')))
            .or(node.child_token(SyntaxKind::AndAnd).and(Some('&')))
            .or(node.child_token(SyntaxKind::OrOr).and(Some('|')))
            .unwrap_or('_');

        let (lhs_n, rhs_n) = node.Expression();
        let lhs = Self::from_expression_node(lhs_n.clone(), ctx);
        let rhs = Self::from_expression_node(rhs_n.clone(), ctx);

        let expected_ty = match operator_class(op) {
            OperatorClass::ComparisonOp => {
                Self::common_target_type_for_type_list([lhs.ty(), rhs.ty()].iter().cloned())
            }
            OperatorClass::LogicalOp => Type::Bool,
            OperatorClass::ArithmeticOp => {
                let (lhs_ty, rhs_ty) = (lhs.ty(), rhs.ty());
                if op == '+' && (lhs_ty == Type::String || rhs_ty == Type::String) {
                    Type::String
                } else if op == '+' || op == '-' {
                    if lhs_ty.default_unit().is_some() {
                        lhs_ty
                    } else if rhs_ty.default_unit().is_some() {
                        rhs_ty
                    } else if matches!(lhs_ty, Type::UnitProduct(_)) {
                        lhs_ty
                    } else if matches!(rhs_ty, Type::UnitProduct(_)) {
                        rhs_ty
                    } else {
                        Type::Float32
                    }
                } else if op == '*' || op == '/' {
                    let has_unit = |ty: &Type| {
                        matches!(ty, Type::UnitProduct(_)) || ty.default_unit().is_some()
                    };
                    match (has_unit(&lhs_ty), has_unit(&rhs_ty)) {
                        (true, true) => {
                            return Expression::BinaryExpression {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                                op,
                            }
                        }
                        (true, false) => {
                            return Expression::BinaryExpression {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs.maybe_convert_to(
                                    Type::Float32,
                                    &rhs_n,
                                    &mut ctx.diag,
                                )),
                                op,
                            }
                        }
                        (false, true) => {
                            return Expression::BinaryExpression {
                                lhs: Box::new(lhs.maybe_convert_to(
                                    Type::Float32,
                                    &lhs_n,
                                    &mut ctx.diag,
                                )),
                                rhs: Box::new(rhs),
                                op,
                            }
                        }
                        (false, false) => Type::Float32,
                    }
                } else {
                    unreachable!()
                }
            }
        };
        Expression::BinaryExpression {
            lhs: Box::new(lhs.maybe_convert_to(expected_ty.clone(), &lhs_n, &mut ctx.diag)),
            rhs: Box::new(rhs.maybe_convert_to(expected_ty, &rhs_n, &mut ctx.diag)),
            op,
        }
    }

    fn from_unaryop_expression_node(
        node: syntax_nodes::UnaryOpExpression,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let exp_n = node.Expression();
        let exp = Self::from_expression_node(exp_n, ctx);

        Expression::UnaryOp {
            sub: Box::new(exp),
            op: None
                .or(node.child_token(SyntaxKind::Plus).and(Some('+')))
                .or(node.child_token(SyntaxKind::Minus).and(Some('-')))
                .or(node.child_token(SyntaxKind::Bang).and(Some('!')))
                .unwrap_or('_'),
        }
    }

    fn from_conditional_expression_node(
        node: syntax_nodes::ConditionalExpression,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let (condition_n, true_expr_n, false_expr_n) = node.Expression();
        // FIXME: we should we add bool to the context
        let condition = Self::from_expression_node(condition_n.clone(), ctx).maybe_convert_to(
            Type::Bool,
            &condition_n,
            &mut ctx.diag,
        );
        let true_expr = Self::from_expression_node(true_expr_n.clone(), ctx);
        let false_expr = Self::from_expression_node(false_expr_n.clone(), ctx);
        let result_ty = Self::common_target_type_for_type_list(
            [true_expr.ty(), false_expr.ty()].iter().cloned(),
        );
        let true_expr = true_expr.maybe_convert_to(result_ty.clone(), &true_expr_n, &mut ctx.diag);
        let false_expr = false_expr.maybe_convert_to(result_ty, &false_expr_n, &mut ctx.diag);
        Expression::Condition {
            condition: Box::new(condition),
            true_expr: Box::new(true_expr),
            false_expr: Box::new(false_expr),
        }
    }

    fn from_object_literal_node(
        node: syntax_nodes::ObjectLiteral,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let values: HashMap<String, Expression> = node
            .ObjectMember()
            .map(|n| {
                (
                    identifier_text(&n).unwrap_or_default(),
                    Expression::from_expression_node(n.Expression(), ctx),
                )
            })
            .collect();
        let ty = Type::Struct {
            fields: values.iter().map(|(k, v)| (k.clone(), v.ty())).collect(),
            name: None,
            node: None,
        };
        Expression::Struct { ty, values }
    }

    fn from_array_node(node: syntax_nodes::Array, ctx: &mut LookupCtx) -> Expression {
        let mut values: Vec<Expression> =
            node.Expression().map(|e| Expression::from_expression_node(e, ctx)).collect();

        // FIXME: what's the type of an empty array ?
        let element_ty =
            Self::common_target_type_for_type_list(values.iter().map(|expr| expr.ty()));

        for e in values.iter_mut() {
            *e = core::mem::replace(e, Expression::Invalid).maybe_convert_to(
                element_ty.clone(),
                &node,
                ctx.diag,
            );
        }

        Expression::Array { element_ty, values }
    }

    fn from_string_template_node(
        node: syntax_nodes::StringTemplate,
        ctx: &mut LookupCtx,
    ) -> Expression {
        let mut exprs = node.Expression().map(|e| {
            Expression::from_expression_node(e.clone(), ctx).maybe_convert_to(
                Type::String,
                &e,
                ctx.diag,
            )
        });
        let mut result = exprs.next().unwrap_or_default();
        while let Some(x) = exprs.next() {
            result = Expression::BinaryExpression {
                lhs: Box::new(std::mem::take(&mut result)),
                rhs: Box::new(x),
                op: '+',
            }
        }
        result
    }

    /// This function is used to find a type that's suitable for casting each instance of a bunch of expressions
    /// to a type that captures most aspects. For example for an array of object literals the result is a merge of
    /// all seen fields.
    fn common_target_type_for_type_list(types: impl Iterator<Item = Type>) -> Type {
        types.fold(Type::Invalid, |target_type, expr_ty| {
            if target_type == expr_ty {
                target_type
            } else if target_type == Type::Invalid {
                expr_ty
            } else {
                match (target_type, expr_ty) {
                    (
                        Type::Struct {
                            fields: mut result_fields,
                            name: result_name,
                            node: result_node,
                        },
                        Type::Struct { fields: elem_fields, name: elem_name, node: elem_node },
                    ) => {
                        for (elem_name, elem_ty) in elem_fields.into_iter() {
                            match result_fields.entry(elem_name) {
                                std::collections::btree_map::Entry::Vacant(free_entry) => {
                                    free_entry.insert(elem_ty);
                                }
                                std::collections::btree_map::Entry::Occupied(
                                    mut existing_field,
                                ) => {
                                    *existing_field.get_mut() =
                                        Self::common_target_type_for_type_list(
                                            [existing_field.get().clone(), elem_ty].iter().cloned(),
                                        );
                                }
                            }
                        }
                        Type::Struct {
                            name: result_name.or(elem_name),
                            fields: result_fields,
                            node: result_node.or(elem_node),
                        }
                    }
                    (target_type, expr_ty) => {
                        if expr_ty.can_convert(&target_type) {
                            target_type
                        } else if target_type.can_convert(&expr_ty)
                            || (expr_ty.default_unit().is_some()
                                && matches!(target_type, Type::Float32 | Type::Int32))
                        {
                            // in the or case: The `0` literal.
                            expr_ty
                        } else {
                            // otherwise, use the target type and let further conversion report an error
                            target_type
                        }
                    }
                }
            }
        })
    }
}

fn continue_lookup_within_element(
    elem: &ElementRc,
    it: &mut impl Iterator<Item = crate::parser::SyntaxToken>,
    node: SyntaxNode,
    ctx: &mut LookupCtx,
) -> Expression {
    let second = if let Some(second) = it.next() {
        second
    } else if matches!(ctx.property_type, Type::ElementReference) {
        return Expression::ElementReference(Rc::downgrade(elem));
    } else {
        ctx.diag.push_error("Cannot take reference of an element".into(), &node);
        return Expression::Invalid;
    };
    let prop_name = crate::parser::normalize_identifier(second.text());

    let PropertyLookupResult { resolved_name, property_type } =
        elem.borrow().lookup_property(&prop_name);
    if property_type.is_property_type() {
        if resolved_name != prop_name {
            ctx.diag.push_property_deprecation_warning(&prop_name, &resolved_name, &second);
        }
        let prop = Expression::PropertyReference(NamedReference::new(elem, &resolved_name));
        maybe_lookup_object(prop, it, ctx)
    } else if matches!(property_type, Type::Callback { .. }) {
        if let Some(x) = it.next() {
            ctx.diag.push_error("Cannot access fields of callback".into(), &x)
        }
        Expression::CallbackReference(NamedReference::new(elem, &resolved_name))
    } else if matches!(property_type, Type::Function { .. }) {
        let member = elem.borrow().base_type.lookup_member_function(&resolved_name);
        Expression::MemberFunction {
            base: Box::new(Expression::ElementReference(Rc::downgrade(elem))),
            base_node: Some(node.into()),
            member: Box::new(member),
        }
    } else {
        let mut err = |extra: &str| {
            let what = match &elem.borrow().base_type {
                Type::Void => {
                    let global = elem.borrow().enclosing_component.upgrade().unwrap();
                    assert!(global.is_global());
                    format!("'{}'", global.id)
                }
                Type::Component(c) => format!("Element '{}'", c.id),
                Type::Builtin(b) => format!("Element '{}'", b.name),
                _ => {
                    assert!(ctx.diag.has_error());
                    return;
                }
            };
            ctx.diag.push_error(
                format!("{} does not have a property '{}'{}", what, second.text(), extra),
                &second,
            );
        };
        if let Some(minus_pos) = second.text().find('-') {
            // Attempt to recover if the user wanted to write "-"
            if elem.borrow().lookup_property(&second.text()[0..minus_pos]).property_type
                != Type::Invalid
            {
                err(". Use space before the '-' if you meant a subtraction");
                return Expression::Invalid;
            }
        }
        err("");
        Expression::Invalid
    }
}

fn maybe_lookup_object(
    mut base: Expression,
    it: impl Iterator<Item = crate::parser::SyntaxToken>,
    ctx: &mut LookupCtx,
) -> Expression {
    for next in it {
        let next_str = crate::parser::normalize_identifier(next.text());
        ctx.current_token = Some(next.clone().into());
        match base.lookup(ctx, &next_str) {
            Some(result) => {
                base = result.expression;
            }
            None => {
                if let Some(minus_pos) = next.text().find('-') {
                    if base.lookup(ctx, &next.text()[0..minus_pos]).is_some() {
                        ctx.diag.push_error(format!("Cannot access the field '{}'. Use space before the '-' if you meant a subtraction", next.text()), &next);
                        return Expression::Invalid;
                    }
                }
                let ty_descr = match base.ty() {
                    Type::Struct { .. } => String::new(),
                    ty => format!(" of {}", ty),
                };
                ctx.diag.push_error(
                    format!("Cannot access the field '{}'{}", next.text(), ty_descr),
                    &next,
                );
                return Expression::Invalid;
            }
        }
    }
    base
}
