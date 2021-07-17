/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use core::cell::RefCell;
use neon::prelude::*;
use rand::RngCore;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_corelib::{ImageInner, SharedVector};

mod js_model;
mod persistent_context;

struct WrappedComponentType(Option<sixtyfps_interpreter::ComponentDefinition>);
struct WrappedComponentRc(Option<sixtyfps_interpreter::ComponentInstance>);

/// We need to do some gymnastic with closures to pass the ExecuteContext with the right lifetime
type GlobalContextCallback<'c> =
    dyn for<'b> Fn(&mut ExecuteContext<'b>, &persistent_context::PersistentContext<'b>) + 'c;
scoped_tls_hkt::scoped_thread_local!(static GLOBAL_CONTEXT:
    for <'a> &'a dyn for<'c> Fn(&'c GlobalContextCallback<'c>));

/// This function exists as a workaround so one can access the ExecuteContext from callback handler
fn run_scoped<'cx, T>(
    cx: &mut impl Context<'cx>,
    object_with_persistent_context: Handle<'cx, JsObject>,
    functor: impl FnOnce() -> Result<T, String>,
) -> NeonResult<T> {
    let persistent_context =
        persistent_context::PersistentContext::from_object(cx, object_with_persistent_context)?;
    Ok(cx
        .execute_scoped(|cx| {
            let cx = RefCell::new(cx);
            let cx_fn = move |callback: &GlobalContextCallback| {
                callback(&mut *cx.borrow_mut(), &persistent_context)
            };
            GLOBAL_CONTEXT.set(&&cx_fn, functor)
        })
        .or_else(|e| cx.throw_error(e))?)
}

fn run_with_global_context(f: &GlobalContextCallback) {
    GLOBAL_CONTEXT.with(|cx_fn| cx_fn(f))
}

/// Load a .60 files.
///
/// The first argument of this function is a string to the .60 file
///
/// The return value is a SixtyFpsComponentType
fn load(mut cx: FunctionContext) -> JsResult<JsValue> {
    let path = cx.argument::<JsString>(0)?.value();
    let path = std::path::Path::new(path.as_str());
    let include_paths = match std::env::var_os("SIXTYFPS_INCLUDE_PATH") {
        Some(paths) => {
            std::env::split_paths(&paths).filter(|path| !path.as_os_str().is_empty()).collect()
        }
        None => vec![],
    };
    let mut compiler = sixtyfps_interpreter::ComponentCompiler::default();
    compiler.set_include_paths(include_paths);
    let c = spin_on::spin_on(compiler.build_from_path(path));

    sixtyfps_interpreter::print_diagnostics(&compiler.diagnostics());

    let c = if let Some(c) = c { c } else { return cx.throw_error("Compilation error") };

    let mut obj = SixtyFpsComponentType::new::<_, JsValue, _>(&mut cx, std::iter::empty())?;
    cx.borrow_mut(&mut obj, |mut obj| obj.0 = Some(c));
    Ok(obj.as_value(&mut cx))
}

fn make_callback_handler<'cx>(
    cx: &mut impl Context<'cx>,
    persistent_context: &persistent_context::PersistentContext<'cx>,
    fun: Handle<'cx, JsFunction>,
    return_type: Option<Box<Type>>,
) -> Box<dyn Fn(&[sixtyfps_interpreter::Value]) -> sixtyfps_interpreter::Value> {
    let fun_value = fun.as_value(cx);
    let fun_idx = persistent_context.allocate(cx, fun_value);
    Box::new(move |args| {
        let args = args.to_vec();
        let ret = core::cell::Cell::new(sixtyfps_interpreter::Value::Void);
        let borrow_ret = &ret;
        let return_type = &return_type;
        run_with_global_context(&move |cx, persistent_context| {
            let args = args.iter().map(|a| to_js_value(a.clone(), cx).unwrap()).collect::<Vec<_>>();
            let ret = persistent_context
                .get(cx, fun_idx)
                .unwrap()
                .downcast::<JsFunction>()
                .unwrap()
                .call::<_, _, JsValue, _>(cx, JsUndefined::new(), args)
                .unwrap();
            if let Some(return_type) = return_type {
                borrow_ret.set(
                    to_eval_value(ret, (**return_type).clone(), cx, persistent_context).unwrap(),
                );
            }
        });
        ret.into_inner()
    })
}

fn create<'cx>(
    cx: &mut CallContext<'cx, impl neon::object::This>,
    component_type: sixtyfps_interpreter::ComponentDefinition,
) -> JsResult<'cx, JsValue> {
    let component = component_type.clone().create();
    let persistent_context = persistent_context::PersistentContext::new(cx);

    if let Some(args) = cx.argument_opt(0).and_then(|arg| arg.downcast::<JsObject>().ok()) {
        let properties =
            component_type.properties_and_callbacks().collect::<std::collections::HashMap<_, _>>();
        for x in args.get_own_property_names(cx)?.to_vec(cx)? {
            let prop_name = x.to_string(cx)?.value();
            let value = args.get(cx, x)?;
            let ty = properties
                .get(&prop_name)
                .ok_or(())
                .or_else(|()| {
                    cx.throw_error(format!("Property {} not found in the component", prop_name))
                })?
                .clone();
            if let Type::Callback { return_type, .. } = ty {
                let fun = value.downcast_or_throw::<JsFunction, _>(cx)?;
                component
                    .set_callback(
                        prop_name.as_str(),
                        make_callback_handler(cx, &persistent_context, fun, return_type),
                    )
                    .or_else(|_| cx.throw_error(format!("Cannot set callback")))?;
            } else {
                let value = to_eval_value(value, ty, cx, &persistent_context)?;
                component
                    .set_property(prop_name.as_str(), value)
                    .or_else(|_| cx.throw_error(format!("Cannot assign property")))?;
            }
        }
    }

    let mut obj = SixtyFpsComponent::new::<_, JsValue, _>(cx, std::iter::empty())?;
    persistent_context.save_to_object(cx, obj.downcast().unwrap());
    cx.borrow_mut(&mut obj, |mut obj| obj.0 = Some(component));
    Ok(obj.as_value(cx))
}

fn to_eval_value<'cx>(
    val: Handle<'cx, JsValue>,
    ty: sixtyfps_compilerlib::langtype::Type,
    cx: &mut impl Context<'cx>,
    persistent_context: &persistent_context::PersistentContext<'cx>,
) -> NeonResult<sixtyfps_interpreter::Value> {
    use sixtyfps_interpreter::Value;
    match ty {
        Type::Float32
        | Type::Int32
        | Type::Duration
        | Type::Angle
        | Type::PhysicalLength
        | Type::LogicalLength
        | Type::Percent
        | Type::UnitProduct(_) => {
            Ok(Value::Number(val.downcast_or_throw::<JsNumber, _>(cx)?.value()))
        }
        Type::String => Ok(Value::String(val.to_string(cx)?.value().into())),
        Type::Color | Type::Brush => {
            let c = val
                .to_string(cx)?
                .value()
                .parse::<css_color_parser2::Color>()
                .or_else(|e| cx.throw_error(&e.to_string()))?;
            Ok((sixtyfps_corelib::Color::from_argb_u8((c.a * 255.) as u8, c.r, c.g, c.b)).into())
        }
        Type::Array(a) => match val.downcast::<JsArray>() {
            Ok(arr) => {
                let vec = arr.to_vec(cx)?;
                Ok(Value::Array(
                    vec.into_iter()
                        .map(|i| to_eval_value(i, (*a).clone(), cx, persistent_context))
                        .collect::<Result<SharedVector<_>, _>>()?,
                ))
            }
            Err(_) => {
                let obj = val.downcast_or_throw::<JsObject, _>(cx)?;
                obj.get(cx, "rowCount")?.downcast_or_throw::<JsFunction, _>(cx)?;
                obj.get(cx, "rowData")?.downcast_or_throw::<JsFunction, _>(cx)?;
                let m = js_model::JsModel::new(obj, *a, cx, persistent_context)?;
                Ok(Value::Model(sixtyfps_corelib::model::SharedModel(m)))
            }
        },
        Type::Image => {
            let path = val.to_string(cx)?.value();
            Ok(Value::Image(
                sixtyfps_corelib::graphics::Image::load_from_path(std::path::Path::new(&path))
                    .or_else(|_| cx.throw_error(format!("cannot load image {:?}", path)))?,
            ))
        }
        Type::Bool => Ok(Value::Bool(val.downcast_or_throw::<JsBoolean, _>(cx)?.value())),
        Type::Struct { fields, .. } => {
            let obj = val.downcast_or_throw::<JsObject, _>(cx)?;
            Ok(Value::Struct(
                fields
                    .iter()
                    .map(|(pro_name, pro_ty)| {
                        Ok((
                            pro_name.clone(),
                            to_eval_value(
                                obj.get(cx, pro_name.as_str())?,
                                pro_ty.clone(),
                                cx,
                                persistent_context,
                            )?,
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            ))
        }
        Type::Enumeration(_) => todo!(),
        Type::Invalid
        | Type::Void
        | Type::InferredProperty
        | Type::InferredCallback
        | Type::Builtin(_)
        | Type::Native(_)
        | Type::Function { .. }
        | Type::Model
        | Type::Callback { .. }
        | Type::Easing
        | Type::Component(_)
        | Type::PathElements
        | Type::LayoutCache
        | Type::ElementReference => cx.throw_error("Cannot convert to a Sixtyfps property value"),
    }
}

fn to_js_value<'cx>(
    val: sixtyfps_interpreter::Value,
    cx: &mut impl Context<'cx>,
) -> NeonResult<Handle<'cx, JsValue>> {
    use sixtyfps_interpreter::Value;
    Ok(match val {
        Value::Void => JsUndefined::new().as_value(cx),
        Value::Number(n) => JsNumber::new(cx, n).as_value(cx),
        Value::String(s) => JsString::new(cx, s.as_str()).as_value(cx),
        Value::Bool(b) => JsBoolean::new(cx, b).as_value(cx),
        Value::Image(r) => match (&r).into() {
            &ImageInner::None => JsUndefined::new().as_value(cx),
            &ImageInner::AbsoluteFilePath(ref path) => {
                JsString::new(cx, path.as_str()).as_value(cx)
            }
            &ImageInner::EmbeddedData { .. } | &ImageInner::EmbeddedRgbaImage { .. } => {
                JsNull::new().as_value(cx)
            } // TODO: maybe pass around node buffers?
        },
        Value::Array(a) => {
            let js_array = JsArray::new(cx, a.len() as _);
            for (i, e) in a.into_iter().enumerate() {
                let v = to_js_value(e, cx)?;
                js_array.set(cx, i as u32, v)?;
            }
            js_array.as_value(cx)
        }
        Value::Struct(o) => {
            let js_object = JsObject::new(cx);
            for (k, e) in o.iter() {
                let v = to_js_value(e.clone(), cx)?;
                js_object.set(cx, k, v)?;
            }
            js_object.as_value(cx)
        }
        Value::Brush(sixtyfps_corelib::Brush::SolidColor(c)) => JsString::new(
            cx,
            &format!("#{:02x}{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue(), c.alpha()),
        )
        .as_value(cx),
        _ => todo!("converting {:?} to js has not been implemented", val),
    })
}

declare_types! {
    class SixtyFpsComponentType for WrappedComponentType {
        init(_) {
            Ok(WrappedComponentType(None))
        }
        method create(mut cx) {
            let this = cx.this();
            let ct = cx.borrow(&this, |x| x.0.clone());
            let ct = ct.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            create(&mut cx, ct)
        }
        method name(mut cx) {
            let this = cx.this();
            let ct = cx.borrow(&this, |x| x.0.clone());
            let ct = ct.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            Ok(cx.string(ct.name()).as_value(&mut cx))
        }
        method properties(mut cx) {
            let this = cx.this();
            let ct = cx.borrow(&this, |x| x.0.clone());
            let ct = ct.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            let properties = ct.properties_and_callbacks().filter(|(_, prop_type)| prop_type.is_property_type());
            let array = JsArray::new(&mut cx, 0);
            let mut len: u32 = 0;
            for (p, _) in properties {
                let prop_name = JsString::new(&mut cx, p);
                array.set(&mut cx, len, prop_name)?;
                len = len + 1;
            }
            Ok(array.as_value(&mut cx))
        }
        method callbacks(mut cx) {
            let this = cx.this();
            let ct = cx.borrow(&this, |x| x.0.clone());
            let ct = ct.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            let callbacks = ct.properties_and_callbacks().filter(|(_, prop_type)| matches!(prop_type, Type::Callback{..}));
            let array = JsArray::new(&mut cx, 0);
            let mut len: u32 = 0;
            for (p, _) in callbacks {
                let prop_name = JsString::new(&mut cx, p);
                array.set(&mut cx, len, prop_name)?;
                len = len + 1;
            }
            Ok(array.as_value(&mut cx))
        }
    }

    class SixtyFpsComponent for WrappedComponentRc {
        init(_) {
            Ok(WrappedComponentRc(None))
        }
        method run(mut cx) {
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            run_scoped(&mut cx,this.downcast().unwrap(), || {
                component.run();
                Ok(())
            })?;
            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method show(mut cx) {
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            run_scoped(&mut cx,this.downcast().unwrap(), || {
                component.show();
                Ok(())
            })?;
            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method hide(mut cx) {
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            run_scoped(&mut cx,this.downcast().unwrap(), || {
                component.hide();
                Ok(())
            })?;
            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method get_property(mut cx) {
            let prop_name = cx.argument::<JsString>(0)?.value();
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            let value = run_scoped(&mut cx,this.downcast().unwrap(), || {
                component.get_property(prop_name.as_str())
                    .map_err(|_| format!("Cannot read property"))
            })?;
            to_js_value(value, &mut cx)
        }
        method set_property(mut cx) {
            let prop_name = cx.argument::<JsString>(0)?.value();
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            let ty = component.definition().properties_and_callbacks()
                .find_map(|(name, proptype)| if name == prop_name { Some(proptype) } else { None })
                .ok_or(())
                .or_else(|()| {
                    cx.throw_error(format!("Property {} not found in the component", prop_name))
                })?
                .clone();

            let persistent_context =
                persistent_context::PersistentContext::from_object(&mut cx, this.downcast().unwrap())?;

            let value = to_eval_value(cx.argument::<JsValue>(1)?, ty, &mut cx, &persistent_context)?;
            component.set_property(prop_name.as_str(), value)
                .or_else(|_| cx.throw_error(format!("Cannot assign property")))?;

            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method invoke_callback(mut cx) {
            let callback_name = cx.argument::<JsString>(0)?.value();
            let arguments = cx.argument::<JsArray>(1)?.to_vec(&mut cx)?;
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            let ty = component.definition().properties_and_callbacks()
                .find_map(|(name, proptype)| if name == callback_name { Some(proptype) } else { None })
                .ok_or(())
                .or_else(|()| {
                    cx.throw_error(format!("Callback {} not found in the component", callback_name))
                })?
                .clone();
            let persistent_context =
                persistent_context::PersistentContext::from_object(&mut cx, this.downcast().unwrap())?;
            let args = if let Type::Callback {args, ..} = ty {
                let count = args.len();
                let args = arguments.into_iter()
                    .zip(args.into_iter())
                    .map(|(a, ty)| to_eval_value(a, ty, &mut cx, &persistent_context))
                    .collect::<Result<Vec<_>, _>>()?;
                if args.len() != count {
                    cx.throw_error(format!("{} expect {} arguments, but {} where provided", callback_name, count, args.len()))?;
                }
                args

            } else {
                cx.throw_error(format!("{} is not a callback", callback_name))?;
                unreachable!()
            };

            let res = run_scoped(&mut cx,this.downcast().unwrap(), || {
                component.invoke_callback(callback_name.as_str(), args.as_slice())
                    .map_err(|_| "Cannot emit callback".to_string())
            })?;
            to_js_value(res, &mut cx)
        }

        method connect_callback(mut cx) {
            let callback_name = cx.argument::<JsString>(0)?.value();
            let handler = cx.argument::<JsFunction>(1)?;
            let this = cx.this();
            let persistent_context =
                persistent_context::PersistentContext::from_object(&mut cx, this.downcast().unwrap())?;
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;

            let ty = component.definition().properties_and_callbacks()
                .find_map(|(name, proptype)| if name == callback_name { Some(proptype) } else { None })
                .ok_or(())
                .or_else(|()| {
                    cx.throw_error(format!("Callback {} not found in the component", callback_name))
                })?
                .clone();
            if let Type::Callback {return_type, ..} = ty {
                component.set_callback(
                    callback_name.as_str(),
                    make_callback_handler(&mut cx, &persistent_context, handler, return_type)
                ).or_else(|_| cx.throw_error(format!("Cannot set callback")))?;
                Ok(JsUndefined::new().as_value(&mut cx))
            } else {
                cx.throw_error(format!("{} is not a callback", callback_name))?;
                unreachable!()
            }
        }

        method send_mouse_click(mut cx) {
            let x = cx.argument::<JsNumber>(0)?.value() as f32;
            let y = cx.argument::<JsNumber>(1)?.value() as f32;
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            run_scoped(&mut cx,this.downcast().unwrap(), || {
                sixtyfps_interpreter::testing::send_mouse_click(&component, x, y);
                Ok(())
            })?;
            Ok(JsUndefined::new().as_value(&mut cx))
        }

        method send_keyboard_string_sequence(mut cx) {
            let sequence = cx.argument::<JsString>(0)?.value();
            let this = cx.this();
            let component = cx.borrow(&this, |x| x.0.as_ref().map(|c| c.clone_strong()));
            let component = component.ok_or(()).or_else(|()| cx.throw_error("Invalid type"))?;
            run_scoped(&mut cx,this.downcast().unwrap(), || {
                sixtyfps_interpreter::testing::send_keyboard_string_sequence(&component, sequence.into());
                Ok(())
            })?;
            Ok(JsUndefined::new().as_value(&mut cx))
        }
    }
}

fn singleshot_timer_property(id: u32) -> String {
    format!("$__sixtyfps_singleshot_timer_{}", id)
}

fn singleshot_timer(mut cx: FunctionContext) -> JsResult<JsValue> {
    let duration_in_msecs = cx.argument::<JsNumber>(0)?.value() as u64;
    let handler = cx.argument::<JsFunction>(1)?;

    let global_object: Handle<JsObject> = cx.global().downcast().unwrap();
    let unique_timer_property = {
        let mut rng = rand::thread_rng();
        loop {
            let id = rng.next_u32();
            let key = singleshot_timer_property(id);
            if global_object.get(&mut cx, &*key)?.is_a::<JsUndefined>() {
                break key;
            }
        }
    };

    let handler_value = handler.as_value(&mut cx);
    global_object.set(&mut cx, &*unique_timer_property, handler_value).unwrap();
    let callback = move || {
        run_with_global_context(&move |cx, _| {
            let global_object: Handle<JsObject> = cx.global().downcast().unwrap();

            let callback = global_object
                .get(cx, &*unique_timer_property)
                .unwrap()
                .downcast::<JsFunction>()
                .unwrap();

            global_object.set(cx, &*unique_timer_property, JsUndefined::new()).unwrap();

            callback.call::<_, _, JsValue, _>(cx, JsUndefined::new(), vec![]).unwrap();
        });
    };

    sixtyfps_corelib::timers::Timer::single_shot(
        std::time::Duration::from_millis(duration_in_msecs),
        callback,
    );

    Ok(JsUndefined::new().upcast())
}

register_module!(mut m, {
    m.export_function("load", load)?;
    m.export_function("mock_elapsed_time", mock_elapsed_time)?;
    m.export_function("singleshot_timer", singleshot_timer)?;
    #[cfg(feature = "testing")]
    m.export_function("init_testing_backend", init_testing_backend)?;
    Ok(())
});

/// let some time elapse for testing purposes
fn mock_elapsed_time(mut cx: FunctionContext) -> JsResult<JsValue> {
    let ms = cx.argument::<JsNumber>(0)?.value();
    sixtyfps_corelib::tests::sixtyfps_mock_elapsed_time(ms as _);
    Ok(JsUndefined::new().as_value(&mut cx))
}

#[cfg(feature = "testing")]
fn init_testing_backend(mut cx: FunctionContext) -> JsResult<JsValue> {
    sixtyfps_rendering_backend_testing::init();
    Ok(JsUndefined::new().as_value(&mut cx))
}
