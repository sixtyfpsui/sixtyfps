/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Helper to do lookup in expressions

use std::rc::Rc;

use crate::diagnostics::{BuildDiagnostics, Spanned};
use crate::expression_tree::{
    BuiltinFunction, BuiltinMacroFunction, EasingCurve, Expression, Unit,
};
use crate::langtype::{Enumeration, EnumerationValue, Type};
use crate::namedreference::NamedReference;
use crate::object_tree::{find_parent_element, ElementRc};
use crate::parser::NodeOrToken;
use crate::typeregister::TypeRegister;

/// Contains information which allow to lookup identifier in expressions
pub struct LookupCtx<'a> {
    /// the name of the property for which this expression refers.
    pub property_name: Option<&'a str>,

    /// the type of the property for which this expression refers.
    /// (some property come in the scope)
    pub property_type: Type,

    /// Here is the stack in which id applies
    pub component_scope: &'a [ElementRc],

    /// Somewhere to report diagnostics
    pub diag: &'a mut BuildDiagnostics,

    /// The name of the arguments of the callback or function
    pub arguments: Vec<String>,

    /// The type register in which to look for Globals
    pub type_register: &'a TypeRegister,

    /// The type loader instance, which may be used to resolve relative path references
    /// for example for img!
    pub type_loader: Option<&'a crate::typeloader::TypeLoader<'a>>,

    /// The token currently processed
    pub current_token: Option<NodeOrToken>,
}

impl<'a> LookupCtx<'a> {
    /// Return a context that is just suitable to build simple const expression
    pub fn empty_context(type_register: &'a TypeRegister, diag: &'a mut BuildDiagnostics) -> Self {
        Self {
            property_name: Default::default(),
            property_type: Default::default(),
            component_scope: Default::default(),
            diag,
            arguments: Default::default(),
            type_register,
            type_loader: None,
            current_token: None,
        }
    }

    pub fn return_type(&self) -> &Type {
        if let Type::Callback { return_type, .. } = &self.property_type {
            return_type.as_ref().map_or(&Type::Void, |b| &(**b))
        } else {
            &self.property_type
        }
    }
}

#[derive(Default)]
pub struct LookupResult {
    pub expression: Expression,
    /// When set, this is deprecated, and the string is the deprecated name
    /// (the new name can be found in the expression's NamedReference)
    pub deprecated: Option<String>,
}

impl LookupResult {
    pub fn new(expression: Expression) -> Self {
        Self { expression, deprecated: None }
    }
}

/// Represent an object which has properties which can be accessible
pub trait LookupObject {
    /// Will call the function for each entry (useful for completion)
    /// If the function return Some, it will immediately be returned and not called further
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R>;

    /// Perform a lookup of a given identifier.
    /// One does not have to re-implement unless we can make it faster
    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        self.for_each_entry(ctx, &mut |prop, expr| (prop == name).then(|| LookupResult::new(expr)))
    }
}

impl<T1: LookupObject, T2: LookupObject> LookupObject for (T1, T2) {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        self.0.for_each_entry(ctx, f).or_else(|| self.1.for_each_entry(ctx, f))
    }

    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        self.0.lookup(ctx, name).or_else(|| self.1.lookup(ctx, name))
    }
}

struct ArgumentsLookup;
impl LookupObject for ArgumentsLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        let args = match &ctx.property_type {
            Type::Callback { args, .. } | Type::Function { args, .. } => args,
            _ => return None,
        };
        for (index, (name, ty)) in ctx.arguments.iter().zip(args.iter()).enumerate() {
            if let Some(r) =
                f(name, Expression::FunctionParameterReference { index, ty: ty.clone() })
            {
                return Some(r);
            }
        }
        None
    }
}

struct SpecialIdLookup;
impl LookupObject for SpecialIdLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        let last = ctx.component_scope.last();
        None.or_else(|| f("self", Expression::ElementReference(Rc::downgrade(last?))))
            .or_else(|| {
                f(
                    "parent",
                    Expression::ElementReference(Rc::downgrade(&find_parent_element(last?)?)),
                )
            })
            .or_else(|| f("true", Expression::BoolLiteral(true)))
            .or_else(|| f("false", Expression::BoolLiteral(false)))
        // "root" is just a normal id
    }
}

struct IdLookup;
impl LookupObject for IdLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        fn visit<R>(
            roots: &[ElementRc],
            f: &mut impl FnMut(&str, Expression) -> Option<R>,
        ) -> Option<R> {
            for e in roots.iter().rev() {
                if !e.borrow().id.is_empty() {
                    if let Some(r) =
                        f(&e.borrow().id, Expression::ElementReference(Rc::downgrade(e)))
                    {
                        return Some(r);
                    }
                }
                for x in &e.borrow().children {
                    if x.borrow().repeated.is_some() {
                        continue;
                    }
                    if let Some(r) = visit(&[x.clone()], f) {
                        return Some(r);
                    }
                }
            }
            None
        }
        visit(ctx.component_scope, f)
    }
    // TODO: hash based lookup
}

struct InScopeLookup;
impl LookupObject for InScopeLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        for elem in ctx.component_scope.iter().rev() {
            if let Some(repeated) = &elem.borrow().repeated {
                if !repeated.index_id.is_empty() {
                    if let Some(r) = f(
                        &repeated.index_id,
                        Expression::RepeaterIndexReference { element: Rc::downgrade(elem) },
                    ) {
                        return Some(r);
                    }
                }
                if !repeated.model_data_id.is_empty() {
                    if let Some(r) = f(
                        &repeated.model_data_id,
                        Expression::RepeaterIndexReference { element: Rc::downgrade(elem) },
                    ) {
                        return Some(r);
                    }
                }
            }

            if let Some(r) = elem.for_each_entry(ctx, f) {
                return Some(r);
            }
        }
        None
    }

    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        if name.is_empty() {
            return None;
        }
        for elem in ctx.component_scope.iter().rev() {
            if let Some(repeated) = &elem.borrow().repeated {
                if repeated.index_id == name {
                    return Some(LookupResult::new(Expression::RepeaterIndexReference {
                        element: Rc::downgrade(elem),
                    }));
                }
                if repeated.model_data_id == name {
                    return Some(LookupResult::new(Expression::RepeaterModelReference {
                        element: Rc::downgrade(elem),
                    }));
                }
            }

            if let Some(r) = elem.lookup(ctx, name) {
                return Some(r);
            }
        }
        None
    }
}

impl LookupObject for ElementRc {
    fn for_each_entry<R>(
        &self,
        _ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        for (name, prop) in &self.borrow().property_declarations {
            let e = expression_from_reference(NamedReference::new(self, name), &prop.property_type);
            if let Some(r) = f(name, e) {
                return Some(r);
            }
        }
        let list = self.borrow().base_type.property_list();
        for (name, ty) in list {
            let e = expression_from_reference(NamedReference::new(self, &name), &ty);
            if let Some(r) = f(&name, e) {
                return Some(r);
            }
        }
        for (name, ty) in crate::typeregister::reserved_properties() {
            let e = expression_from_reference(NamedReference::new(self, name), &ty);
            if let Some(r) = f(name, e) {
                return Some(r);
            }
        }
        None
    }

    fn lookup(&self, _ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        let crate::langtype::PropertyLookupResult { resolved_name, property_type } =
            self.borrow().lookup_property(name);
        (property_type != Type::Invalid).then(|| LookupResult {
            expression: expression_from_reference(
                NamedReference::new(self, &resolved_name),
                &property_type,
            ),
            deprecated: (resolved_name != name).then(|| resolved_name.to_string()),
        })
    }
}

fn expression_from_reference(n: NamedReference, ty: &Type) -> Expression {
    if matches!(ty, Type::Callback { .. }) {
        Expression::CallbackReference(n)
    } else {
        Expression::PropertyReference(n)
    }
}

/// Lookup for Globals and Enum.
/// Note: for enums, the expression's value is `usize::MAX`
struct LookupType;
impl LookupObject for LookupType {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        for (name, ty) in ctx.type_register.all_types() {
            if let Some(r) = Self::as_expr(ty).and_then(|e| f(&name, e)) {
                return Some(r);
            }
        }
        None
    }

    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        Self::as_expr(ctx.type_register.lookup(name)).map(LookupResult::new)
    }
}
impl LookupType {
    fn as_expr(ty: Type) -> Option<Expression> {
        match ty {
            Type::Component(c) if c.is_global() => {
                Some(Expression::ElementReference(Rc::downgrade(&c.root_element)))
            }
            Type::Enumeration(e) => Some(Expression::EnumerationValue(EnumerationValue {
                value: usize::MAX,
                enumeration: e,
            })),
            _ => None,
        }
    }
}

struct ReturnTypeSpecificLookup;
impl LookupObject for ReturnTypeSpecificLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        match ctx.return_type() {
            Type::Color => ColorSpecific.for_each_entry(ctx, f),
            Type::Brush => ColorSpecific.for_each_entry(ctx, f),
            Type::Easing => EasingSpecific.for_each_entry(ctx, f),
            Type::Enumeration(enumeration) => enumeration.clone().for_each_entry(ctx, f),
            _ => None,
        }
    }

    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        match ctx.return_type() {
            Type::Color => ColorSpecific.lookup(ctx, name),
            Type::Brush => ColorSpecific.lookup(ctx, name),
            Type::Easing => EasingSpecific.lookup(ctx, name),
            Type::Enumeration(enumeration) => enumeration.clone().lookup(ctx, name),
            _ => None,
        }
    }
}

struct ColorSpecific;
impl LookupObject for ColorSpecific {
    fn for_each_entry<R>(
        &self,
        _ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        for (name, c) in css_color_parser2::NAMED_COLORS.iter() {
            if let Some(r) = f(name, Self::as_expr(*c)) {
                return Some(r);
            }
        }
        None
    }
    fn lookup(&self, _ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        css_color_parser2::NAMED_COLORS.get(name).map(|c| LookupResult::new(Self::as_expr(*c)))
    }
}
impl ColorSpecific {
    fn as_expr(c: css_color_parser2::Color) -> Expression {
        let value =
            ((c.a as u32 * 255) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32);
        Expression::Cast {
            from: Box::new(Expression::NumberLiteral(value as f64, Unit::None)),
            to: Type::Color,
        }
    }
}

struct EasingSpecific;
impl LookupObject for EasingSpecific {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        use EasingCurve::CubicBezier;
        None.or_else(|| f("linear", Expression::EasingCurve(EasingCurve::Linear)))
            .or_else(|| f("ease", Expression::EasingCurve(CubicBezier(0.25, 0.1, 0.25, 1.0))))
            .or_else(|| f("ease_in", Expression::EasingCurve(CubicBezier(0.42, 0.0, 1.0, 1.0))))
            .or_else(|| {
                f("ease_in_out", Expression::EasingCurve(CubicBezier(0.42, 0.0, 0.58, 1.0)))
            })
            .or_else(|| f("ease_out", Expression::EasingCurve(CubicBezier(0.0, 0.0, 0.58, 1.0))))
            .or_else(|| {
                f(
                    "cubic_bezier",
                    Expression::BuiltinMacroReference(
                        BuiltinMacroFunction::CubicBezier,
                        ctx.current_token.clone(),
                    ),
                )
            })
    }
}

impl LookupObject for Rc<Enumeration> {
    fn for_each_entry<R>(
        &self,
        _ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        for (value, name) in self.values.iter().enumerate() {
            if let Some(r) = f(
                name,
                Expression::EnumerationValue(EnumerationValue { value, enumeration: self.clone() }),
            ) {
                return Some(r);
            }
        }
        None
    }
}

struct BuiltinFunctionLookup;
impl LookupObject for BuiltinFunctionLookup {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        use Expression::{BuiltinFunctionReference, BuiltinMacroReference};
        let t = &ctx.current_token;
        let sl = || t.as_ref().map(|t| t.to_source_location());
        None.or_else(|| f("debug", BuiltinMacroReference(BuiltinMacroFunction::Debug, t.clone())))
            .or_else(|| f("mod", BuiltinFunctionReference(BuiltinFunction::Mod, sl())))
            .or_else(|| f("round", BuiltinFunctionReference(BuiltinFunction::Round, sl())))
            .or_else(|| f("ceil", BuiltinFunctionReference(BuiltinFunction::Ceil, sl())))
            .or_else(|| f("floor", BuiltinFunctionReference(BuiltinFunction::Floor, sl())))
            .or_else(|| f("abs", BuiltinFunctionReference(BuiltinFunction::Abs, sl())))
            .or_else(|| f("sqrt", BuiltinFunctionReference(BuiltinFunction::Sqrt, sl())))
            .or_else(|| f("rgb", BuiltinMacroReference(BuiltinMacroFunction::Rgb, t.clone())))
            .or_else(|| f("rgba", BuiltinMacroReference(BuiltinMacroFunction::Rgb, t.clone())))
            .or_else(|| f("max", BuiltinMacroReference(BuiltinMacroFunction::Max, t.clone())))
            .or_else(|| f("min", BuiltinMacroReference(BuiltinMacroFunction::Min, t.clone())))
            .or_else(|| f("sin", BuiltinFunctionReference(BuiltinFunction::Sin, sl())))
            .or_else(|| f("cos", BuiltinFunctionReference(BuiltinFunction::Cos, sl())))
            .or_else(|| f("tan", BuiltinFunctionReference(BuiltinFunction::Tan, sl())))
            .or_else(|| f("asin", BuiltinFunctionReference(BuiltinFunction::ASin, sl())))
            .or_else(|| f("acos", BuiltinFunctionReference(BuiltinFunction::ACos, sl())))
            .or_else(|| f("atan", BuiltinFunctionReference(BuiltinFunction::ATan, sl())))
    }
}

pub fn global_lookup() -> impl LookupObject {
    (
        ArgumentsLookup,
        (
            SpecialIdLookup,
            (
                IdLookup,
                (InScopeLookup, (LookupType, (ReturnTypeSpecificLookup, BuiltinFunctionLookup))),
            ),
        ),
    )
}

impl LookupObject for Expression {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        match self {
            Expression::ElementReference(e) => e.upgrade().unwrap().for_each_entry(ctx, f),
            Expression::EnumerationValue(ev) => {
                if ev.value == usize::MAX {
                    ev.enumeration.for_each_entry(ctx, f)
                } else {
                    None
                }
            }
            _ => match self.ty() {
                Type::Struct { fields, .. } => {
                    for name in fields.keys() {
                        if let Some(r) = f(
                            name,
                            Expression::StructFieldAccess {
                                base: Box::new(self.clone()),
                                name: name.clone(),
                            },
                        ) {
                            return Some(r);
                        }
                    }
                    None
                }
                Type::Component(c) => c.root_element.for_each_entry(ctx, f),
                Type::String => StringExpression(self).for_each_entry(ctx, f),
                Type::Color => ColorExpression(self).for_each_entry(ctx, f),
                Type::Image => ImageExpression(self).for_each_entry(ctx, f),
                _ => None,
            },
        }
    }

    fn lookup(&self, ctx: &LookupCtx, name: &str) -> Option<LookupResult> {
        match self {
            Expression::ElementReference(e) => e.upgrade().unwrap().lookup(ctx, name),
            Expression::EnumerationValue(ev) => {
                if ev.value == usize::MAX {
                    ev.enumeration.lookup(ctx, name)
                } else {
                    None
                }
            }
            _ => match self.ty() {
                Type::Struct { fields, .. } => fields.contains_key(name).then(|| {
                    LookupResult::new(Expression::StructFieldAccess {
                        base: Box::new(self.clone()),
                        name: name.to_string(),
                    })
                }),
                Type::Component(c) => c.root_element.lookup(ctx, name),
                Type::String => StringExpression(self).lookup(ctx, name),
                Type::Color => ColorExpression(self).lookup(ctx, name),
                Type::Image => ImageExpression(self).lookup(ctx, name),
                _ => None,
            },
        }
    }
}

struct StringExpression<'a>(&'a Expression);
impl<'a> LookupObject for StringExpression<'a> {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        let member_function = |f: BuiltinFunction| Expression::MemberFunction {
            base: Box::new(self.0.clone()),
            base_node: ctx.current_token.clone(), // Note that this is not the base_node, but the function's node
            member: Box::new(Expression::BuiltinFunctionReference(
                f,
                ctx.current_token.as_ref().map(|t| t.to_source_location()),
            )),
        };
        None.or_else(|| f("is_float", member_function(BuiltinFunction::StringIsFloat)))
            .or_else(|| f("to_float", member_function(BuiltinFunction::StringToFloat)))
    }
}
struct ColorExpression<'a>(&'a Expression);
impl<'a> LookupObject for ColorExpression<'a> {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        let member_function = |f: BuiltinFunction| Expression::MemberFunction {
            base: Box::new(self.0.clone()),
            base_node: ctx.current_token.clone(), // Note that this is not the base_node, but the function's node
            member: Box::new(Expression::BuiltinFunctionReference(
                f,
                ctx.current_token.as_ref().map(|t| t.to_source_location()),
            )),
        };
        None.or_else(|| f("brighter", member_function(BuiltinFunction::ColorBrighter)))
            .or_else(|| f("darker", member_function(BuiltinFunction::ColorDarker)))
    }
}

struct ImageExpression<'a>(&'a Expression);
impl<'a> LookupObject for ImageExpression<'a> {
    fn for_each_entry<R>(
        &self,
        ctx: &LookupCtx,
        f: &mut impl FnMut(&str, Expression) -> Option<R>,
    ) -> Option<R> {
        let field_access = |f: &str| Expression::StructFieldAccess {
            base: Box::new(Expression::FunctionCall {
                function: Box::new(Expression::BuiltinFunctionReference(
                    BuiltinFunction::ImageSize,
                    ctx.current_token.as_ref().map(|t| t.to_source_location()),
                )),
                source_location: ctx.current_token.as_ref().map(|t| t.to_source_location()),
                arguments: vec![self.0.clone()],
            }),
            name: f.into(),
        };
        None.or_else(|| f("width", field_access("width")))
            .or_else(|| f("height", field_access("height")))
    }
}
