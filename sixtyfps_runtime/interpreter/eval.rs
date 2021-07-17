/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use crate::api::{Struct, Value};
use crate::dynamic_component::InstanceRef;
use core::convert::TryInto;
use core::iter::FromIterator;
use core::pin::Pin;
use corelib::graphics::{GradientStop, LinearGradientBrush, PathElement};
use corelib::items::{ItemRef, PropertyAnimation};
use corelib::rtti::AnimatedBindingKind;
use corelib::window::ComponentWindow;
use corelib::{Brush, Color, PathData, SharedString, SharedVector};
use sixtyfps_compilerlib::expression_tree::{
    BindingExpression, BuiltinFunction, EasingCurve, Expression, Path as ExprPath,
    PathElement as ExprPathElement,
};
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::object_tree::ElementRc;
use sixtyfps_corelib as corelib;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub trait ErasedPropertyInfo {
    fn get(&self, item: Pin<ItemRef>) -> Value;
    fn set(&self, item: Pin<ItemRef>, value: Value, animation: Option<PropertyAnimation>);
    fn set_binding(
        &self,
        item: Pin<ItemRef>,
        binding: Box<dyn Fn() -> Value>,
        animation: AnimatedBindingKind,
    );
    fn offset(&self) -> usize;

    /// Safety: Property2 must be a (pinned) pointer to a `Property<T>`
    /// where T is the same T as the one represented by this property.
    unsafe fn link_two_ways(&self, item: Pin<ItemRef>, property2: *const ());
}

impl<Item: vtable::HasStaticVTable<corelib::items::ItemVTable>> ErasedPropertyInfo
    for &'static dyn corelib::rtti::PropertyInfo<Item, Value>
{
    fn get(&self, item: Pin<ItemRef>) -> Value {
        (*self).get(ItemRef::downcast_pin(item).unwrap()).unwrap()
    }
    fn set(&self, item: Pin<ItemRef>, value: Value, animation: Option<PropertyAnimation>) {
        (*self).set(ItemRef::downcast_pin(item).unwrap(), value, animation).unwrap()
    }
    fn set_binding(
        &self,
        item: Pin<ItemRef>,
        binding: Box<dyn Fn() -> Value>,
        animation: AnimatedBindingKind,
    ) {
        (*self).set_binding(ItemRef::downcast_pin(item).unwrap(), binding, animation).unwrap();
    }
    fn offset(&self) -> usize {
        (*self).offset()
    }
    unsafe fn link_two_ways(&self, item: Pin<ItemRef>, property2: *const ()) {
        // Safety: ErasedPropertyInfo::link_two_ways and PropertyInfo::link_two_ways have the same safety requirement
        (*self).link_two_ways(ItemRef::downcast_pin(item).unwrap(), property2)
    }
}

pub trait ErasedCallbackInfo {
    fn call(&self, item: Pin<ItemRef>, args: &[Value]) -> Value;
    fn set_handler(&self, item: Pin<ItemRef>, handler: Box<dyn Fn(&[Value]) -> Value>);
}

impl<Item: vtable::HasStaticVTable<corelib::items::ItemVTable>> ErasedCallbackInfo
    for &'static dyn corelib::rtti::CallbackInfo<Item, Value>
{
    fn call(&self, item: Pin<ItemRef>, args: &[Value]) -> Value {
        (*self).call(ItemRef::downcast_pin(item).unwrap(), args).unwrap()
    }

    fn set_handler(&self, item: Pin<ItemRef>, handler: Box<dyn Fn(&[Value]) -> Value>) {
        (*self).set_handler(ItemRef::downcast_pin(item).unwrap(), handler).unwrap()
    }
}

impl corelib::rtti::ValueType for Value {}

#[derive(Copy, Clone)]
pub(crate) enum ComponentInstance<'a, 'id> {
    InstanceRef(InstanceRef<'a, 'id>),
    GlobalComponent(&'a Pin<Rc<dyn crate::global_component::GlobalComponent>>),
}

/// The local variable needed for binding evaluation
pub struct EvalLocalContext<'a, 'id> {
    local_variables: HashMap<String, Value>,
    function_arguments: Vec<Value>,
    pub(crate) component_instance: ComponentInstance<'a, 'id>,
    /// When Some, a return statement was executed and one must stop evaluating
    return_value: Option<Value>,
}

impl<'a, 'id> EvalLocalContext<'a, 'id> {
    pub fn from_component_instance(component: InstanceRef<'a, 'id>) -> Self {
        Self {
            local_variables: Default::default(),
            function_arguments: Default::default(),
            component_instance: ComponentInstance::InstanceRef(component),
            return_value: None,
        }
    }

    /// Create a context for a function and passing the arguments
    pub fn from_function_arguments(
        component: InstanceRef<'a, 'id>,
        function_arguments: Vec<Value>,
    ) -> Self {
        Self {
            component_instance: ComponentInstance::InstanceRef(component),
            function_arguments,
            local_variables: Default::default(),
            return_value: None,
        }
    }
}

/// Evaluate an expression and return a Value as the result of this expression
pub fn eval_expression(expression: &Expression, local_context: &mut EvalLocalContext) -> Value {
    if let Some(r) = &local_context.return_value {
        return r.clone();
    }
    match expression {
        Expression::Invalid => panic!("invalid expression while evaluating"),
        Expression::Uncompiled(_) => panic!("uncompiled expression while evaluating"),
        Expression::TwoWayBinding(..) => panic!("invalid expression while evaluating"),
        Expression::StringLiteral(s) => Value::String(s.into()),
        Expression::NumberLiteral(n, unit) => Value::Number(unit.normalize(*n)),
        Expression::BoolLiteral(b) => Value::Bool(*b),
        Expression::CallbackReference { .. } => panic!("callback in expression"),
        Expression::BuiltinFunctionReference(..) => panic!(
            "naked builtin function reference not allowed, should be handled by function call"
        ),
        Expression::ElementReference(_) => todo!("Element references are only supported in the context of built-in function calls at the moment"),
        Expression::MemberFunction { .. } => panic!("member function expressions must not appear in the code generator anymore"),
        Expression::BuiltinMacroReference { .. } => panic!("macro expressions must not appear in the code generator anymore"),
        Expression::PropertyReference(nr) => {
            load_property_helper(local_context.component_instance, &nr.element(), nr.name()).unwrap()
        }
        Expression::RepeaterIndexReference { element } => load_property_helper(local_context.component_instance,
            &element.upgrade().unwrap().borrow().base_type.as_component().root_element,
            "index",
        )
        .unwrap(),
        Expression::RepeaterModelReference { element } => load_property_helper(local_context.component_instance,
            &element.upgrade().unwrap().borrow().base_type.as_component().root_element,
            "model_data",
        )
        .unwrap(),
        Expression::FunctionParameterReference { index, .. } => {
            local_context.function_arguments[*index].clone()
        }
        Expression::StructFieldAccess { base, name } => {
            if let Value::Struct(o) = eval_expression(base, local_context) {
                o.get_field(name).cloned().unwrap_or(Value::Void)
            } else {
                Value::Void
            }
        }
        Expression::Cast { from, to } => {
            let v = eval_expression(&*from, local_context);
            match (v, to) {
                (Value::Number(n), Type::Int32) => Value::Number(n.round()),
                (Value::Number(n), Type::String) => {
                    Value::String(SharedString::from(format!("{}", n).as_str()))
                }
                (Value::Number(n), Type::Color) => Color::from_argb_encoded(n as u32).into(),
                (Value::Brush(brush), Type::Color) => brush.color().into(),
                (v, _) => v,
            }
        }
        Expression::CodeBlock(sub) => {
            let mut v = Value::Void;
            for e in sub {
                v = eval_expression(e, local_context);
                if let Some(r) = &local_context.return_value {
                    return r.clone();
                }
            }
            v
        }
        Expression::FunctionCall { function, arguments, source_location: _ } => match &**function {
            Expression::CallbackReference(nr) => {
                let args = arguments.iter().map(|e| eval_expression(e, local_context)).collect::<Vec<_>>();
                invoke_callback(local_context.component_instance, &nr.element(), nr.name(), &args).unwrap()
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::GetWindowScaleFactor, _) => {
                match local_context.component_instance {
                    ComponentInstance::InstanceRef(component) => Value::Number(window_ref(component).unwrap().scale_factor() as _),
                    ComponentInstance::GlobalComponent(_) => panic!("Cannot get the window from a global component"),
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Debug, _) => {
                let to_print: SharedString = eval_expression(&arguments[0], local_context).try_into().unwrap();
                corelib::debug_log!("{}", to_print);
                Value::Void
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Mod, _) => {
                let mut to_int = |e| -> i32 { eval_expression(e, local_context).try_into().unwrap() };
                Value::Number((to_int(&arguments[0]) % to_int(&arguments[1])) as _)
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Round, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.round())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Ceil, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.ceil())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Floor, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.floor())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Sqrt, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.sqrt())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Abs, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.abs())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Sin, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.to_radians().sin())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Cos, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.to_radians().cos())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Tan, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.to_radians().tan())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ASin, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.asin().to_degrees())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ACos, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.acos().to_degrees())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ATan, _) => {
                let x: f64 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                Value::Number(x.atan().to_degrees())
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::SetFocusItem, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to SetFocusItem")
                }
                let component = match  local_context.component_instance  {
                    ComponentInstance::InstanceRef(c) => c,
                    ComponentInstance::GlobalComponent(_) => panic!("Cannot access the focus item from a global component")
                };
                if let Expression::ElementReference(focus_item) = &arguments[0] {
                    generativity::make_guard!(guard);

                    let focus_item = focus_item.upgrade().unwrap();
                    let enclosing_component =
                        enclosing_component_for_element(&focus_item, component, guard);
                    let component_type = enclosing_component.component_type;

                    let item_info = &component_type.items[focus_item.borrow().id.as_str()];

                    let focus_item_comp = enclosing_component.self_weak().get().unwrap().upgrade().unwrap();

                    window_ref(component).unwrap().set_focus_item(&corelib::items::ItemRc::new(vtable::VRc::into_dyn(focus_item_comp), item_info.item_index()));
                    Value::Void
                } else {
                    panic!("internal error: argument to SetFocusItem must be an element")
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ShowPopupWindow, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to ShowPopupWindow")
                }
                let component = match  local_context.component_instance  {
                    ComponentInstance::InstanceRef(c) => c,
                    ComponentInstance::GlobalComponent(_) => panic!("Cannot show popup from a global component")
                };
                if let Expression::ElementReference(popup_window) = &arguments[0] {
                    let popup_window = popup_window.upgrade().unwrap();
                    let pop_comp = popup_window.borrow().enclosing_component.upgrade().unwrap();
                    let parent_component = pop_comp.parent_element.upgrade().unwrap().borrow().enclosing_component.upgrade().unwrap();
                    let popup_list = parent_component.popup_windows.borrow();
                    let popup = popup_list.iter().find(|p| Rc::ptr_eq(&p.component, &pop_comp)).unwrap();
                    let x = load_property_helper(local_context.component_instance, &popup.x.element(), popup.x.name()).unwrap();
                    let y = load_property_helper(local_context.component_instance, &popup.y.element(), popup.y.name()).unwrap();
                    crate::dynamic_component::show_popup(popup, x.try_into().unwrap(), y.try_into().unwrap(), component.borrow(), window_ref(component).unwrap());
                    Value::Void
                } else {
                    panic!("internal error: argument to SetFocusItem must be an element")
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::StringIsFloat, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to StringIsFloat")
                }
                if let Value::String(s) = eval_expression(&arguments[0], local_context) {
                    Value::Bool(<f64 as core::str::FromStr>::from_str(s.as_str()).is_ok())
                } else {
                    panic!("Argument not a string");
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::StringToFloat, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to StringToFloat")
                }
                if let Value::String(s) = eval_expression(&arguments[0], local_context) {
                    Value::Number(core::str::FromStr::from_str(s.as_str()).unwrap_or(0.))
                } else {
                    panic!("Argument not a string");
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ColorBrighter, _) => {
                if arguments.len() != 2 {
                    panic!("internal error: incorrect argument count to ColorBrighter")
                }
                if let Value::Brush(Brush::SolidColor(col)) = eval_expression(&arguments[0], local_context) {
                    if let Value::Number(factor) = eval_expression(&arguments[1], local_context) {
                        col.brighter(factor as _).into()
                    } else {
                        panic!("Second argument not a number");
                    }
                } else {
                    panic!("First argument not a color");
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ColorDarker, _) => {
                if arguments.len() != 2 {
                    panic!("internal error: incorrect argument count to ColorDarker")
                }
                if let Value::Brush(Brush::SolidColor(col)) = eval_expression(&arguments[0], local_context) {
                    if let Value::Number(factor) = eval_expression(&arguments[1], local_context) {
                        col.darker(factor as _).into()
                    } else {
                        panic!("Second argument not a number");
                    }
                } else {
                    panic!("First argument not a color");
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ImageSize, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to ColorDarker")
                }
                if let Value::Image(img) = eval_expression(&arguments[0], local_context) {
                    let size = img.size();
                    let values = std::array::IntoIter::new([
                        ("width".to_string(), Value::Number(size.width as f64)),
                        ("height".to_string(), Value::Number(size.height as f64)),
                    ]).collect();
                    Value::Struct(values)
                } else {
                    panic!("First argument not an image");
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::Rgb, _) => {
                let r: i32 = eval_expression(&arguments[0], local_context).try_into().unwrap();
                let g: i32 = eval_expression(&arguments[1], local_context).try_into().unwrap();
                let b: i32 = eval_expression(&arguments[2], local_context).try_into().unwrap();
                let a: f32 = eval_expression(&arguments[3], local_context).try_into().unwrap();
                let r: u8 = r.max(0).min(255) as u8;
                let g: u8 = g.max(0).min(255) as u8;
                let b: u8 = b.max(0).min(255) as u8;
                let a: u8 = (255. * a).max(0.).min(255.) as u8;
                Value::Brush(Brush::SolidColor(Color::from_argb_u8(a, r, g, b)))
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::ImplicitLayoutInfo(orient), _) => {
                let component = match  local_context.component_instance  {
                    ComponentInstance::InstanceRef(c) => c,
                    ComponentInstance::GlobalComponent(_) => panic!("Cannot access the implicit item size from a global component")
                };
                if let [Expression::ElementReference(item)] = arguments.as_slice() {
                    generativity::make_guard!(guard);

                    let item = item.upgrade().unwrap();
                    let enclosing_component =
                        enclosing_component_for_element(&item, component, guard);
                    let component_type = enclosing_component.component_type;
                    let item_info = &component_type.items[item.borrow().id.as_str()];
                    let item_ref = unsafe { item_info.item_from_component(enclosing_component.as_ptr()) };

                    let window = window_ref(component).unwrap();
                    item_ref.as_ref().layouting_info(crate::eval_layout::to_runtime(*orient), &window).into()
                } else {
                    panic!("internal error: incorrect arguments to ImplicitLayoutInfo {:?}", arguments);
                }
            }
            Expression::BuiltinFunctionReference(BuiltinFunction::RegisterCustomFontByPath, _) => {
                if arguments.len() != 1 {
                    panic!("internal error: incorrect argument count to RegisterCustomFontByPath")
                }
                if let Value::String(s) = eval_expression(&arguments[0], local_context) {
                    if let Some(err) = crate::register_font_from_path(&std::path::PathBuf::from(s.as_str())).err() {
                        corelib::debug_log!("Error loading custom font {}: {}", s.as_str(), err);
                    }
                    Value::Void
                } else {
                    panic!("Argument not a string");
                }
            }
            _ => panic!("call of something not a callback"),
        }
        Expression::SelfAssignment { lhs, rhs, op } => {
            let rhs = eval_expression(&**rhs, local_context);
            eval_assignment(lhs, *op, rhs, local_context);
            Value::Void
        }
        Expression::BinaryExpression { lhs, rhs, op } => {
            let lhs = eval_expression(&**lhs, local_context);
            let rhs = eval_expression(&**rhs, local_context);

            match (op, lhs, rhs) {
                ('+', Value::String(mut a), Value::String(b)) => { a.push_str(b.as_str()); Value::String(a) },
                ('+', Value::Number(a), Value::Number(b)) => Value::Number(a + b),
                ('+', a @ Value::Struct(_), b @ Value::Struct(_)) => {
                    let a : Option<corelib::layout::LayoutInfo> = a.try_into().ok();
                    let b : Option<corelib::layout::LayoutInfo> = b.try_into().ok();
                    if let (Some(a), Some(b)) = (a, b) {
                        a.merge(&b).into()
                    } else {
                        panic!("unsupported {:?} {} {:?}", a, op, b);
                    }
                }
                ('-', Value::Number(a), Value::Number(b)) => Value::Number(a - b),
                ('/', Value::Number(a), Value::Number(b)) => Value::Number(a / b),
                ('*', Value::Number(a), Value::Number(b)) => Value::Number(a * b),
                ('<', Value::Number(a), Value::Number(b)) => Value::Bool(a < b),
                ('>', Value::Number(a), Value::Number(b)) => Value::Bool(a > b),
                ('≤', Value::Number(a), Value::Number(b)) => Value::Bool(a <= b),
                ('≥', Value::Number(a), Value::Number(b)) => Value::Bool(a >= b),
                ('<', Value::String(a), Value::String(b)) => Value::Bool(a < b),
                ('>', Value::String(a), Value::String(b)) => Value::Bool(a > b),
                ('≤', Value::String(a), Value::String(b)) => Value::Bool(a <= b),
                ('≥', Value::String(a), Value::String(b)) => Value::Bool(a >= b),
                ('=', a, b) => Value::Bool(a == b),
                ('!', a, b) => Value::Bool(a != b),
                ('&', Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
                ('|', Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
                (op, lhs, rhs) => panic!("unsupported {:?} {} {:?}", lhs, op, rhs),
            }
        }
        Expression::UnaryOp { sub, op } => {
            let sub = eval_expression(&**sub, local_context);
            match (sub, op) {
                (Value::Number(a), '+') => Value::Number(a),
                (Value::Number(a), '-') => Value::Number(-a),
                (Value::Bool(a), '!') => Value::Bool(!a),
                (sub, op) => panic!("unsupported {} {:?}", op, sub),
            }
        }
        Expression::ImageReference(resource_ref) => {
            Value::Image(match resource_ref {
                sixtyfps_compilerlib::expression_tree::ImageReference::None => {
                    Ok(Default::default())
                }
                sixtyfps_compilerlib::expression_tree::ImageReference::AbsolutePath(path) => {
                    corelib::graphics::Image::load_from_path(std::path::Path::new(path))
                }
                sixtyfps_compilerlib::expression_tree::ImageReference::EmbeddedData(_) => panic!("Resource embedding is not supported by the interpreter")
            }.unwrap_or_else(|_| {
                eprintln!("Could not load image {:?}",resource_ref );
                Default::default()
            }))
        }
        Expression::Condition { condition, true_expr, false_expr } => {
            match eval_expression(&**condition, local_context).try_into()
                as Result<bool, _>
            {
                Ok(true) => eval_expression(&**true_expr, local_context),
                Ok(false) => eval_expression(&**false_expr, local_context),
                _ => local_context.return_value.clone().expect("conditional expression did not evaluate to boolean"),
            }
        }
        Expression::Array { values, .. } => Value::Array(
            values.iter().map(|e| eval_expression(e, local_context)).collect(),
        ),
        Expression::Struct { values, .. } => Value::Struct(
            values
                .iter()
                .map(|(k, v)| (k.clone(), eval_expression(v, local_context)))
                .collect(),
        ),
        Expression::PathElements { elements } => {
            Value::PathElements(convert_path(elements, local_context))
        }
        Expression::StoreLocalVariable { name, value } => {
            let value = eval_expression(value, local_context);
            local_context.local_variables.insert(name.clone(), value);
            Value::Void
        }
        Expression::ReadLocalVariable { name, .. } => {
            local_context.local_variables.get(name).unwrap().clone()
        }
        Expression::EasingCurve(curve) => Value::EasingCurve(match curve {
            EasingCurve::Linear => corelib::animations::EasingCurve::Linear,
            EasingCurve::CubicBezier(a, b, c, d) => {
                corelib::animations::EasingCurve::CubicBezier([*a, *b, *c, *d])
            }
        }),
        Expression::LinearGradient{angle, stops} => {
            let angle = eval_expression(angle, local_context);
            Value::Brush(Brush::LinearGradient(LinearGradientBrush::new(angle.try_into().unwrap(), stops.iter().map(|(color, stop)| {
                let color = eval_expression(color, local_context).try_into().unwrap();
                let position = eval_expression(stop, local_context).try_into().unwrap();
                GradientStop{ color, position }
            }))))
        }
        Expression::EnumerationValue(value) => {
            Value::EnumerationValue(value.enumeration.name.clone(), value.to_string())
        }
        Expression::ReturnStatement(x) => {
            let val = x.as_ref().map_or(Value::Void, |x| eval_expression(x, local_context));
            if local_context.return_value.is_none() {
                local_context.return_value = Some(val);
            }
            local_context.return_value.clone().unwrap()
        }
        Expression::LayoutCacheAccess { layout_cache_prop, index, repeater_index } => {
            let cache = load_property_helper(local_context.component_instance, &layout_cache_prop.element(), layout_cache_prop.name()).unwrap();
            if let Value::LayoutCache(cache) = cache {
                if let Some(ri) = repeater_index {
                    let offset : usize = eval_expression(ri, local_context).try_into().unwrap();
                    Value::Number(cache[(cache[*index] as usize) + offset * 2].into())
                } else {
                    Value::Number(cache[*index].into())
                }
            } else {
                panic!("invalid layout cache")
            }
        }
        Expression::ComputeLayoutInfo(lay, o) => crate::eval_layout::compute_layout_info(lay, *o, local_context),
        Expression::SolveLayout(lay, o) => crate::eval_layout::solve_layout(lay, *o, local_context),
    }
}

fn eval_assignment(lhs: &Expression, op: char, rhs: Value, local_context: &mut EvalLocalContext) {
    let eval = |lhs| match (lhs, &rhs, op) {
        (Value::String(ref mut a), Value::String(b), '+') => {
            a.push_str(b.as_str());
            Value::String(a.clone())
        }
        (Value::Number(a), Value::Number(b), '+') => Value::Number(a + b),
        (Value::Number(a), Value::Number(b), '-') => Value::Number(a - b),
        (Value::Number(a), Value::Number(b), '/') => Value::Number(a / b),
        (Value::Number(a), Value::Number(b), '*') => Value::Number(a * b),
        (lhs, rhs, op) => panic!("unsupported {:?} {} {:?}", lhs, op, rhs),
    };
    match lhs {
        Expression::PropertyReference(nr) => {
            let element = nr.element();
            generativity::make_guard!(guard);
            let enclosing_component = enclosing_component_instance_for_element(
                &element,
                local_context.component_instance,
                guard,
            );

            match enclosing_component {
                ComponentInstance::InstanceRef(enclosing_component) => {
                    if op == '=' {
                        store_property(enclosing_component, &element, nr.name(), rhs).unwrap();
                        return;
                    }

                    let component = element.borrow().enclosing_component.upgrade().unwrap();
                    if element.borrow().id == component.root_element.borrow().id {
                        if let Some(x) =
                            enclosing_component.component_type.custom_properties.get(nr.name())
                        {
                            unsafe {
                                let p = Pin::new_unchecked(
                                    &*enclosing_component.as_ptr().add(x.offset),
                                );
                                x.prop.set(p, eval(x.prop.get(p).unwrap()), None).unwrap();
                            }
                            return;
                        }
                    };
                    let item_info =
                        &enclosing_component.component_type.items[element.borrow().id.as_str()];
                    let item =
                        unsafe { item_info.item_from_component(enclosing_component.as_ptr()) };
                    let p = &item_info.rtti.properties[nr.name()];
                    p.set(item, eval(p.get(item)), None);
                }
                ComponentInstance::GlobalComponent(global) => {
                    let val =
                        if op == '=' { rhs } else { eval(global.as_ref().get_property(nr.name())) };
                    global.as_ref().set_property(nr.name(), val);
                }
            }
        }
        Expression::StructFieldAccess { base, name } => {
            if let Value::Struct(mut o) = eval_expression(base, local_context) {
                let mut r = o.get_field(name).unwrap().clone();
                r = if op == '=' { rhs } else { eval(std::mem::take(&mut r)) };
                o.set_field(name.to_owned(), r);
                eval_assignment(base, '=', Value::Struct(o), local_context)
            }
        }
        Expression::RepeaterModelReference { element } => {
            let element = element.upgrade().unwrap();
            let component_instance = match local_context.component_instance {
                ComponentInstance::InstanceRef(i) => i,
                ComponentInstance::GlobalComponent(_) => panic!("can't have repeater in global"),
            };
            generativity::make_guard!(g1);
            let enclosing_component =
                enclosing_component_for_element(&element, component_instance, g1);
            // we need a 'static Repeater component in order to call model_set_row_data, so get it.
            // Safety: This is the only 'static Id in scope.
            let static_guard =
                unsafe { generativity::Guard::new(generativity::Id::<'static>::new()) };
            let repeater = crate::dynamic_component::get_repeater_by_name(
                enclosing_component,
                element.borrow().id.as_str(),
                static_guard,
            );
            repeater.0.model_set_row_data(
                eval_expression(
                    &Expression::RepeaterIndexReference { element: Rc::downgrade(&element) },
                    local_context,
                )
                .try_into()
                .unwrap(),
                if op == '=' {
                    rhs
                } else {
                    eval(eval_expression(
                        &Expression::RepeaterModelReference { element: Rc::downgrade(&element) },
                        local_context,
                    ))
                },
            )
        }
        _ => panic!("typechecking should make sure this was a PropertyReference"),
    }
}

pub fn load_property(component: InstanceRef, element: &ElementRc, name: &str) -> Result<Value, ()> {
    load_property_helper(ComponentInstance::InstanceRef(component), element, name)
}

fn load_property_helper(
    component_instance: ComponentInstance,
    element: &ElementRc,
    name: &str,
) -> Result<Value, ()> {
    generativity::make_guard!(guard);
    match enclosing_component_instance_for_element(element, component_instance, guard) {
        ComponentInstance::InstanceRef(enclosing_component) => {
            let element = element.borrow();
            if element.id == element.enclosing_component.upgrade().unwrap().root_element.borrow().id
            {
                if let Some(x) = enclosing_component.component_type.custom_properties.get(name) {
                    return unsafe {
                        x.prop.get(Pin::new_unchecked(&*enclosing_component.as_ptr().add(x.offset)))
                    };
                }
            };
            let item_info = enclosing_component
                .component_type
                .items
                .get(element.id.as_str())
                .unwrap_or_else(|| panic!("Unknown element for {}.{}", element.id, name));
            core::mem::drop(element);
            let item = unsafe { item_info.item_from_component(enclosing_component.as_ptr()) };
            Ok(item_info.rtti.properties.get(name).ok_or(())?.get(item))
        }
        ComponentInstance::GlobalComponent(glob) => Ok(glob.as_ref().get_property(name)),
    }
}

pub fn store_property(
    component_instance: InstanceRef,
    element: &ElementRc,
    name: &str,
    value: Value,
) -> Result<(), ()> {
    generativity::make_guard!(guard);
    let enclosing_component = enclosing_component_for_element(element, component_instance, guard);
    let maybe_animation = crate::dynamic_component::animation_for_property(
        enclosing_component,
        &element.borrow(),
        name,
    );

    let component = element.borrow().enclosing_component.upgrade().unwrap();
    if element.borrow().id == component.root_element.borrow().id {
        if let Some(x) = enclosing_component.component_type.custom_properties.get(name) {
            unsafe {
                let p = Pin::new_unchecked(&*enclosing_component.as_ptr().add(x.offset));
                return x.prop.set(p, value, maybe_animation.as_animation());
            }
        }
    };
    let item_info = &enclosing_component.component_type.items[element.borrow().id.as_str()];
    let item = unsafe { item_info.item_from_component(enclosing_component.as_ptr()) };
    let p = &item_info.rtti.properties.get(name).ok_or(())?;
    p.set(item, value, maybe_animation.as_animation());
    Ok(())
}

pub(crate) fn invoke_callback(
    component_instance: ComponentInstance,
    element: &ElementRc,
    callback_name: &str,
    args: &[Value],
) -> Option<Value> {
    generativity::make_guard!(guard);
    match enclosing_component_instance_for_element(element, component_instance, guard) {
        ComponentInstance::InstanceRef(enclosing_component) => {
            let component_type = enclosing_component.component_type;
            let item_info = &component_type.items[element.borrow().id.as_str()];
            let item = unsafe { item_info.item_from_component(enclosing_component.as_ptr()) };

            if let Some(callback) = item_info.rtti.callbacks.get(callback_name) {
                Some(callback.call(item, args))
            } else if let Some(callback_offset) = component_type.custom_callbacks.get(callback_name)
            {
                let callback = callback_offset.apply(&*enclosing_component.instance);
                Some(callback.call(args))
            } else {
                None
            }
        }
        ComponentInstance::GlobalComponent(global) => {
            Some(global.as_ref().invoke_callback(callback_name, args))
        }
    }
}

fn root_component_instance<'a, 'old_id, 'new_id>(
    component: InstanceRef<'a, 'old_id>,
    guard: generativity::Guard<'new_id>,
) -> InstanceRef<'a, 'new_id> {
    if let Some(parent_offset) = component.component_type.parent_component_offset {
        let parent_component =
            if let Some(parent) = parent_offset.apply(&*component.instance.get_ref()) {
                *parent
            } else {
                panic!("invalid parent ptr");
            };
        // we need a 'static guard in order to be able to re-borrow with lifetime 'a.
        // Safety: This is the only 'static Id in scope.
        let static_guard = unsafe { generativity::Guard::new(generativity::Id::<'static>::new()) };
        root_component_instance(
            unsafe { InstanceRef::from_pin_ref(parent_component, static_guard) },
            guard,
        )
    } else {
        // Safety: new_id is an unique id
        unsafe {
            std::mem::transmute::<InstanceRef<'a, 'old_id>, InstanceRef<'a, 'new_id>>(component)
        }
    }
}

pub fn window_ref(component: InstanceRef) -> Option<ComponentWindow> {
    component.component_type.window_offset.apply(&*component.instance.get_ref()).clone()
}

/// Return the component instance which hold the given element.
/// Does not take in account the global component.
pub fn enclosing_component_for_element<'a, 'old_id, 'new_id>(
    element: &'a ElementRc,
    component: InstanceRef<'a, 'old_id>,
    guard: generativity::Guard<'new_id>,
) -> InstanceRef<'a, 'new_id> {
    let enclosing = &element.borrow().enclosing_component.upgrade().unwrap();
    if Rc::ptr_eq(enclosing, &component.component_type.original) {
        // Safety: new_id is an unique id
        unsafe {
            std::mem::transmute::<InstanceRef<'a, 'old_id>, InstanceRef<'a, 'new_id>>(component)
        }
    } else {
        assert!(!enclosing.is_global());
        let parent_component = component
            .component_type
            .parent_component_offset
            .unwrap()
            .apply(component.as_ref())
            .unwrap();
        generativity::make_guard!(new_guard);
        let parent_instance = unsafe { InstanceRef::from_pin_ref(parent_component, new_guard) };
        let parent_instance = unsafe {
            core::mem::transmute::<InstanceRef, InstanceRef<'a, 'static>>(parent_instance)
        };
        enclosing_component_for_element(element, parent_instance, guard)
    }
}

/// Return the component instance which hold the given element.
/// The difference with enclosing_component_for_element is that it takes the GlobalComponent into account.
pub(crate) fn enclosing_component_instance_for_element<'a, 'old_id, 'new_id>(
    element: &'a ElementRc,
    component_instance: ComponentInstance<'a, 'old_id>,
    guard: generativity::Guard<'new_id>,
) -> ComponentInstance<'a, 'new_id> {
    let enclosing = &element.borrow().enclosing_component.upgrade().unwrap();
    match component_instance {
        ComponentInstance::InstanceRef(component) => {
            if enclosing.is_global() && !component.component_type.original.is_global() {
                // we need a 'static guard in order to be able to borrow from `root` otherwise it does not work because of variance.
                // Safety: This is the only 'static Id in scope.
                let static_guard =
                    unsafe { generativity::Guard::new(generativity::Id::<'static>::new()) };
                let root = root_component_instance(component, static_guard);
                ComponentInstance::GlobalComponent(
                    &root.component_type.extra_data_offset.apply(&*root.instance.get_ref()).globals
                        [enclosing.root_element.borrow().id.as_str()],
                )
            } else {
                ComponentInstance::InstanceRef(enclosing_component_for_element(
                    element, component, guard,
                ))
            }
        }
        ComponentInstance::GlobalComponent(global) => {
            //assert!(Rc::ptr_eq(enclosing, &global.component));
            ComponentInstance::GlobalComponent(global)
        }
    }
}

pub fn new_struct_with_bindings<ElementType: 'static + Default + corelib::rtti::BuiltinItem>(
    bindings: &BTreeMap<String, BindingExpression>,
    local_context: &mut EvalLocalContext,
) -> ElementType {
    let mut element = ElementType::default();
    for (prop, info) in ElementType::fields::<Value>().into_iter() {
        if let Some(binding) = &bindings.get(prop) {
            let value = eval_expression(binding, local_context);
            info.set_field(&mut element, value).unwrap();
        }
    }
    element
}

fn convert_from_lyon_path<'a>(
    it: impl IntoIterator<Item = &'a lyon_path::Event<lyon_path::math::Point, lyon_path::math::Point>>,
) -> PathData {
    use corelib::graphics::PathEvent;
    use lyon_path::Event;

    let mut coordinates = Vec::new();

    let events = it
        .into_iter()
        .map(|event| match event {
            Event::Begin { at } => {
                coordinates.push(at);
                PathEvent::Begin
            }
            Event::Line { from, to } => {
                coordinates.push(from);
                coordinates.push(to);
                PathEvent::Line
            }
            Event::Quadratic { from, ctrl, to } => {
                coordinates.push(from);
                coordinates.push(ctrl);
                coordinates.push(to);
                PathEvent::Quadratic
            }
            Event::Cubic { from, ctrl1, ctrl2, to } => {
                coordinates.push(from);
                coordinates.push(ctrl1);
                coordinates.push(ctrl2);
                coordinates.push(to);
                PathEvent::Cubic
            }
            Event::End { close, .. } => {
                if *close {
                    PathEvent::EndClosed
                } else {
                    PathEvent::EndOpen
                }
            }
        })
        .collect::<Vec<_>>();

    PathData::Events(
        SharedVector::from(events.as_slice()),
        SharedVector::from_iter(coordinates.into_iter().cloned()),
    )
}

pub fn convert_path(path: &ExprPath, local_context: &mut EvalLocalContext) -> PathData {
    match path {
        ExprPath::Elements(elements) => PathData::Elements(SharedVector::<PathElement>::from_iter(
            elements.iter().map(|element| convert_path_element(element, local_context)),
        )),
        ExprPath::Events(events) => convert_from_lyon_path(events.iter()),
    }
}

fn convert_path_element(
    expr_element: &ExprPathElement,
    local_context: &mut EvalLocalContext,
) -> PathElement {
    match expr_element.element_type.native_class.class_name.as_str() {
        "MoveTo" => {
            PathElement::MoveTo(new_struct_with_bindings(&expr_element.bindings, local_context))
        }
        "LineTo" => {
            PathElement::LineTo(new_struct_with_bindings(&expr_element.bindings, local_context))
        }
        "ArcTo" => {
            PathElement::ArcTo(new_struct_with_bindings(&expr_element.bindings, local_context))
        }
        "CubicTo" => {
            PathElement::CubicTo(new_struct_with_bindings(&expr_element.bindings, local_context))
        }
        "QuadraticTo" => PathElement::QuadraticTo(new_struct_with_bindings(
            &expr_element.bindings,
            local_context,
        )),
        "Close" => PathElement::Close,
        _ => panic!(
            "Cannot create unsupported path element {}",
            expr_element.element_type.native_class.class_name
        ),
    }
}

/// Create a value suitable as the default value of a given type
pub fn default_value_for_type(ty: &Type) -> Value {
    match ty {
        Type::Float32 | Type::Int32 => Value::Number(0.),
        Type::String => Value::String(Default::default()),
        Type::Color | Type::Brush => Value::Brush(Default::default()),
        Type::Duration | Type::Angle | Type::PhysicalLength | Type::LogicalLength => {
            Value::Number(0.)
        }
        Type::Image => Value::Image(Default::default()),
        Type::Bool => Value::Bool(false),
        Type::Callback { .. } => Value::Void,
        Type::Struct { fields, .. } => Value::Struct(Struct::from_iter(
            fields.iter().map(|(n, t)| (n.clone(), default_value_for_type(t))),
        )),
        Type::Array(_) => Value::Array(Default::default()),
        Type::Percent => Value::Number(0.),
        Type::Enumeration(e) => {
            Value::EnumerationValue(e.name.clone(), e.values.get(e.default_value).unwrap().clone())
        }
        Type::Easing => Value::EasingCurve(Default::default()),
        Type::Void | Type::Invalid => Value::Void,
        Type::Model => Value::Void,
        Type::UnitProduct(_) => Value::Number(0.),
        Type::PathElements => Value::PathElements(Default::default()),
        Type::LayoutCache => Value::LayoutCache(Default::default()),
        Type::InferredProperty
        | Type::InferredCallback
        | Type::ElementReference
        | Type::Builtin(_)
        | Type::Component(_)
        | Type::Native(_)
        | Type::Function { .. } => {
            panic!("There can't be such property")
        }
    }
}
