/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
    Work in progress for a formatter.
    Use like this to format a file:
    ```sh
        cargo run sixtyfps-fmt -- -i some_file.60
    ```

    Some code in this main.rs file is duplicated with the syntax_updater, i guess it could
    be refactored in a separate utility crate or module or something.

    The [`TokenWriter`] trait is meant to be able to support the LSP later as the
    LSP wants just the edits , not the full file
*/

use sixtyfps_compilerlib::diagnostics::BuildDiagnostics;
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxNode, SyntaxToken};
use std::io::Write;
use structopt::StructOpt;

mod fmt;

#[derive(StructOpt)]
struct Cli {
    #[structopt(name = "path to .60 file(s)", parse(from_os_str))]
    paths: Vec<std::path::PathBuf>,

    /// modify the file inline instead of printing to stdout
    #[structopt(short, long)]
    inline: bool,
}

fn main() -> std::io::Result<()> {
    let args = Cli::from_args();

    for path in args.paths {
        let source = std::fs::read_to_string(&path)?;

        if args.inline {
            let file = std::fs::File::create(&path)?;
            process_file(source, path, file)?
        } else {
            process_file(source, path, std::io::stdout())?
        }
    }
    Ok(())
}

/// FIXME! this is duplicated with the updater
fn process_rust_file(source: String, mut file: impl Write) -> std::io::Result<()> {
    let mut source_slice = &source[..];
    let sixtyfps_macro = format!("{}!", "sixtyfps"); // in a variable so it does not appear as is
    'l: while let Some(idx) = source_slice.find(&sixtyfps_macro) {
        // Note: this code ignore string literal and unbalanced comment, but that should be good enough
        let idx2 =
            if let Some(idx2) = source_slice[idx..].find(|c| c == '{' || c == '(' || c == '[') {
                idx2
            } else {
                break 'l;
            };
        let open = source_slice.as_bytes()[idx + idx2].into();
        let close = match open {
            '{' => '}',
            '(' => ')',
            '[' => ']',
            _ => panic!(),
        };
        file.write_all(source_slice[..=idx + idx2].as_bytes())?;
        source_slice = &source_slice[idx + idx2 + 1..];
        let mut idx = 0;
        let mut count = 1;
        while count > 0 {
            if let Some(idx2) = source_slice[idx..].find(|c| {
                if c == open {
                    count += 1;
                    true
                } else if c == close {
                    count -= 1;
                    true
                } else {
                    false
                }
            }) {
                idx += idx2 + 1;
            } else {
                break 'l;
            }
        }
        let code = &source_slice[..idx - 1];
        source_slice = &source_slice[idx - 1..];

        let mut diag = BuildDiagnostics::default();
        let syntax_node = sixtyfps_compilerlib::parser::parse(code.to_owned(), None, &mut diag);
        let len = syntax_node.text_range().end().into();
        visit_node(syntax_node, &mut file, &mut State::default())?;
        if diag.has_error() {
            file.write_all(&code.as_bytes()[len..])?;
            diag.print();
        }
    }
    return file.write_all(source_slice.as_bytes());
}

/// FIXME! this is duplicated with the updater
fn process_markdown_file(source: String, mut file: impl Write) -> std::io::Result<()> {
    let mut source_slice = &source[..];
    const CODE_FENCE_START: &'static str = "```60\n";
    const CODE_FENCE_END: &'static str = "```\n";
    'l: while let Some(code_start) =
        source_slice.find(CODE_FENCE_START).map(|idx| idx + CODE_FENCE_START.len())
    {
        let code_end = if let Some(code_end) = source_slice[code_start..].find(CODE_FENCE_END) {
            code_end
        } else {
            break 'l;
        };
        file.write_all(source_slice[..=code_start - 1].as_bytes())?;
        source_slice = &source_slice[code_start..];
        let code = &source_slice[..code_end];
        source_slice = &source_slice[code_end..];

        let mut diag = BuildDiagnostics::default();
        let syntax_node = sixtyfps_compilerlib::parser::parse(code.to_owned(), None, &mut diag);
        let len = syntax_node.text_range().end().into();
        visit_node(syntax_node, &mut file, &mut State::default())?;
        if diag.has_error() {
            file.write_all(&code.as_bytes()[len..])?;
            diag.print();
        }
    }
    return file.write_all(source_slice.as_bytes());
}

fn process_file(
    source: String,
    path: std::path::PathBuf,
    mut file: impl Write,
) -> std::io::Result<()> {
    match path.extension() {
        Some(ext) if ext == "rs" => return process_rust_file(source, file),
        Some(ext) if ext == "md" => return process_markdown_file(source, file),
        _ => {}
    }

    let mut diag = BuildDiagnostics::default();
    let syntax_node = sixtyfps_compilerlib::parser::parse(source.clone(), Some(&path), &mut diag);
    let len = syntax_node.node.text_range().end().into();
    visit_node(syntax_node, &mut file, &mut State::default())?;
    if diag.has_error() {
        file.write_all(&source.as_bytes()[len..])?;
        diag.print();
    }
    Ok(())
}

type State = ();

fn visit_node(node: SyntaxNode, file: &mut impl Write, _state: &mut State) -> std::io::Result<()> {
    if let Some(doc) = syntax_nodes::Document::new(node) {
        let mut writer = FileWriter { file };
        fmt::format_document(doc, &mut writer)
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Not a Document"))
    }
}

/// The idea is that each token need to go through this, either with no changes,
/// or with a new content.
trait TokenWriter {
    fn no_change(&mut self, token: SyntaxToken) -> std::io::Result<()>;
    fn with_new_content(&mut self, token: SyntaxToken, contents: &str) -> std::io::Result<()>;
    fn insert_before(&mut self, token: SyntaxToken, contents: &str) -> std::io::Result<()>;
}

/// Just write the token stream to a file
struct FileWriter<'a, W> {
    file: &'a mut W,
}

impl<'a, W: Write> TokenWriter for FileWriter<'a, W> {
    fn no_change(&mut self, token: SyntaxToken) -> std::io::Result<()> {
        self.file.write_all(token.text().as_bytes())
    }

    fn with_new_content(&mut self, _token: SyntaxToken, contents: &str) -> std::io::Result<()> {
        self.file.write_all(contents.as_bytes())
    }

    fn insert_before(&mut self, token: SyntaxToken, contents: &str) -> std::io::Result<()> {
        self.file.write_all(contents.as_bytes())?;
        self.file.write_all(token.text().as_bytes())
    }
}
