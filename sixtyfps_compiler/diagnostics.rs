/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Span represent an error location within a file.
///
/// Currently, it is just an offset in byte within the file.
///
/// When the `proc_macro_span` feature is enabled, it may also hold a proc_macro span.
#[derive(Debug, Clone)]
pub struct Span {
    pub offset: usize,
    #[cfg(feature = "proc_macro_span")]
    pub span: Option<proc_macro::Span>,
}

impl Span {
    pub fn is_valid(&self) -> bool {
        self.offset != usize::MAX
    }

    pub fn new(offset: usize) -> Self {
        Self { offset, ..Default::default() }
    }
}

impl Default for Span {
    fn default() -> Self {
        Span {
            offset: usize::MAX,
            #[cfg(feature = "proc_macro_span")]
            span: Default::default(),
        }
    }
}

impl PartialEq for Span {
    fn eq(&self, other: &Span) -> bool {
        self.offset == other.offset
    }
}

#[cfg(feature = "proc_macro_span")]
impl From<proc_macro::Span> for Span {
    fn from(span: proc_macro::Span) -> Self {
        Self { span: Some(span), ..Default::default() }
    }
}

/// Returns a span.  This is implemented for tokens and nodes
pub trait Spanned {
    fn span(&self) -> Span;
    fn source_file(&self) -> Option<&SourceFile>;
    fn to_source_location(&self) -> SourceLocation {
        SourceLocation { source_file: self.source_file().cloned(), span: self.span() }
    }
}

#[derive(Default)]
pub struct SourceFileInner {
    path: PathBuf,

    /// Complete source code of the path, used to map from offset to line number
    source: Option<String>,

    /// The offset of each linebreak
    line_offsets: once_cell::unsync::OnceCell<Vec<usize>>,
}

impl std::fmt::Debug for SourceFileInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.path)
    }
}

impl SourceFileInner {
    pub fn new(path: PathBuf, source: String) -> Self {
        Self { path, source: Some(source), line_offsets: Default::default() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create a SourceFile that has just a path, but no contents
    pub fn from_path_only(path: PathBuf) -> Rc<Self> {
        Rc::new(Self { path, ..Default::default() })
    }

    fn line_offsets(&self) -> &[usize] {
        self.line_offsets.get_or_init(|| {
            self.source
                .as_ref()
                .map(|s| {
                    s.bytes()
                        .enumerate()
                        .filter_map(|(i, c)| if c == b'\n' { Some(i) } else { None })
                        .collect()
                })
                .unwrap_or_default()
        })
    }
}

pub type SourceFile = Rc<SourceFileInner>;

pub fn load_from_path(path: &Path) -> Result<String, Diagnostic> {
    std::fs::read_to_string(path).map_err(|err| Diagnostic {
        message: format!("Could not load {}: {}", path.display(), err),
        span: SourceLocation {
            source_file: Some(SourceFileInner::from_path_only(path.to_owned())),
            span: Default::default(),
        },
        level: DiagnosticLevel::Error,
    })
}

#[derive(Debug, Clone, Default)]
pub struct SourceLocation {
    pub source_file: Option<SourceFile>,
    pub span: Span,
}

impl Spanned for SourceLocation {
    fn span(&self) -> Span {
        self.span.clone()
    }

    fn source_file(&self) -> Option<&SourceFile> {
        self.source_file.as_ref()
    }
}

impl Spanned for Option<SourceLocation> {
    fn span(&self) -> crate::diagnostics::Span {
        self.as_ref().map(|n| n.span()).unwrap_or_default()
    }

    fn source_file(&self) -> Option<&SourceFile> {
        self.as_ref().map(|n| n.source_file.as_ref()).unwrap_or_default()
    }
}

/// Diagnostics level (error or warning)
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum DiagnosticLevel {
    Error,
    Warning,
}

impl Default for DiagnosticLevel {
    fn default() -> Self {
        Self::Error
    }
}

#[cfg(feature = "display-diagnostics")]
impl From<DiagnosticLevel> for codemap_diagnostic::Level {
    fn from(l: DiagnosticLevel) -> Self {
        match l {
            DiagnosticLevel::Error => codemap_diagnostic::Level::Error,
            DiagnosticLevel::Warning => codemap_diagnostic::Level::Warning,
        }
    }
}

/// This structure represent a diagnostic emitted while compiling .60 code.
///
/// It is basically a message, a level (warning or error), attached to a
/// position in the code
#[derive(Debug, Clone)]
pub struct Diagnostic {
    message: String,
    span: SourceLocation,
    level: DiagnosticLevel,
}

impl Diagnostic {
    /// Return the level for this diagnostic
    pub fn level(&self) -> DiagnosticLevel {
        self.level
    }

    /// Return a message for this diagnostic
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns a tuple with the line (starting at 1) and column number (starting at 0)
    pub fn line_column(&self) -> (usize, usize) {
        let offset = self.span.span.offset;
        let line_offsets = match &self.span.source_file {
            None => return (0, 0),
            Some(sl) => sl.line_offsets(),
        };
        line_offsets.binary_search(&offset).map_or_else(
            |line| {
                if line == 0 {
                    (line + 1, offset)
                } else {
                    (line + 1, line_offsets.get(line - 1).map_or(0, |x| offset - x))
                }
            },
            |line| (line + 1, 0),
        )
    }

    /// return the path of the source file where this error is attached
    pub fn source_file(&self) -> Option<&Path> {
        self.span.source_file().map(|sf| sf.path())
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(sf) = self.span.source_file() {
            let (line, _) = self.line_column();
            write!(f, "{}:{}: {}", sf.path.display(), line, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

#[derive(Default)]
pub struct BuildDiagnostics {
    inner: Vec<Diagnostic>,

    /// This is the list of all loaded files (with or without diagnostic)
    /// does not include the main file.
    /// FIXME: this doesn't really belong in the diagnostics, it should be somehow returned in another way
    /// (maybe in a compilation state that include the diagnostics?)
    pub all_loaded_files: Vec<PathBuf>,
}

impl IntoIterator for BuildDiagnostics {
    type Item = Diagnostic;
    type IntoIter = <Vec<Diagnostic> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl BuildDiagnostics {
    pub fn push_diagnostic_with_span(
        &mut self,
        message: String,
        span: SourceLocation,
        level: DiagnosticLevel,
    ) {
        debug_assert!(
            !message.as_str().ends_with('.'),
            "Error message should not end with a period: ({:?})",
            message
        );
        self.inner.push(Diagnostic { message, span, level }.into());
    }
    pub fn push_error_with_span(&mut self, message: String, span: SourceLocation) {
        self.push_diagnostic_with_span(message, span, DiagnosticLevel::Error)
    }
    pub fn push_error(&mut self, message: String, source: &dyn Spanned) {
        self.push_error_with_span(message, source.to_source_location());
    }
    pub fn push_compiler_error(&mut self, error: Diagnostic) {
        self.inner.push(error.into());
    }

    pub fn push_property_deprecation_warning(
        &mut self,
        old_property: &str,
        new_property: &str,
        source: &dyn Spanned,
    ) {
        self.push_diagnostic_with_span(
            format!(
                "The property '{}' has been deprecated. Please use '{}' instead",
                old_property, new_property
            ),
            source.to_source_location(),
            crate::diagnostics::DiagnosticLevel::Warning,
        )
    }

    /// Return true if there is at least one compilation error for this file
    pub fn has_error(&self) -> bool {
        self.inner.iter().any(|diag| diag.level == DiagnosticLevel::Error)
    }

    /// Return true if there are no diagnostics (warnings or errors); false otherwise.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[cfg(feature = "display-diagnostics")]
    fn call_diagnostics<'a, Output>(
        self,
        output: &'a mut Output,
        mut handle_no_source: Option<&mut dyn FnMut(Diagnostic)>,
        emitter_factory: impl for<'b> FnOnce(
            &'b mut Output,
            Option<&'b codemap::CodeMap>,
        ) -> codemap_diagnostic::Emitter<'b>,
    ) {
        if self.inner.is_empty() {
            return;
        }

        let mut codemap = codemap::CodeMap::new();
        let mut codemap_files = std::collections::HashMap::new();

        let diags: Vec<_> = self
            .inner
            .into_iter()
            .filter_map(|d| {
                let spans = if !d.span.span.is_valid() {
                    vec![]
                } else if let Some(sf) = &d.span.source_file {
                    if let Some(ref mut handle_no_source) = handle_no_source {
                        if sf.source.is_none() {
                            handle_no_source(d);
                            return None;
                        }
                    }
                    let path: String = sf.path.to_string_lossy().into();
                    let file = codemap_files.entry(path).or_insert_with(|| {
                        codemap.add_file(
                            sf.path.to_string_lossy().into(),
                            sf.source.clone().unwrap_or_default(),
                        )
                    });
                    let file_span = file.span;
                    let s = codemap_diagnostic::SpanLabel {
                        span: file_span
                            .subspan(d.span.span.offset as u64, d.span.span.offset as u64),
                        style: codemap_diagnostic::SpanStyle::Primary,
                        label: None,
                    };
                    vec![s]
                } else {
                    vec![]
                };
                Some(codemap_diagnostic::Diagnostic {
                    level: d.level.into(),
                    message: d.message,
                    code: None,
                    spans,
                })
            })
            .collect();

        let mut emitter = emitter_factory(output, Some(&codemap));
        emitter.emit(&diags);
    }

    #[cfg(feature = "display-diagnostics")]
    /// Print the diagnostics on the console
    pub fn print(self) {
        self.call_diagnostics(&mut (), None, |_, codemap| {
            codemap_diagnostic::Emitter::stderr(codemap_diagnostic::ColorConfig::Always, codemap)
        });
    }

    #[cfg(feature = "display-diagnostics")]
    /// Print into a string
    pub fn diagnostics_as_string(self) -> String {
        let mut output = Vec::new();
        self.call_diagnostics(&mut output, None, |output, codemap| {
            codemap_diagnostic::Emitter::vec(output, codemap)
        });

        String::from_utf8(output).expect(
            "Internal error: There were errors during compilation but they did not result in valid utf-8 diagnostics!"
        )
    }

    #[cfg(all(feature = "proc_macro_span", feature = "display-diagnostics"))]
    /// Will convert the diagnostics that only have offsets to the actual proc_macro::Span
    pub fn report_macro_diagnostic(
        self,
        span_map: &[crate::parser::Token],
    ) -> proc_macro::TokenStream {
        let mut result = proc_macro::TokenStream::default();
        let mut needs_error = self.has_error();
        self.call_diagnostics(
            &mut (),
            Some(&mut |diag| {
                let span = diag.span.span.span.clone().or_else(|| {
                    //let pos =
                    //span_map.binary_search_by_key(d.span.offset, |x| x.0).unwrap_or_else(|x| x);
                    //d.span.span = span_map.get(pos).as_ref().map(|x| x.1);
                    let mut offset = 0;
                    span_map.iter().find_map(|t| {
                        if diag.span.span.offset <= offset {
                            t.span
                        } else {
                            offset += t.text.len();
                            None
                        }
                    })
                });
                let message = &diag.message;
                match diag.level {
                    DiagnosticLevel::Error => {
                        needs_error = false;
                        result.extend(proc_macro::TokenStream::from(if let Some(span) = span {
                            quote::quote_spanned!(span.into() => compile_error!{ #message })
                        } else {
                            quote::quote!(compile_error! { #message })
                        }));
                    }
                    // FIXME: find a way to report warnings.
                    DiagnosticLevel::Warning => (),
                }
            }),
            |_, codemap| {
                codemap_diagnostic::Emitter::stderr(
                    codemap_diagnostic::ColorConfig::Always,
                    codemap,
                )
            },
        );
        if needs_error {
            result.extend(proc_macro::TokenStream::from(quote::quote!(
                compile_error! { "Error occurred" }
            )))
        }
        result
    }

    pub fn to_string_vec(&self) -> Vec<String> {
        self.inner.iter().map(|d| d.to_string()).collect()
    }

    pub fn push_diagnostic(
        &mut self,
        message: String,
        source: &dyn Spanned,
        level: DiagnosticLevel,
    ) {
        self.push_diagnostic_with_span(message, source.to_source_location(), level)
    }

    pub fn push_internal_error(&mut self, err: Diagnostic) {
        self.inner.push(err)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.inner.iter()
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn check_and_exit_on_error(self) -> Self {
        if self.has_error() {
            self.print();
            std::process::exit(-1);
        }
        self
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn print_warnings_and_exit_on_error(self) {
        let has_error = self.has_error();
        self.print();
        if has_error {
            std::process::exit(-1);
        }
    }
}
