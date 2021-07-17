/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! This module contains the implementation of the builtin macros.
//! They are just transformations that convert into some more complicated expression tree

use crate::diagnostics::{BuildDiagnostics, Spanned};
use crate::expression_tree::{
    BuiltinFunction, BuiltinMacroFunction, EasingCurve, Expression, Unit,
};
use crate::langtype::{EnumerationValue, Type};
use crate::parser::NodeOrToken;

/// Used for uniquely name some variables
static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// "Expand" the macro `mac` (at location `n`) with the arguments `sub_expr`
pub fn lower_macro(
    mac: BuiltinMacroFunction,
    n: Option<NodeOrToken>,
    mut sub_expr: impl Iterator<Item = (Expression, Option<NodeOrToken>)>,
    diag: &mut BuildDiagnostics,
) -> Expression {
    match mac {
        BuiltinMacroFunction::Min => min_max_macro(n, '<', sub_expr.collect(), diag),
        BuiltinMacroFunction::Max => min_max_macro(n, '>', sub_expr.collect(), diag),
        BuiltinMacroFunction::Debug => debug_macro(n, sub_expr.collect(), diag),
        BuiltinMacroFunction::CubicBezier => {
            let mut has_error = None;
            // FIXME: this is not pretty to be handling there.
            // Maybe "cubic_bezier" should be a function that is lowered later
            let mut a = || match sub_expr.next() {
                None => {
                    has_error.get_or_insert((n.clone(), "Not enough arguments"));
                    0.
                }
                Some((Expression::NumberLiteral(val, Unit::None), _)) => val as f32,
                Some((_, n)) => {
                    has_error.get_or_insert((
                        n,
                        "Arguments to cubic bezier curve must be number literal",
                    ));
                    0.
                }
            };
            let expr = Expression::EasingCurve(EasingCurve::CubicBezier(a(), a(), a(), a()));
            if let Some((_, n)) = sub_expr.next() {
                has_error.get_or_insert((n, "Too many argument for bezier curve"));
            }
            if let Some((n, msg)) = has_error {
                diag.push_error(msg.into(), &n);
            }

            expr
        }
        BuiltinMacroFunction::Rgb => rgb_macro(n, sub_expr.collect(), diag),
    }
}

fn min_max_macro(
    node: Option<NodeOrToken>,
    op: char,
    args: Vec<(Expression, Option<NodeOrToken>)>,
    diag: &mut BuildDiagnostics,
) -> Expression {
    if args.is_empty() {
        diag.push_error("Needs at least one argument".into(), &node);
        return Expression::Invalid;
    }
    let mut args = args.into_iter();
    let (mut base, arg_node) = args.next().unwrap();
    let ty = match base.ty() {
        Type::Float32 => Type::Float32,
        // In case there are other floats, we don't want to convert the result to int
        Type::Int32 => Type::Float32,
        Type::PhysicalLength => Type::PhysicalLength,
        Type::LogicalLength => Type::LogicalLength,
        Type::Duration => Type::Duration,
        Type::Angle => Type::Angle,
        Type::Percent => Type::Float32,
        _ => {
            diag.push_error("Invalid argument type".into(), &arg_node);
            return Expression::Invalid;
        }
    };
    for (next, arg_node) in args {
        let rhs = next.maybe_convert_to(ty.clone(), &arg_node, diag);
        base = min_max_expression(base, rhs, op);
    }
    base
}

fn rgb_macro(
    node: Option<NodeOrToken>,
    args: Vec<(Expression, Option<NodeOrToken>)>,
    diag: &mut BuildDiagnostics,
) -> Expression {
    if args.len() < 3 {
        diag.push_error("Needs 3 or 4 argument".into(), &node);
        return Expression::Invalid;
    }
    let mut arguments: Vec<_> = args
        .into_iter()
        .enumerate()
        .map(|(i, (expr, n))| {
            if i < 3 {
                if expr.ty() == Type::Percent {
                    Expression::BinaryExpression {
                        lhs: Box::new(expr.maybe_convert_to(Type::Float32, &n, diag)),
                        rhs: Box::new(Expression::NumberLiteral(255., Unit::None)),
                        op: '*',
                    }
                } else {
                    expr.maybe_convert_to(Type::Int32, &n, diag)
                }
            } else {
                expr.maybe_convert_to(Type::Float32, &n, diag)
            }
        })
        .collect();
    if arguments.len() < 4 {
        arguments.push(Expression::NumberLiteral(1., Unit::None))
    }
    Expression::FunctionCall {
        function: Box::new(Expression::BuiltinFunctionReference(
            BuiltinFunction::Rgb,
            node.as_ref().map(|t| t.to_source_location()),
        )),
        arguments,
        source_location: Some(node.to_source_location()),
    }
}

fn debug_macro(
    node: Option<NodeOrToken>,
    args: Vec<(Expression, Option<NodeOrToken>)>,
    diag: &mut BuildDiagnostics,
) -> Expression {
    let mut string = None;
    for (expr, node) in args {
        let val = to_debug_string(expr, node, diag);
        string = Some(match string {
            None => val,
            Some(string) => Expression::BinaryExpression {
                lhs: Box::new(string),
                op: '+',
                rhs: Box::new(Expression::BinaryExpression {
                    lhs: Box::new(Expression::StringLiteral(", ".into())),
                    op: '+',
                    rhs: Box::new(val),
                }),
            },
        });
    }
    let sl = node.map(|node| node.to_source_location());
    Expression::FunctionCall {
        function: Box::new(Expression::BuiltinFunctionReference(
            BuiltinFunction::Debug,
            sl.clone(),
        )),
        arguments: vec![string.unwrap_or_else(|| Expression::default_value_for_type(&Type::String))],
        source_location: sl,
    }
}

fn to_debug_string(
    expr: Expression,
    node: Option<NodeOrToken>,
    diag: &mut BuildDiagnostics,
) -> Expression {
    let ty = expr.ty();
    match &ty {
        Type::Invalid => Expression::Invalid,
        Type::Void
        | Type::InferredCallback
        | Type::InferredProperty
        | Type::Component(_)
        | Type::Builtin(_)
        | Type::Native(_)
        | Type::Callback { .. }
        | Type::Function { .. }
        | Type::ElementReference
        | Type::LayoutCache
        | Type::Model
        | Type::PathElements => {
            diag.push_error("Cannot debug this expression".into(), &node);
            Expression::Invalid
        }
        Type::Float32 | Type::Int32 => expr.maybe_convert_to(Type::String, &node, diag),
        Type::String => expr,
        // TODO
        Type::Color | Type::Brush | Type::Image | Type::Easing | Type::Array(_) => {
            Expression::StringLiteral("<debug-of-this-type-not-yet-implemented>".into())
        }
        Type::Duration
        | Type::PhysicalLength
        | Type::LogicalLength
        | Type::Angle
        | Type::Percent
        | Type::UnitProduct(_) => Expression::BinaryExpression {
            lhs: Box::new(
                Expression::Cast { from: Box::new(expr), to: Type::Float32 }.maybe_convert_to(
                    Type::String,
                    &node,
                    diag,
                ),
            ),
            op: '+',
            rhs: Box::new(Expression::StringLiteral(
                Type::UnitProduct(ty.as_unit_product().unwrap()).to_string(),
            )),
        },
        Type::Bool => Expression::Condition {
            condition: Box::new(expr),
            true_expr: Box::new(Expression::StringLiteral("true".into())),
            false_expr: Box::new(Expression::StringLiteral("false".into())),
        },
        Type::Struct { fields, .. } => {
            let local_object = format!(
                "debug_struct{}",
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let mut string = None;
            for (k, _) in fields {
                let field_name =
                    if string.is_some() { format!(", {}: ", k) } else { format!("{{ {}: ", k) };
                let value = to_debug_string(
                    Expression::StructFieldAccess {
                        base: Box::new(Expression::ReadLocalVariable {
                            name: local_object.clone(),
                            ty: ty.clone(),
                        }),
                        name: k.clone(),
                    },
                    node.clone(),
                    diag,
                );
                let field = Expression::BinaryExpression {
                    lhs: Box::new(Expression::StringLiteral(field_name)),
                    op: '+',
                    rhs: Box::new(value),
                };
                string = Some(match string {
                    None => field,
                    Some(x) => Expression::BinaryExpression {
                        lhs: Box::new(x),
                        op: '+',
                        rhs: Box::new(field),
                    },
                });
            }
            match string {
                None => Expression::StringLiteral("{}".into()),
                Some(string) => Expression::CodeBlock(vec![
                    Expression::StoreLocalVariable { name: local_object, value: Box::new(expr) },
                    Expression::BinaryExpression {
                        lhs: Box::new(string),
                        op: '+',
                        rhs: Box::new(Expression::StringLiteral(" }".into())),
                    },
                ]),
            }
        }
        Type::Enumeration(enu) => {
            let local_object = "debug_enum";
            let mut v = vec![Expression::StoreLocalVariable {
                name: local_object.into(),
                value: Box::new(expr),
            }];
            let mut cond = Expression::StringLiteral(format!("Error: invalid value for {}", ty));
            for (idx, val) in enu.values.iter().enumerate() {
                cond = Expression::Condition {
                    condition: Box::new(Expression::BinaryExpression {
                        lhs: Box::new(Expression::ReadLocalVariable {
                            name: local_object.into(),
                            ty: ty.clone(),
                        }),
                        rhs: Box::new(Expression::EnumerationValue(EnumerationValue {
                            value: idx,
                            enumeration: enu.clone(),
                        })),
                        op: '=',
                    }),
                    true_expr: Box::new(Expression::StringLiteral(val.clone())),
                    false_expr: Box::new(cond),
                };
            }
            v.push(Expression::BinaryExpression {
                lhs: Box::new(cond),
                op: '+',
                rhs: Box::new(Expression::StringLiteral(" }".into())),
            });
            Expression::CodeBlock(v)
        }
    }
}

/// Generate an expression which is like `min(lhs, rhs)` if op is '<' or `max(lhs, rhs)` if op is '>'.
/// counter is an unique id.
/// The rhs and lhs of the expression must have the same numerical type
pub fn min_max_expression(lhs: Expression, rhs: Expression, op: char) -> Expression {
    let ty = lhs.ty();
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n1 = format!("minmax_lhs{}", id);
    let n2 = format!("minmax_rhs{}", id);
    let a1 = Box::new(Expression::ReadLocalVariable { name: n1.clone(), ty: ty.clone() });
    let a2 = Box::new(Expression::ReadLocalVariable { name: n2.clone(), ty });
    Expression::CodeBlock(vec![
        Expression::StoreLocalVariable { name: n1, value: Box::new(lhs) },
        Expression::StoreLocalVariable { name: n2, value: Box::new(rhs) },
        Expression::Condition {
            condition: Box::new(Expression::BinaryExpression {
                lhs: a1.clone(),
                rhs: a2.clone(),
                op,
            }),
            true_expr: a1,
            false_expr: a2,
        },
    ])
}
