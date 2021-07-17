/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

sixtyfps::include_modules!();

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    console_error_panic_hook::set_once();

    let main_window = MainWindow::new();
    main_window.set_ink_levels(sixtyfps::VecModel::from_slice(&[
        InkLevel { color: sixtyfps::Color::from_rgb_u8(0, 255, 255), level: 0.40 },
        InkLevel { color: sixtyfps::Color::from_rgb_u8(255, 0, 255), level: 0.20 },
        InkLevel { color: sixtyfps::Color::from_rgb_u8(255, 255, 0), level: 0.50 },
        InkLevel { color: sixtyfps::Color::from_rgb_u8(0, 0, 0), level: 0.80 },
    ]));

    let main_weak = main_window.as_weak();
    main_window.on_fax_number_erase(move || {
        let main_window = main_weak.unwrap();
        let mut fax_number = main_window.get_fax_number().to_string();
        fax_number.pop();
        main_window.set_fax_number(fax_number.into());
    });

    let main_weak = main_window.as_weak();
    main_window.on_fax_send(move || {
        let main_window = main_weak.upgrade().unwrap();
        let fax_number = main_window.get_fax_number().to_string();
        println!("Sending a fax to {}", fax_number);
        main_window.set_fax_number(sixtyfps::SharedString::default());
    });

    main_window.on_quit(move || {
        #[cfg(not(target_arch = "wasm32"))]
        std::process::exit(0);
    });

    main_window.run();
}
