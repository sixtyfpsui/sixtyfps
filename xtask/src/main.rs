/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use anyhow::Context;
use std::error::Error;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

mod cbindgen;
mod cppdocs;
mod license_headers_check;
mod nodepackage;

#[derive(Debug, StructOpt)]
pub enum TaskCommand {
    #[structopt(name = "check_license_headers")]
    CheckLicenseHeaders(license_headers_check::LicenseHeaderCheck),
    #[structopt(name = "cppdocs")]
    CppDocs(CppDocsCommand),
    #[structopt(name = "cbindgen")]
    Cbindgen(CbindgenCommand),
    #[structopt(name = "node_package")]
    NodePackage,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "xtask")]
pub struct ApplicationArguments {
    #[structopt(subcommand)]
    pub command: TaskCommand,
}

#[derive(Debug, StructOpt)]
pub struct CbindgenCommand {
    #[structopt(long)]
    output_dir: String,
}

#[derive(Debug, StructOpt)]
pub struct CppDocsCommand {
    #[structopt(long)]
    show_warnings: bool,
}

/// The root dir of the git repository
fn root_dir() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop(); // $root/xtask -> $root
    root
}

struct CommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_command<I, K, V>(program: &str, args: &[&str], env: I) -> anyhow::Result<CommandOutput>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<std::ffi::OsStr>,
    V: AsRef<std::ffi::OsStr>,
{
    let cmdline = || format!("{} {}", program, args.join(" "));
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(root_dir())
        .envs(env)
        .output()
        .with_context(|| format!("Error launching {}", cmdline()))?;
    let code = output
        .status
        .code()
        .with_context(|| format!("Command received callback: {}", cmdline()))?;
    if code != 0 {
        Err(anyhow::anyhow!(
            "Command {} exited with non-zero status: {}\nstdout: {}\nstderr: {}",
            cmdline(),
            code,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    } else {
        Ok(CommandOutput { stderr: output.stderr, stdout: output.stdout })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    match ApplicationArguments::from_args().command {
        TaskCommand::CheckLicenseHeaders(cmd) => cmd.check_license_headers()?,
        TaskCommand::CppDocs(cmd) => cppdocs::generate(cmd.show_warnings)?,
        TaskCommand::Cbindgen(cmd) => cbindgen::gen_all(&root_dir(), Path::new(&cmd.output_dir))?,
        TaskCommand::NodePackage => nodepackage::generate()?,
    };

    Ok(())
}
