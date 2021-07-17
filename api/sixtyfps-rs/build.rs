/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use rustc_version::{version_meta, Channel};

fn main() {
    // Set cfg flags depending on release channel
    //
    match version_meta().unwrap().channel {
        Channel::Stable => println!("cargo:rustc-cfg=stable"),
        Channel::Beta => println!("cargo:rustc-cfg=beta"),
        Channel::Nightly => println!("cargo:rustc-cfg=nightly"),
        Channel::Dev => println!("cargo:rustc-cfg=rustc_dev"),
    }
}
