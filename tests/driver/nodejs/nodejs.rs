/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::error::Error;
use std::{fs::File, io::Write, path::PathBuf};

lazy_static::lazy_static! {
    static ref NODE_API_JS_PATH: PathBuf = {
        let  node_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../api/sixtyfps-node");

        // On Windows npm is 'npm.cmd', which Rust's process::Command doesn't look for as extension, because
         // it tries to emulate CreateProcess.
         let npm = which::which("npm").unwrap();

        // Ensure TypeScript is installed
       std::process::Command::new(npm.clone())
            .arg("install")
            .arg("--ignore-scripts")
            .arg("--no-audit")
            .current_dir(node_dir.clone())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|err| format!("Could not launch npm install: {}", err)).unwrap();

        // Build the .js file of the NodeJS API from the .ts file
        std::process::Command::new(npm)
            .arg("run")
            .arg("build")
            .current_dir(node_dir.clone())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|err| format!("Could not launch npm run build: {}", err)).unwrap();

        node_dir.join("dist").join("index.js")
    };
}

pub fn test(testcase: &test_driver_lib::TestCase) -> Result<(), Box<dyn Error>> {
    let sixtyfpspath = NODE_API_JS_PATH.clone();

    let dir = tempfile::tempdir()?;

    let mut main_js = File::create(dir.path().join("main.js"))?;
    write!(
        main_js,
        r#"
                const assert = require('assert').strict;
                let sixtyfpslib = require(String.raw`{sixtyfpspath}`);
                sixtyfpslib.private_api.init_testing_backend();
                let sixtyfps = require(String.raw`{path}`);
        "#,
        sixtyfpspath = sixtyfpspath.to_string_lossy(),
        path = testcase.absolute_path.to_string_lossy()
    )?;
    let source = std::fs::read_to_string(&testcase.absolute_path)?;
    let include_paths = test_driver_lib::extract_include_paths(&source);
    for x in test_driver_lib::extract_test_functions(&source).filter(|x| x.language_id == "js") {
        write!(main_js, "{{\n    {}\n}}\n", x.source.replace("\n", "\n    "))?;
    }

    let output = std::process::Command::new("node")
        .arg(dir.path().join("main.js"))
        .current_dir(dir.path())
        .env("SIXTYFPS_NODE_NATIVE_LIB", std::env::var_os("SIXTYFPS_NODE_NATIVE_LIB").unwrap())
        .env("SIXTYFPS_INCLUDE_PATH", std::env::join_paths(include_paths).unwrap())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|err| format!("Could not launch npm start: {}", err))?;

    if !output.status.success() {
        print!("{}", String::from_utf8_lossy(output.stdout.as_ref()));
        print!("{}", String::from_utf8_lossy(output.stderr.as_ref()));
        return Err(String::from_utf8_lossy(output.stderr.as_ref()).to_owned().into());
    }

    Ok(())
}
