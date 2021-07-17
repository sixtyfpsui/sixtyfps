/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This crate serves as a companion crate for the sixtyfps crate.
It is meant to allow you to compile the `.60` files from your `build.rs`script.

The main entry point of this crate is the [`compile()`] function

## Example

In your Cargo.toml:

```toml
[package]
...
build = "build.rs"

[dependencies]
sixtyfps = "0.1.0"
...

[build-dependencies]
sixtyfps-build = "0.1.0"
```

In the `build.rs` file:

```ignore
fn main() {
    sixtyfps_build::compile("ui/hello.60").unwrap();
}
```

Then in your main file

```ignore
sixtyfps::include_modules!();
fn main() {
    HelloWorld::new().run();
}
```
*/
#![doc(html_logo_url = "https://sixtyfps.io/resources/logo.drawio.svg")]
#![warn(missing_docs)]

use std::env;
use std::io::Write;
use std::path::Path;

use sixtyfps_compilerlib::diagnostics::BuildDiagnostics;

/// The structure for configuring aspects of the compilation of `.60` markup files to Rust.
pub struct CompilerConfiguration {
    config: sixtyfps_compilerlib::CompilerConfiguration,
}

impl Default for CompilerConfiguration {
    fn default() -> Self {
        Self {
            config: sixtyfps_compilerlib::CompilerConfiguration::new(
                sixtyfps_compilerlib::generator::OutputFormat::Rust,
            ),
        }
    }
}

impl CompilerConfiguration {
    /// Creates a new default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new configuration that includes sets the include paths used for looking up
    /// `.60` imports to the specified vector of paths.
    pub fn with_include_paths(self, include_paths: Vec<std::path::PathBuf>) -> Self {
        let mut config = self.config;
        config.include_paths = include_paths;
        Self { config }
    }

    /// Create a new configuration that selects the style to be used for widgets.
    pub fn with_style(self, style: String) -> Self {
        let mut config = self.config;
        config.style = Some(style);
        Self { config }
    }
}

/// Error returned by the `compile` function
#[derive(thiserror::Error, Debug)]
pub enum CompileError {
    /// Cannot read environment variable CARGO_MANIFEST_DIR or OUT_DIR. The build script need to be run via cargo.
    #[error("Cannot read environment variable CARGO_MANIFEST_DIR or OUT_DIR. The build script need to be run via cargo.")]
    NotRunViaCargo,
    /// Parse error. The error are printed in the stderr, and also are in the vector
    #[error("{0:?}")]
    CompileError(Vec<String>),
    /// Cannot write the generated file
    #[error("Cannot write the generated file: {0}")]
    SaveError(std::io::Error),
}

struct CodeFormatter<Sink> {
    indentation: usize,
    in_string: bool,
    sink: Sink,
}

impl<Sink: Write> Write for CodeFormatter<Sink> {
    fn write(&mut self, mut s: &[u8]) -> std::io::Result<usize> {
        let len = s.len();
        while let Some(idx) = s.iter().position(|c| match c {
            b'{' if !self.in_string => {
                self.indentation += 1;
                true
            }
            b'}' if !self.in_string => {
                self.indentation -= 1;
                true
            }
            b';' if !self.in_string => true,
            b'"' if !self.in_string => {
                self.in_string = true;
                false
            }
            b'"' if self.in_string => {
                // FIXME! escape character
                self.in_string = false;
                false
            }
            _ => false,
        }) {
            let idx = idx + 1;
            self.sink.write_all(&s[..idx])?;
            self.sink.write_all(b"\n")?;
            for _ in 0..self.indentation {
                self.sink.write_all(b"    ")?;
            }
            s = &s[idx..];
        }
        self.sink.write_all(s)?;
        Ok(len)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.sink.flush()
    }
}

/// Compile the `.60` file and generate rust code for it.
///
/// The generated code code will be created in the directory specified by
/// the `OUT` environment variable as it is expected for build script.
///
/// The following line need to be added within your crate in order to include
/// the generated code.
/// ```ignore
/// sixtyfps::include_modules!();
/// ```
///
/// The path is relative to the `CARGO_MANIFEST_DIR`.
///
/// In case of compilation error, the errors are shown in `stderr`, the error
/// are also returned in the [`CompileError`] enum. You must `unwrap` the returned
/// result to make sure that cargo make the compilation fail in case there were
/// errors when generating the code.
///
/// Please check out the documentation of the `sixtyfps` crate for more information
/// about how to use the generated code.
pub fn compile(path: impl AsRef<std::path::Path>) -> Result<(), CompileError> {
    compile_with_config(path, CompilerConfiguration::default())
}

/// Same as [`compile`], but allow to specify a configuration.
pub fn compile_with_config(
    path: impl AsRef<std::path::Path>,
    config: CompilerConfiguration,
) -> Result<(), CompileError> {
    let path = Path::new(&env::var_os("CARGO_MANIFEST_DIR").ok_or(CompileError::NotRunViaCargo)?)
        .join(path.as_ref());

    let mut diag = BuildDiagnostics::default();
    let syntax_node = sixtyfps_compilerlib::parser::parse_file(&path, &mut diag);

    if diag.has_error() {
        let vec = diag.to_string_vec();
        diag.print();
        return Err(CompileError::CompileError(vec));
    }

    let mut compiler_config = config.config;

    if let (Ok(target), Ok(host)) = (env::var("TARGET"), env::var("HOST")) {
        if target != host {
            compiler_config.embed_resources = true;
        }
    };

    let syntax_node = syntax_node.expect("diags contained no compilation errors");

    // 'spin_on' is ok here because the compiler in single threaded and does not block if there is no blocking future
    let (doc, mut diag) = spin_on::spin_on(sixtyfps_compilerlib::compile_syntax_node(
        syntax_node,
        diag,
        compiler_config,
    ));

    if diag.has_error() {
        let vec = diag.to_string_vec();
        diag.print();
        return Err(CompileError::CompileError(vec));
    }

    let output_file_path = Path::new(&env::var_os("OUT_DIR").ok_or(CompileError::NotRunViaCargo)?)
        .join(
            path.file_stem()
                .map(Path::new)
                .unwrap_or(Path::new("sixtyfps_out"))
                .with_extension("rs"),
        );

    let file = std::fs::File::create(&output_file_path).map_err(CompileError::SaveError)?;
    let mut code_formatter = CodeFormatter { indentation: 0, in_string: false, sink: file };
    let generated = match sixtyfps_compilerlib::generator::rust::generate(&doc, &mut diag) {
        Some(code) => {
            for x in &diag.all_loaded_files {
                if x.is_absolute() {
                    println!("cargo:rerun-if-changed={}", x.display());
                }
            }

            // print warnings
            diag.diagnostics_as_string().lines().for_each(|w| {
                if !w.is_empty() {
                    println!("cargo:warning={}", w.strip_prefix("warning: ").unwrap_or(w))
                }
            });
            code
        }
        None => {
            let vec = diag.to_string_vec();
            diag.print();
            return Err(CompileError::CompileError(vec));
        }
    };

    write!(code_formatter, "{}", generated).map_err(CompileError::SaveError)?;
    println!("cargo:rerun-if-changed={}", path.display());

    for resource in doc.root_component.embedded_file_resources.borrow().keys() {
        println!("cargo:rerun-if-changed={}", resource);
    }
    println!("cargo:rerun-if-env-changed=SIXTYFPS_STYLE");

    println!("cargo:rustc-env=SIXTYFPS_INCLUDE_GENERATED={}", output_file_path.display());
    Ok(())
}
