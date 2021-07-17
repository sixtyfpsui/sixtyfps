/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::api::Value;
use sixtyfps_corelib::model::Model;
use std::cell::RefCell;

pub struct ValueModel {
    value: RefCell<Value>,
    notify: sixtyfps_corelib::model::ModelNotify,
}

impl ValueModel {
    pub fn new(value: Value) -> Self {
        Self { value: RefCell::new(value), notify: Default::default() }
    }
}

impl Model for ValueModel {
    type Data = Value;

    fn row_count(&self) -> usize {
        match &*self.value.borrow() {
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            Value::Number(x) => *x as usize,
            Value::Array(a) => a.len(),
            Value::Void => 0,
            Value::Model(model_ptr) => model_ptr.row_count(),
            x => panic!("Invalid model {:?}", x),
        }
    }

    fn row_data(&self, row: usize) -> Self::Data {
        match &*self.value.borrow() {
            Value::Bool(_) => Value::Void,
            Value::Number(_) => Value::Number(row as _),
            Value::Array(a) => a[row].clone(),
            Value::Model(model_ptr) => model_ptr.row_data(row),
            x => panic!("Invalid model {:?}", x),
        }
    }

    fn attach_peer(&self, peer: sixtyfps_corelib::model::ModelPeer) {
        if let Value::Model(ref model_ptr) = *self.value.borrow() {
            model_ptr.attach_peer(peer.clone())
        }
        self.notify.attach(peer)
    }

    fn set_row_data(&self, row: usize, data: Self::Data) {
        match &mut *self.value.borrow_mut() {
            Value::Array(a) => {
                a.as_slice_mut()[row] = data;
                self.notify.row_changed(row)
            }
            Value::Model(model_ptr) => model_ptr.set_row_data(row, data),
            _ => println!("Value of model cannot be change"),
        }
    }
}
