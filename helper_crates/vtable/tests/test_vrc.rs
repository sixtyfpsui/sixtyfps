/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#![allow(improper_ctypes_definitions)]

use std::rc::Rc;
use vtable::*;
#[vtable]
struct FooVTable {
    drop_in_place: fn(VRefMut<FooVTable>) -> Layout,
    dealloc: fn(&FooVTable, ptr: *mut u8, layout: Layout),
    rc_string: fn(VRef<FooVTable>) -> Rc<String>,
}

#[derive(Debug, const_field_offset::FieldOffsets)]
#[repr(C)]
#[pin]
struct SomeStruct {
    e: u8,
    x: String,
    foo: Rc<String>,
}
impl Foo for SomeStruct {
    fn rc_string(&self) -> Rc<String> {
        self.foo.clone()
    }
}

FooVTable_static!(static SOME_STRUCT_TYPE for SomeStruct);

#[test]
fn rc_test() {
    let string = Rc::new("hello".to_string());
    let rc = VRc::new(SomeStruct { e: 42, x: "44".into(), foo: string.clone() });
    let string_copy = VRc::borrow(&rc).rc_string();
    assert!(Rc::ptr_eq(&string, &string_copy));
    assert_eq!(VRc::strong_count(&rc), 1);
    assert_eq!(rc.e, 42);
    drop(string_copy);
    let w = VRc::downgrade(&rc);
    assert_eq!(VRc::strong_count(&rc), 1);
    {
        let rc2 = w.upgrade().unwrap();
        let string_copy = VRc::borrow(&rc2).rc_string();
        assert!(Rc::ptr_eq(&string, &string_copy));
        assert_eq!(VRc::strong_count(&rc), 2);
        assert!(VRc::ptr_eq(&rc, &rc2));
        // one in `string`, one in `string_copy`, one in the shared region.
        assert_eq!(Rc::strong_count(&string), 3);
        assert_eq!(rc2.e, 42);
    }
    assert_eq!(VRc::strong_count(&rc), 1);
    drop(rc);
    assert_eq!(Rc::strong_count(&string), 1);
    assert!(w.upgrade().is_none());

    let rc = VRc::new(SomeStruct { e: 55, x: "_".into(), foo: string.clone() });
    assert!(VRc::ptr_eq(&rc, &rc.clone()));
    assert_eq!(Rc::strong_count(&string), 2);
    assert_eq!(VRc::strong_count(&rc), 1);
    assert_eq!(rc.e, 55);
    drop(rc);
    assert_eq!(Rc::strong_count(&string), 1);
}

#[test]
fn rc_dyn_test() {
    let string = Rc::new("hello".to_string());
    let origin = VRc::new(SomeStruct { e: 42, x: "44".into(), foo: string.clone() });
    let origin_weak = VRc::downgrade(&origin);
    let rc: VRc<FooVTable> = VRc::into_dyn(origin);
    let string_copy = VRc::borrow(&rc).rc_string();
    assert!(Rc::ptr_eq(&string, &string_copy));
    assert_eq!(VRc::strong_count(&rc), 1);
    drop(string_copy);
    let w = VRc::downgrade(&rc);
    assert_eq!(VRc::strong_count(&rc), 1);
    {
        let rc2 = w.upgrade().unwrap();
        let string_copy = VRc::borrow(&rc2).rc_string();
        assert!(Rc::ptr_eq(&string, &string_copy));
        assert_eq!(VRc::strong_count(&rc), 2);
        assert!(VRc::ptr_eq(&rc, &rc2));
        // one in `string`, one in `string_copy`, one in the shared region.
        assert_eq!(Rc::strong_count(&string), 3);
    }
    assert_eq!(VRc::strong_count(&rc), 1);
    {
        let rc_origin = origin_weak.upgrade().unwrap();
        assert_eq!(rc_origin.e, 42);
        assert!(VRc::ptr_eq(&VRc::into_dyn(rc_origin.clone()), &rc));
    }
    drop(rc);
    assert_eq!(Rc::strong_count(&string), 1);
    assert!(w.upgrade().is_none());
    assert!(origin_weak.upgrade().is_none());
    assert!(origin_weak.into_dyn().upgrade().is_none());

    let rc: VRc<FooVTable> =
        VRc::into_dyn(VRc::new(SomeStruct { e: 55, x: "_".into(), foo: string.clone() }));
    assert!(VRc::ptr_eq(&rc, &rc.clone()));
    assert_eq!(Rc::strong_count(&string), 2);
    assert_eq!(VRc::strong_count(&rc), 1);
    drop(rc);
    assert_eq!(Rc::strong_count(&string), 1);
}

#[derive(Debug, const_field_offset::FieldOffsets)]
#[repr(C)]
struct SyncStruct {
    e: std::sync::Mutex<String>,
}
impl Foo for SyncStruct {
    fn rc_string(&self) -> Rc<String> {
        Rc::new(self.e.lock().unwrap().clone())
    }
}

FooVTable_static!(static SYNC_STRUCT_TYPE for SyncStruct);
#[test]
fn rc_test_threading() {
    let rc = VRc::new(SyncStruct { e: std::sync::Mutex::new("44".into()) });
    let weak = VRc::downgrade(&rc);
    assert_eq!(*rc.rc_string(), "44");
    let mut handles = Vec::new();
    for _ in 0..10 {
        let rc = rc.clone();
        handles.push(std::thread::spawn(move || {
            let _w = VRc::downgrade(&rc.clone());
            for _ in 0..10 {
                let weak = VRc::downgrade(&rc);
                let rc2 = weak.upgrade().unwrap();
                let mut lock = rc2.e.lock().unwrap();
                let v: u32 = lock.parse().unwrap();
                *lock = (v + 1).to_string();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(*rc.rc_string(), "144");
    let h = std::thread::spawn(move || drop(rc));
    drop(weak);
    h.join().unwrap();
}

#[vtable]
struct AppVTable {
    drop_in_place: fn(VRefMut<AppVTable>) -> Layout,
    dealloc: fn(&AppVTable, ptr: *mut u8, layout: Layout),
}

#[derive(Debug, const_field_offset::FieldOffsets)]
#[repr(C)]
#[pin]
struct AppStruct {
    some: SomeStruct,
    another_struct: SomeStruct,
}

impl AppStruct {
    fn new() -> VRc<AppVTable, Self> {
        let self_ = Self {
            some: SomeStruct { e: 55, x: "_".into(), foo: Rc::new("some".into()) },
            another_struct: SomeStruct { e: 100, x: "_".into(), foo: Rc::new("another".into()) },
        };
        VRc::new(self_)
    }
}

impl App for AppStruct {}

AppVTable_static!(static APP_STRUCT_TYPE for AppStruct);

#[test]
fn rc_map_test() {
    fn get_struct_value(instance: &VRcMapped<AppVTable, SomeStruct>) -> u8 {
        let field_ref = SomeStruct::FIELD_OFFSETS.e.apply_pin(instance.as_pin_ref());
        *field_ref
    }

    let app_rc = AppStruct::new();

    let some_struct_ref =
        VRc::map(app_rc.clone(), |app| AppStruct::FIELD_OFFSETS.some.apply_pin(app));
    let other_struct_ref =
        VRc::map(app_rc.clone(), |app| AppStruct::FIELD_OFFSETS.another_struct.apply_pin(app));

    let weak_struct_ref = VRcMapped::downgrade(&some_struct_ref);

    {
        let strong_struct_ref = weak_struct_ref.upgrade().unwrap();
        assert_eq!(get_struct_value(&strong_struct_ref), 55);
    }

    {
        assert_eq!(get_struct_value(&other_struct_ref), 100);
    }

    drop(app_rc);

    {
        let strong_struct_ref = weak_struct_ref.upgrade().unwrap();
        let e_field = SomeStruct::FIELD_OFFSETS.e.apply_pin(strong_struct_ref.as_pin_ref());
        assert_eq!(*e_field, 55);
    }

    drop(some_struct_ref);
    drop(other_struct_ref);

    assert!(weak_struct_ref.upgrade().is_none());
}

#[test]
fn rc_map_origin() {
    let app_rc = AppStruct::new();

    let some_struct_ref =
        VRc::map(app_rc.clone(), |app| AppStruct::FIELD_OFFSETS.some.apply_pin(app));

    drop(app_rc);

    let strong_origin = VRcMapped::origin(&some_struct_ref);

    drop(some_struct_ref);

    let weak_origin = VRc::downgrade(&strong_origin);

    assert_eq!(VRc::strong_count(&strong_origin), 1);

    drop(strong_origin);

    assert!(weak_origin.upgrade().is_none());
}

#[test]
fn rc_map_dyn_test() {
    let app_rc = AppStruct::new();

    let some_struct_ref = VRcMapped::into_dyn::<FooVTable>(&VRc::map(app_rc.clone(), |app| {
        AppStruct::FIELD_OFFSETS.some.apply_pin(app)
    }));
    assert_eq!(VRcMappedDyn::borrow(&some_struct_ref).rc_string().as_str(), "some");

    let weak_struct_ref = VRcMappedDyn::downgrade(&some_struct_ref);

    {
        let strong_struct_ref = weak_struct_ref.upgrade().unwrap();
        assert_eq!(VRcMappedDyn::borrow(&strong_struct_ref).rc_string().as_str(), "some");
    }

    drop(app_rc);

    {
        let strong_struct_ref = weak_struct_ref.upgrade().unwrap();
        assert_eq!(VRcMappedDyn::borrow(&strong_struct_ref).rc_string().as_str(), "some");
    }

    drop(some_struct_ref);
    assert!(weak_struct_ref.upgrade().is_none());
}
