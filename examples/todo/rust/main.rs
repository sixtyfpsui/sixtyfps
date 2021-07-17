/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use sixtyfps::Model;
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

sixtyfps::include_modules!();

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    console_error_panic_hook::set_once();

    let todo_model = Rc::new(sixtyfps::VecModel::<TodoItem>::from(vec![
        TodoItem { checked: true, title: "Implement the .60 file".into() },
        TodoItem { checked: true, title: "Do the Rust part".into() },
        TodoItem { checked: false, title: "Make the C++ code".into() },
        TodoItem { checked: false, title: "Write some JavaScript code".into() },
        TodoItem { checked: false, title: "Test the application".into() },
        TodoItem { checked: false, title: "Ship to customer".into() },
        TodoItem { checked: false, title: "???".into() },
        TodoItem { checked: false, title: "Profit".into() },
    ]));

    let main_window = MainWindow::new();
    main_window.on_todo_added({
        let todo_model = todo_model.clone();
        move |text| todo_model.push(TodoItem { checked: false, title: text })
    });
    main_window.on_remove_done({
        let todo_model = todo_model.clone();
        move || {
            let mut offset = 0;
            for i in 0..todo_model.row_count() {
                if todo_model.row_data(i - offset).checked {
                    todo_model.remove(i - offset);
                    offset += 1;
                }
            }
        }
    });

    main_window.set_todo_model(sixtyfps::ModelHandle::new(todo_model));

    main_window.run();
}
