/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

// cspell:ignore cppdocs pipenv pipfile

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn symlink_file<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> Result<()> {
    if dst.as_ref().exists() {
        std::fs::remove_file(dst.as_ref()).context("Error removing old symlink")?;
    }
    #[cfg(target_os = "windows")]
    return std::os::windows::fs::symlink_file(&src, &dst).context("Error creating symlink");
    #[cfg(not(target_os = "windows"))]
    return std::os::unix::fs::symlink(&src, &dst).context(format!(
        "Error creating symlink from {} to {}",
        src.as_ref().display(),
        dst.as_ref().display()
    ));
}

fn symlink_dir<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> Result<()> {
    if dst.as_ref().exists() {
        std::fs::remove_dir_all(dst.as_ref()).context("Error removing old symlink")?;
    }
    #[cfg(target_os = "windows")]
    return std::os::windows::fs::symlink_dir(&src, &dst).context("Error creating symlink");
    #[cfg(not(target_os = "windows"))]
    return std::os::unix::fs::symlink(&src, &dst).context(format!(
        "Error creating symlink from {} to {}",
        src.as_ref().display(),
        dst.as_ref().display()
    ));
}

fn symlink_files_in_dir<S: AsRef<Path>, T: AsRef<Path>, TS: AsRef<Path>>(
    src: S,
    target: T,
    target_to_source: TS,
) -> Result<()> {
    for entry in std::fs::read_dir(src.as_ref()).context("Error reading docs source directory")? {
        let entry = entry.context("Error reading directory entry")?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
        let symlink_source = target_to_source.as_ref().to_path_buf().join(&file_name);
        let symlink_target = target.as_ref().to_path_buf().join(path.file_name().unwrap());
        if path.is_file() {
            symlink_file(symlink_source, symlink_target).context("Could not symlink file")?;
        } else if path.is_dir() {
            symlink_dir(symlink_source, symlink_target).context("Could not symlink directory")?;
        }
    }
    Ok(())
}

pub fn generate(show_warnings: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = super::root_dir();

    let docs_source_dir = root.join("api/sixtyfps-cpp");
    let docs_build_dir = root.join("target/cppdocs");
    let html_static_dir = docs_build_dir.join("_static");

    std::fs::create_dir_all(docs_build_dir.as_path()).context("Error creating docs build dir")?;
    std::fs::create_dir_all(html_static_dir.as_path())
        .context("Error creating _static path for docs build")?;

    symlink_files_in_dir(
        docs_source_dir.join("docs"),
        &docs_build_dir,
        ["..", "..", "api", "sixtyfps-cpp", "docs"].iter().collect::<PathBuf>(),
    )
    .context("Error creating symlinks from docs source to docs build dir")?;

    symlink_dir(["..", "..", "docs"].iter().collect::<PathBuf>(), docs_build_dir.join("markdown"))?;

    symlink_file(
        ["..", "..", "api", "sixtyfps-cpp", "README.md"].iter().collect::<PathBuf>(),
        docs_build_dir.join("README.md"),
    )?;

    let pip_env = vec![(OsString::from("PIPENV_PIPFILE"), docs_source_dir.join("docs/Pipfile"))];

    println!("Running pipenv install");

    super::run_command("pipenv", &["install"], pip_env.clone())
        .context("Error running pipenv install")?;

    println!("Running sphinx-build");

    let output = super::run_command(
        "pipenv",
        &[
            "run",
            "sphinx-build",
            docs_build_dir.to_str().unwrap(),
            docs_build_dir.join("html").to_str().unwrap(),
        ],
        pip_env,
    )
    .context("Error running pipenv install")?;

    println!("{}", String::from_utf8_lossy(&output.stdout));

    if show_warnings {
        println!("{}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}
