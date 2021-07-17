/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
Since neon does not allow to have a persistent handle, use this hack.
*/

use neon::prelude::*;
pub struct PersistentContext<'a>(Handle<'a, JsArray>);

const KEY: &'static str = "$__persistent_context";

/// Since neon does not allow to have a persistent handle, this allocates property in an array.
/// This array is gonna be kept as a property somewhere.
impl<'a> PersistentContext<'a> {
    pub fn new(cx: &mut impl Context<'a>) -> Self {
        PersistentContext(JsArray::new(cx, 0))
    }

    pub fn allocate(&self, cx: &mut impl Context<'a>, value: Handle<'a, JsValue>) -> u32 {
        let idx = self.0.len();
        self.0.set(cx, idx, value).unwrap();
        idx
    }

    pub fn get(&self, cx: &mut impl Context<'a>, idx: u32) -> JsResult<'a, JsValue> {
        self.0.get(cx, idx)
    }

    pub fn save_to_object(&self, cx: &mut impl Context<'a>, o: Handle<'a, JsObject>) {
        o.set(cx, KEY, self.0).unwrap();
    }

    pub fn from_object(cx: &mut impl Context<'a>, o: Handle<'a, JsObject>) -> NeonResult<Self> {
        Ok(PersistentContext(o.get(cx, KEY)?.downcast_or_throw(cx)?))
    }
}
