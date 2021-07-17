/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

fn main() {
    println!("cargo:rerun-if-env-changed=SIXTYFPS_NO_QT");
    if std::env::var("TARGET").map_or(false, |t| t.starts_with("wasm"))
        || std::env::var("SIXTYFPS_NO_QT").is_ok()
    {
        println!("cargo:rustc-cfg=no_qt");
        return;
    }
    if std::env::var("DEP_QT_FOUND").unwrap() != "1" {
        println!("cargo:rustc-cfg=no_qt");
        println!(
            "cargo:warning=Could not find a Qt installation. The Qt backend will not be functional. \
            See https://github.com/sixtyfpsui/sixtyfps/blob/master/docs/install_qt.md for more info"
        );
        return;
    }
    let qt_version = std::env::var("DEP_QT_VERSION").unwrap();
    if !qt_version.starts_with("5.15") && !qt_version.starts_with("6.") {
        println!("cargo:rustc-cfg=no_qt");
        println!(
            "cargo:warning=Qt {} is not supported, you need at least Qt 5.15. The Qt backend will not be functional. \
             See https://github.com/sixtyfpsui/sixtyfps/blob/master/docs/install_qt.md for more info",
            qt_version
        );
        return;
    }

    let mut config = cpp_build::Config::new();

    config.flag_if_supported("-std=c++17");
    config.flag_if_supported("/std:c++17");

    if cfg!(target_os = "macos") {
        config.flag("-F");
        config.flag(&std::env::var("DEP_QT_LIBRARY_PATH").unwrap());
    }
    config.include(std::env::var("DEP_QT_INCLUDE_PATH").unwrap()).build("lib.rs");

    println!("cargo:rerun-if-changed=qt_window.rs");
    println!("cargo:rerun-if-changed=widgets.rs");
    println!("cargo:rerun-if-changed=lib.rs");
}
