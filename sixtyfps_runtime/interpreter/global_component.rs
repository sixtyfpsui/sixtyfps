/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use core::pin::Pin;
use std::rc::Rc;

use crate::api::Value;
use crate::dynamic_component::{ErasedComponentBox, ErasedComponentDescription};
use sixtyfps_compilerlib::namedreference::NamedReference;
use sixtyfps_compilerlib::{langtype::Type, object_tree::Component};
use sixtyfps_corelib::component::ComponentVTable;
use sixtyfps_corelib::rtti;

pub enum CompiledGlobal {
    Builtin(String, Rc<sixtyfps_compilerlib::langtype::BuiltinElement>),
    Component(ErasedComponentDescription),
}

pub trait GlobalComponent {
    fn invoke_callback(self: Pin<&Self>, _callback_name: &str, _args: &[Value]) -> Value {
        todo!("call callback")
    }

    fn set_property(self: Pin<&Self>, prop_name: &str, value: Value);
    fn get_property(self: Pin<&Self>, prop_name: &str) -> Value;

    fn get_property_ptr(self: Pin<&Self>, prop_name: &str) -> *const ();
}

pub fn instantiate(description: &CompiledGlobal) -> (String, Pin<Rc<dyn GlobalComponent>>) {
    match description {
        CompiledGlobal::Builtin(name, b) => {
            trait Helper {
                fn instantiate(name: &str) -> Pin<Rc<dyn GlobalComponent>> {
                    panic!("Cannot find native global {}", name)
                }
            }
            impl Helper for () {}
            impl<T: rtti::BuiltinItem + Default + 'static, Next: Helper> Helper for (T, Next) {
                fn instantiate(name: &str) -> Pin<Rc<dyn GlobalComponent>> {
                    if name == T::name() {
                        Rc::pin(T::default())
                    } else {
                        Next::instantiate(name)
                    }
                }
            }
            let g = sixtyfps_rendering_backend_default::NativeGlobals::instantiate(
                b.native_class.class_name.as_ref(),
            );
            (name.clone(), g)
        }
        CompiledGlobal::Component(description) => {
            generativity::make_guard!(guard);
            let description = description.unerase(guard);
            let component = &description.original;
            let g = Rc::pin(GlobalComponentInstance(crate::dynamic_component::instantiate(
                description.clone(),
                None,
                None,
            )));
            let id = if component.is_global() {
                component.root_element.borrow().id.clone()
            } else {
                component.id.clone()
            };
            (id, g)
        }
    }
}

/// For the global components, we don't use the dynamic_type optimization,
/// and we don't try to to optimize the property to their real type
pub struct GlobalComponentInstance(vtable::VRc<ComponentVTable, ErasedComponentBox>);

impl GlobalComponent for GlobalComponentInstance {
    fn set_property(self: Pin<&Self>, prop_name: &str, value: Value) {
        generativity::make_guard!(guard);
        let comp = self.0.unerase(guard);
        comp.description().set_property(comp.borrow(), prop_name, value).unwrap()
    }

    fn get_property(self: Pin<&Self>, prop_name: &str) -> Value {
        generativity::make_guard!(guard);
        let comp = self.0.unerase(guard);
        comp.description().get_property(comp.borrow(), prop_name).unwrap()
    }

    fn get_property_ptr(self: Pin<&Self>, prop_name: &str) -> *const () {
        generativity::make_guard!(guard);
        let comp = self.0.unerase(guard);
        crate::dynamic_component::get_property_ptr(
            &NamedReference::new(&comp.description().original.root_element, prop_name),
            comp.borrow_instance(),
        )
    }
}

impl<T: rtti::BuiltinItem + 'static> GlobalComponent for T {
    fn set_property(self: Pin<&Self>, prop_name: &str, value: Value) {
        let prop = Self::properties().into_iter().find(|(k, _)| *k == prop_name).unwrap().1;
        prop.set(self, value, None).unwrap()
    }

    fn get_property(self: Pin<&Self>, prop_name: &str) -> Value {
        let prop = Self::properties().into_iter().find(|(k, _)| *k == prop_name).unwrap().1;
        prop.get(self).unwrap()
    }

    fn get_property_ptr(self: Pin<&Self>, prop_name: &str) -> *const () {
        let prop: &dyn rtti::PropertyInfo<Self, Value> =
            Self::properties().into_iter().find(|(k, _)| *k == prop_name).unwrap().1;
        unsafe { (self.get_ref() as *const Self as *const u8).add(prop.offset()) as *const () }
    }
}

pub(crate) fn generate(component: &Rc<Component>) -> CompiledGlobal {
    debug_assert!(component.is_global());
    match &component.root_element.borrow().base_type {
        Type::Void => {
            generativity::make_guard!(guard);
            CompiledGlobal::Component(
                crate::dynamic_component::generate_component(component, guard).into(),
            )
        }
        Type::Builtin(b) => CompiledGlobal::Builtin(component.id.clone(), b.clone()),
        _ => unreachable!(),
    }
}
