/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#[allow(dead_code)]
// ANCHOR: main
fn main() {
    MainWindow::new().run();
}
sixtyfps::sixtyfps! {
    MainWindow := Window {
        Text {
            text: "hello world";
            color: green;
        }
    }
}
// ANCHOR_END: main
