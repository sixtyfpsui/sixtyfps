/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
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
