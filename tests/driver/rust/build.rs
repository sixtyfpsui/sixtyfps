/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::io::Write;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let mut generated_file = std::fs::File::create(
        Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("generated.rs"),
    )?;

    for testcase in test_driver_lib::collect_test_cases()? {
        println!("cargo:rerun-if-changed={}", testcase.absolute_path.display());
        let mut module_name = testcase.identifier();
        if module_name.starts_with(|c: char| !c.is_ascii_alphabetic()) {
            module_name.insert(0, '_');
        }
        let source = std::fs::read_to_string(&testcase.absolute_path)?;

        if source.contains("\\{") {
            // Unfortunately, \{ is not valid in a rust string so it cannot be used in a sixtyfps! macro
            write!(
                generated_file,
                "mod r#{0} {{ #[test] #[ignore] fn ignored_because_string_template() {{}} }}",
                module_name
            )?;
            continue;
        }

        writeln!(generated_file, "#[path=\"{0}.rs\"] mod r#{0};", module_name)?;

        let mut output = std::fs::File::create(
            Path::new(&std::env::var_os("OUT_DIR").unwrap()).join(format!("{}.rs", module_name)),
        )?;

        let include_paths = test_driver_lib::extract_include_paths(&source);

        output.write_all(b"sixtyfps::sixtyfps!{")?;

        for path in include_paths {
            let mut abs_path = testcase.absolute_path.clone();
            abs_path.pop();
            abs_path.push(path);

            output.write_all(b"#[include_path=r#\"")?;
            output.write_all(abs_path.to_string_lossy().as_bytes())?;
            output.write_all(b"\"#]\n")?;

            println!("cargo:rerun-if-changed={}", abs_path.to_string_lossy());
        }

        let mut abs_path = testcase.absolute_path.clone();
        abs_path.pop();
        output.write_all(b"#[include_path=r#\"")?;
        output.write_all(abs_path.to_string_lossy().as_bytes())?;
        output.write_all(b"\"#]\n")?;
        output.write_all(source.as_bytes())?;
        output.write_all(b"}\n")?;

        for (i, x) in test_driver_lib::extract_test_functions(&source)
            .filter(|x| x.language_id == "rust")
            .enumerate()
        {
            write!(
                output,
                r"
#[test] fn t_{}() -> Result<(), Box<dyn std::error::Error>> {{
    sixtyfps_rendering_backend_testing::init();
    {}
    Ok(())
}}",
                i,
                x.source.replace('\n', "\n    ")
            )?;
        }
    }

    Ok(())
}
