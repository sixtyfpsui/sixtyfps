/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::io::Write;
use std::path::PathBuf;

fn os_dylib_prefix_and_suffix() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        ("", "dll")
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // target/{debug|release}/build/package/out/ -> target/{debug|release}
    let mut target_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    target_dir.pop();
    target_dir.pop();
    target_dir.pop();

    let nodejs_native_lib_name = {
        let (prefix, suffix) = os_dylib_prefix_and_suffix();
        format!("{}sixtyfps_node_native.{}", prefix, suffix)
    };
    println!(
        "cargo:rustc-env=SIXTYFPS_NODE_NATIVE_LIB={}",
        target_dir.join(nodejs_native_lib_name).display()
    );

    let tests_file_path =
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("test_functions.rs");

    let mut tests_file = std::fs::File::create(&tests_file_path)?;

    for testcase in test_driver_lib::collect_test_cases()? {
        println!("cargo:rerun-if-changed={}", testcase.absolute_path.display());
        let test_function_name = testcase.identifier();

        write!(
            tests_file,
            r##"
            #[test]
            fn test_nodejs_{function_name}() {{
                nodejs::test(&test_driver_lib::TestCase{{
                    absolute_path: std::path::PathBuf::from(r#"{absolute_path}"#),
                    relative_path: std::path::PathBuf::from(r#"{relative_path}"#),
                }}).unwrap();
            }}
        "##,
            function_name = test_function_name,
            absolute_path = testcase.absolute_path.to_string_lossy(),
            relative_path = testcase.relative_path.to_string_lossy(),
        )?;
    }

    println!("cargo:rustc-env=TEST_FUNCTIONS={}", tests_file_path.to_string_lossy());

    Ok(())
}
