/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::fs::read_dir;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> std::io::Result<()> {
    let mut library_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    library_dir.push("widgets");

    println!("cargo:rerun-if-changed={}", library_dir.display());

    let output_file_path = Path::new(&std::env::var_os("OUT_DIR").unwrap())
        .join(Path::new("included_library").with_extension("rs"));

    let mut file = std::fs::File::create(&output_file_path)?;
    write!(
        file,
        r#"
use crate::typeloader::{{VirtualFile, VirtualDirectory}};
pub fn widget_library() -> &'static [(&'static str, &'static VirtualDirectory<'static>)] {{
    &[
"#
    )?;

    for style in read_dir(library_dir)?.filter_map(Result::ok) {
        let path = style.path();
        if !path.is_dir() {
            continue;
        }
        writeln!(
            file,
            "(\"{}\", &[{}]),",
            path.file_name().unwrap().to_string_lossy(),
            process_style(&path)?
        )?;
    }

    writeln!(file, "]\n}}")?;

    println!("cargo:rustc-env=SIXTYFPS_WIDGETS_LIBRARY={}", output_file_path.display());

    Ok(())
}

fn process_style(path: &Path) -> std::io::Result<String> {
    let library_files: Vec<PathBuf> = read_dir(path)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_file()
                && entry.path().extension().unwrap_or_default() == std::ffi::OsStr::new("60")
        })
        .map(|entry| entry.path())
        .collect();

    Ok(library_files
        .iter()
        .map(|file| {
            format!(
                "&VirtualFile {{path: r#\"{}\"# , contents: include_str!(r#\"{}\"#)}}",
                file.file_name().unwrap().to_string_lossy(),
                file.display()
            )
        })
        .collect::<Vec<_>>()
        .join(","))
}
