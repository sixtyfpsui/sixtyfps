/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! This test is trying to compile all the *.60 files in the sub directories and check that compilation
//! errors are properly reported
//!
//! The compiler can have comments like this:
//! ```
//!  // ^error{some_regexp}
//! ```

use std::path::{Path, PathBuf};

#[test]
fn syntax_tests() -> std::io::Result<()> {
    if let Some(specific_test) = std::env::args()
        .skip(1)
        .skip_while(|arg| arg.starts_with("--") || arg == "syntax_tests")
        .next()
    {
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push(specific_test);
        assert!(process_file(&path)?);
        return Ok(());
    }
    let mut success = true;
    for entry in std::fs::read_dir(format!("{}/tests/syntax", env!("CARGO_MANIFEST_DIR")))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            for test_entry in path.read_dir()? {
                let test_entry = test_entry?;
                let path = test_entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "60" {
                        success &= process_file(&path)?;
                    }
                }
            }
        }
    }
    assert!(success);
    Ok(())
}

fn process_file(path: &std::path::Path) -> std::io::Result<bool> {
    let source = std::fs::read_to_string(&path)?;
    std::panic::catch_unwind(|| process_file_source(path, source, false)).unwrap_or_else(|err| {
        println!("Panic while processing {}: {:?}", path.display(), err);
        Ok(false)
    })
}

fn process_diagnostics(
    compile_diagnostics: &sixtyfps_compilerlib::diagnostics::BuildDiagnostics,
    path: &Path,
    source: &str,
    silent: bool,
) -> std::io::Result<bool> {
    let mut success = true;

    let path = canonical(path);

    let mut diags = compile_diagnostics
        .iter()
        .filter(|d| canonical(d.source_file().unwrap()) == path)
        .collect::<Vec<_>>();

    let lines = source
        .bytes()
        .enumerate()
        .filter_map(|(i, c)| if c == b'\n' { Some(i) } else { None })
        .collect::<Vec<usize>>();

    // Find expected errors in the file. The first caret (^) points to the expected column. The number of
    // carets refers to the number of lines to go back. This is useful when one line of code produces multiple
    // errors or warnings.
    let re = regex::Regex::new(r"\n *//[^\n\^]*(\^+)(error|warning)\{([^\n]*)\}").unwrap();
    for m in re.captures_iter(&source) {
        let line_begin_offset = m.get(0).unwrap().start();
        let column = m.get(1).unwrap().start() - line_begin_offset;
        let lines_to_source = m.get(1).unwrap().as_str().len();
        let warning_or_error = m.get(2).unwrap().as_str();
        let rx = m.get(3).unwrap().as_str();
        let r = match regex::Regex::new(&rx) {
            Err(e) => {
                eprintln!("{:?}: Invalid regexp {:?} : {:?}", path, rx, e);
                return Ok(false);
            }
            Ok(r) => r,
        };

        let mut line_counter = 0;
        let mut line_offset = source[..line_begin_offset].rfind('\n').unwrap_or(0);
        let offset = loop {
            line_counter += 1;
            if line_counter >= lines_to_source {
                break line_offset;
            }
            line_offset = source[..line_offset].rfind('\n').unwrap_or(0);
        } + column;

        let expected_diag_level = match warning_or_error {
            "warning" => sixtyfps_compilerlib::diagnostics::DiagnosticLevel::Warning,
            "error" => sixtyfps_compilerlib::diagnostics::DiagnosticLevel::Error,
            _ => panic!("Unsupported diagnostic level {}", warning_or_error),
        };

        match diags.iter().position(|e| {
            let (l, c) = e.line_column();
            let o = lines.get(l.wrapping_sub(2)).unwrap_or(&0) + c;
            o == offset && r.is_match(e.message()) && e.level() == expected_diag_level
        }) {
            Some(idx) => {
                diags.remove(idx);
            }
            None => {
                success = false;
                println!(
                    "{:?}: {} not found at offset {}: {:?}",
                    path, warning_or_error, offset, rx
                );
            }
        }
    }
    if !diags.is_empty() {
        println!("{:?}: Unexpected errors/warnings: {:#?}", path, diags);

        #[cfg(feature = "display-diagnostics")]
        if !silent {
            let mut to_report = sixtyfps_compilerlib::diagnostics::BuildDiagnostics::default();
            for d in diags {
                to_report.push_compiler_error(d.clone());
            }
            to_report.print();
        }

        success = false;
    }
    Ok(success)
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

fn process_file_source(
    path: &std::path::Path,
    source: String,
    silent: bool,
) -> std::io::Result<bool> {
    let mut parse_diagnostics = sixtyfps_compilerlib::diagnostics::BuildDiagnostics::default();
    let syntax_node =
        sixtyfps_compilerlib::parser::parse(source.clone(), Some(path), &mut parse_diagnostics);
    let has_parse_error = parse_diagnostics.has_error();
    let mut compiler_config = sixtyfps_compilerlib::CompilerConfiguration::new(
        sixtyfps_compilerlib::generator::OutputFormat::Interpreter,
    );
    compiler_config.style = Some("ugly".into());
    let compile_diagnostics = if !parse_diagnostics.has_error() {
        let (_, build_diags) = spin_on::spin_on(sixtyfps_compilerlib::compile_syntax_node(
            syntax_node.clone(),
            parse_diagnostics,
            compiler_config.clone(),
        ));
        build_diags
    } else {
        parse_diagnostics
    };

    let mut success = true;
    success &= process_diagnostics(&compile_diagnostics, &path, &source, silent)?;

    for p in &compile_diagnostics.all_loaded_files {
        let source = if p.is_absolute() {
            std::fs::read_to_string(&p)?
        } else {
            // probably sixtyfps_widgets.60
            String::new()
        };
        success &= process_diagnostics(&compile_diagnostics, &p, &source, silent)?;
    }

    if has_parse_error {
        // Still try to compile to make sure it doesn't panic
        spin_on::spin_on(sixtyfps_compilerlib::compile_syntax_node(
            syntax_node,
            compile_diagnostics,
            compiler_config,
        ));
    }

    Ok(success)
}

#[test]
/// Test that this actually fail when it should
fn self_test() -> std::io::Result<()> {
    let fake_path = std::path::Path::new("fake.60");
    let process = |str: &str| process_file_source(&fake_path, str.into(), true);

    // this should succeed
    assert!(process(
        r#"
Foo := Rectangle { x: 0px; }
    "#
    )?);

    // unless we expected an error
    assert!(!process(
        r#"
Foo := Rectangle { x: 0px; }
//     ^error{i want an error}
    "#
    )?);

    // An error should fail
    assert!(!process(
        r#"
Foo := Rectangle foo { x:0px; }
    "#
    )?);

    // An error with the proper comment should pass
    assert!(process(
        r#"
Foo := Rectangle foo { x:0px; }
//               ^error{expected LBrace}
    "#
    )?);

    // But not if it is at the wrong position
    assert!(!process(
        r#"
Foo := Rectangle foo { x:0px; }
//             ^error{expected LBrace}
    "#
    )?);

    // or the wrong line
    assert!(!process(
        r#"
Foo := Rectangle foo { x:0px; }

//               ^error{expected LBrace}
    "#
    )?);

    // or the wrong message
    assert!(!process(
        r#"
Foo := Rectangle foo { x:0px; }
//               ^error{foo_bar}
    "#
    )?);

    // or the wrong line because two carets
    assert!(!process(
        r#"

Foo := Rectangle foo { x:0px; }
//               ^^error{expected LBrace}
    "#
    )?);

    // Even on windows, it should work
    assert!(process(
        "\r\nFoo := Rectangle foo { x:0px; }\r\n//               ^error{expected LBrace}\r\n"
    )?);

    Ok(())
}
