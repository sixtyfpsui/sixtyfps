/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use itertools::Itertools;
use sixtyfps_interpreter::{DiagnosticLevel, Value, ValueType};
use std::error::Error;

pub fn test(testcase: &test_driver_lib::TestCase) -> Result<(), Box<dyn Error>> {
    sixtyfps_rendering_backend_testing::init();

    let source = std::fs::read_to_string(&testcase.absolute_path)?;
    let include_paths = test_driver_lib::extract_include_paths(&source)
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let mut compiler = sixtyfps_interpreter::ComponentCompiler::default();
    compiler.set_include_paths(include_paths);

    let component =
        spin_on::spin_on(compiler.build_from_source(source, testcase.absolute_path.clone()));

    let component = match component {
        None => {
            sixtyfps_interpreter::print_diagnostics(&compiler.diagnostics());

            match std::env::var("SIXTYFPS_INTERPRETER_ERROR_WHITELIST") {
                Ok(wl) if !wl.is_empty() => {
                    let errors = compiler
                        .diagnostics()
                        .iter()
                        .filter(|d| d.level() == DiagnosticLevel::Error)
                        .collect::<Vec<_>>();
                    if !errors.is_empty() && errors.iter().all(|d| d.message().contains(&wl)) {
                        eprintln!(
                            "{}: Ignoring Error because of the error whitelist!",
                            testcase.relative_path.display()
                        );
                        return Ok(());
                    }
                }
                _ => {}
            }

            return Err(compiler
                .diagnostics()
                .into_iter()
                .map(|d| d.to_string())
                .join("\n")
                .into());
        }
        Some(c) => c,
    };

    let instance = component.create();

    if let Some((_, ty)) = component.properties().find(|x| x.0 == "test") {
        if ty == ValueType::Bool {
            let result = instance.get_property("test")?;
            if result != Value::Bool(true) {
                eprintln!("FAIL: {}: test returned {:?}", testcase.relative_path.display(), result);
                eprintln!("Property list:");
                for (p, _) in component.properties() {
                    eprintln!(" {}: {:?}", p, instance.get_property(&p));
                }
                panic!("Test Failed: {:?}", result);
            }
        }
    }

    Ok(())
}
