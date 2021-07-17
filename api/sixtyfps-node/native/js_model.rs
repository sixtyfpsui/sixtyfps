/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use neon::prelude::*;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_corelib::model::Model;
use std::cell::Cell;
use std::rc::{Rc, Weak};

/// Model coming from JS
pub struct JsModel {
    notify: sixtyfps_corelib::model::ModelNotify,
    /// The index of the value in the PersistentContext
    value_index: u32,
    data_type: Type,
}

impl JsModel {
    pub fn new<'cx>(
        obj: Handle<'cx, JsObject>,
        data_type: Type,
        cx: &mut impl Context<'cx>,
        persistent_context: &crate::persistent_context::PersistentContext<'cx>,
    ) -> NeonResult<Rc<Self>> {
        let val = obj.as_value(cx);
        let model = Rc::new(JsModel {
            notify: Default::default(),
            value_index: persistent_context.allocate(cx, val),
            data_type,
        });

        let mut notify = SixtyFpsModelNotify::new::<_, JsValue, _>(cx, std::iter::empty())?;
        cx.borrow_mut(&mut notify, |mut notify| notify.0 = Rc::downgrade(&model));
        let notify = notify.as_value(cx);
        obj.set(cx, "notify", notify)?;

        Ok(model)
    }

    fn get_object<'cx>(
        &self,
        cx: &mut impl Context<'cx>,
        persistent_context: &crate::persistent_context::PersistentContext<'cx>,
    ) -> JsResult<'cx, JsObject> {
        persistent_context.get(cx, self.value_index)?.downcast_or_throw(cx)
    }
}

impl Model for JsModel {
    type Data = sixtyfps_interpreter::Value;

    fn row_count(&self) -> usize {
        let r = Cell::new(0usize);
        crate::run_with_global_context(&|cx, persistent_context| {
            let obj = self.get_object(cx, persistent_context).unwrap();
            let _ = obj
                .get(cx, "rowCount")
                .ok()
                .and_then(|func| func.downcast::<JsFunction>().ok())
                .and_then(|func| func.call(cx, obj, std::iter::empty::<Handle<JsValue>>()).ok())
                .and_then(|res| res.downcast::<JsNumber>().ok())
                .map(|num| r.set(num.value() as _));
        });
        r.get()
    }

    fn row_data(&self, row: usize) -> Self::Data {
        let r = Cell::new(sixtyfps_interpreter::Value::default());
        crate::run_with_global_context(&|cx, persistent_context| {
            let row = JsNumber::new(cx, row as f64);
            let obj = self.get_object(cx, persistent_context).unwrap();
            let _ = obj
                .get(cx, "rowData")
                .ok()
                .and_then(|func| func.downcast::<JsFunction>().ok())
                .and_then(|func| func.call(cx, obj, std::iter::once(row)).ok())
                .and_then(|res| {
                    crate::to_eval_value(res, self.data_type.clone(), cx, persistent_context).ok()
                })
                .map(|res| r.set(res));
        });
        r.into_inner()
    }

    fn attach_peer(&self, peer: sixtyfps_corelib::model::ModelPeer) {
        self.notify.attach(peer)
    }

    fn set_row_data(&self, row: usize, data: Self::Data) {
        crate::run_with_global_context(&|cx, persistent_context| {
            let row = JsNumber::new(cx, row as f64).as_value(cx);
            let data = crate::to_js_value(data.clone(), cx).unwrap();
            let obj = self.get_object(cx, persistent_context).unwrap();
            let _ = obj
                .get(cx, "setRowData")
                .ok()
                .and_then(|func| func.downcast::<JsFunction>().ok())
                .and_then(|func| func.call(cx, obj, [row, data].iter().cloned()).ok());
        });
    }
}

struct WrappedJsModel(Weak<JsModel>);

declare_types! {
    class SixtyFpsModelNotify for WrappedJsModel {
        init(_) {
            Ok(WrappedJsModel(Weak::default()))
        }
        method rowDataChanged(mut cx) {
            let this = cx.this();
            let row = cx.argument::<JsNumber>(0)?.value() as usize;
            if let Some(model) = cx.borrow(&this, |x| x.0.upgrade()) {
                model.notify.row_changed(row)
            }
            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method rowAdded(mut cx) {
            let this = cx.this();
            let row = cx.argument::<JsNumber>(0)?.value() as usize;
            let count = cx.argument::<JsNumber>(1)?.value() as usize;
            if let Some(model) = cx.borrow(&this, |x| x.0.upgrade()) {
                model.notify.row_added(row, count)
            }
            Ok(JsUndefined::new().as_value(&mut cx))
        }
        method rowRemoved(mut cx) {
            let this = cx.this();
            let row = cx.argument::<JsNumber>(0)?.value() as usize;
            let count = cx.argument::<JsNumber>(1)?.value() as usize;
            if let Some(model) = cx.borrow(&this, |x| x.0.upgrade()) {
                model.notify.row_removed(row, count)
            }
            Ok(JsUndefined::new().as_value(&mut cx))
        }

    }

}
