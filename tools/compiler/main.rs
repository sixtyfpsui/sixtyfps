/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use sixtyfps_compilerlib::diagnostics::BuildDiagnostics;
use sixtyfps_compilerlib::*;
use std::io::Write;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Cli {
    /// Set output format
    #[structopt(short = "f", long = "format", default_value = "cpp")]
    format: generator::OutputFormat,

    /// Include path for other .60 files
    #[structopt(short = "I", name = "include path", number_of_values = 1)]
    include_paths: Vec<std::path::PathBuf>,

    /// Path to .60 file
    #[structopt(name = "file", parse(from_os_str))]
    path: std::path::PathBuf,

    /// Generate a dependency file
    #[structopt(
        name = "dependency file",
        long = "depfile",
        number_of_values = 1,
        parse(from_os_str)
    )]
    depfile: Option<std::path::PathBuf>,

    /// Sets the output file ('-' for stdout)
    #[structopt(name = "file to generate", short = "o", default_value = "-")]
    output: std::path::PathBuf,
}

fn main() -> std::io::Result<()> {
    proc_macro2::fallback::force(); // avoid a abort if panic=abort is set
    let args = Cli::from_args();
    let mut diag = BuildDiagnostics::default();
    let syntax_node = parser::parse_file(&args.path, &mut diag);
    //println!("{:#?}", syntax_node);
    if diag.has_error() {
        diag.print();
        std::process::exit(-1);
    }
    let mut compiler_config = CompilerConfiguration::new(args.format);
    compiler_config.include_paths = args.include_paths;
    let syntax_node = syntax_node.expect("diags contained no compilation errors");
    let (doc, diag) = spin_on::spin_on(compile_syntax_node(syntax_node, diag, compiler_config));

    let mut diag = diag.check_and_exit_on_error();

    if args.output == std::path::Path::new("-") {
        generator::generate(args.format, &mut std::io::stdout(), &doc, &mut diag)?;
    } else {
        generator::generate(
            args.format,
            &mut std::fs::File::create(&args.output)?,
            &doc,
            &mut diag,
        )?;
    }

    if let Some(depfile) = args.depfile {
        let mut f = std::fs::File::create(depfile)?;
        write!(f, "{}:", args.output.display())?;
        for x in &diag.all_loaded_files {
            if x.is_absolute() {
                write!(f, " {}", x.display())?;
            }
        }
        for resource in doc.root_component.embedded_file_resources.borrow().keys() {
            write!(f, " {}", resource)?;
        }

        writeln!(f, "")?;
    }
    diag.print_warnings_and_exit_on_error();
    Ok(())
}
