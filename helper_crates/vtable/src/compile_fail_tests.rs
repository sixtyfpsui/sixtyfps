/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
mod m1 {

    /**
    ```
    use vtable::*;
    #[vtable]
    struct MyVTable {
        foo: fn(VRef<'_, MyVTable>) -> u32,
        create: fn(&MyVTable)->VBox<MyVTable>,
        drop: fn(VRefMut<'_, MyVTable>),
    }
    struct S(u32);
    impl My for S {
        fn foo(&self) -> u32 { self.0 }
        fn create() -> Self { S(55) }
    }
    struct R(u8);
    impl My for R {
        fn foo(&self) -> u32 { (self.0 + 3) as _ }
        fn create() -> Self { R(8) }
    }
    MyVTable_static!(static S_VT for S);
    MyVTable_static!(static R_VT for R);
    let x = S_VT.create();
    ```
    */
    #[cfg(doctest)]
    const _: u32 = 0;

    /**
    Test that one cannot call a function of the vtable with the wrong type
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable {
        foo: fn(VRef<'_, MyVTable>) -> u32,
        create: fn(&MyVTable)->VBox<MyVTable>,
        drop: fn(VRefMut<'_, MyVTable>),
    }
    struct S(u32);
    impl My for S {
        fn foo(&self) -> u32 { self.0 }
        fn create() -> Self { S(55) }
    }
    struct R(u8);
    impl My for R {
        fn foo(&self) -> u32 { (self.0 + 3) as _ }
        fn create() -> Self { R(8) }
    }
    MyVTable_static!(static S_VT for S);
    MyVTable_static!(static R_VT for R);
    let x = S_VT.create();
    //unsafe     // must compile when unsafe
    { (R_VT.foo)(x.borrow()); }
    ```
    */
    #[cfg(doctest)]
    const _: u32 = 0;
}

mod test_vrefmut {
    /**
    VRefMut cannot be cloned
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    fn xx(x : VRefMut<'a, MyVTable>) {
        let x2 = x.clone()
    }
    ```
    */
    #[cfg(doctest)]
    const _1: u32 = 0;

    /**
    VRefMut's dereference cannot be copied
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    fn xx(x : VRefMut<'a, MyVTable>) {
        let x2 = *x;
    }
    ```
    */
    #[cfg(doctest)]
    const _2: u32 = 0;
}

mod test_new_vref {
    /** can't return something local
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    struct X;
    impl My for X {}
    fn xx<'a>(_: &'a u32) -> VRef<'a, MyVTable> {
        let x = X;
        new_vref!(let q : VRef<MyVTable> for My = &x);
        q
    }
    ```
    */
    #[cfg(doctest)]
    const _1: u32 = 0;

    /** Can't outlive the vtable
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    struct X;
    impl My for X {}
    fn xx<'a>(x: &'a X) -> VRef<'a, MyVTable> {
        new_vref!(let q : VRef<MyVTable> for My = x);
        q
    }
    ```
    */
    #[cfg(doctest)]
    const _2: u32 = 0;

    /** Same for the mut version
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    struct X;
    impl My for X {}
    fn xx<'a>(x: &'a mut X) -> VRefMut<'a, MyVTable> {
        new_vref!(let mut q : VRefMut<MyVTable> for My = x);
        q
    }
    ```
    */
    #[cfg(doctest)]
    const _3: u32 = 0;

    /** Try to use mut while not mut
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct MyVTable { }
    struct X;
    impl My for X {}
    fn xx<'a>(x: &'a X)  {
        new_vref!(let mut q : VRefMut<MyVTable> for My = x);
    }
    ```
    */
    #[cfg(doctest)]
    const _4: u32 = 0;

    /** Mixed types
    ```compile_fail
    use vtable::*;
    #[vtable]
    struct My1VTable { }
    #[vtable]
    struct My2VTable { }
    struct X;
    impl My1 for X {}
    impl My2 for X {}
    fn xx<'a>(x: &'a X)  {
        new_vref!(let q : VRef<My1VTable> for My2 = x);
    }
    ```
    */
    #[cfg(doctest)]
    const _5: u32 = 0;
}
