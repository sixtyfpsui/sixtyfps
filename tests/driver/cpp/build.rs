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

#[path = "../../../xtask/src/cbindgen.rs"]
mod cbindgen;

/// The root dir of the git repository
fn root_dir() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // $root/tests/driver/driver/ -> $root
    root.pop();
    root.pop();
    root.pop();
    root
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Variables that cc.rs needs.
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET").unwrap());
    println!("cargo:rustc-env=HOST={}", std::env::var("HOST").unwrap());
    println!("cargo:rustc-env=OPT_LEVEL={}", std::env::var("OPT_LEVEL").unwrap());

    // target/{debug|release}/build/package/out/ -> target/{debug|release}
    let mut target_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    target_dir.pop();
    target_dir.pop();
    target_dir.pop();

    println!("cargo:rustc-env=CPP_LIB_PATH={}", target_dir.display());

    let mut include_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    include_dir.push("include");
    println!("cargo:rustc-env=GENERATED_CPP_HEADERS_PATH={}", include_dir.display());
    cbindgen::gen_all(&root_dir(), &include_dir)?;
    // re-run cbindgen if files changes
    let root_dir = root_dir();
    println!("cargo:rerun-if-changed={}/sixtyfps_runtime/corelib/", root_dir.display());
    for entry in std::fs::read_dir(root_dir.join("sixtyfps_runtime/corelib/"))? {
        let entry = entry?;
        if entry.path().extension().map_or(false, |e| e == "rs") {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    println!(
        "cargo:rustc-env=CPP_API_HEADERS_PATH={}/api/sixtyfps-cpp/include",
        root_dir.display()
    );

    let tests_file_path =
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("test_functions.rs");

    let mut tests_file = std::fs::File::create(&tests_file_path)?;

    for testcase in test_driver_lib::collect_test_cases()? {
        let test_function_name = testcase.identifier();

        write!(
            tests_file,
            r##"
            #[test]
            fn test_cpp_{function_name}() {{
                cppdriver::test(&test_driver_lib::TestCase{{
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
