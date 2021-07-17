/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
# The SixtyFPS compiler library

**NOTE:** This library is an internal crate for the SixtyFPS project.
This crate should not be used directly by application using SixtyFPS.
You should use the `sixtyfps` crate instead

*/

#![doc(html_logo_url = "https://sixtyfps.io/resources/logo.drawio.svg")]
// It would be nice to keep the compiler free of unsafe code
#![deny(unsafe_code)]

#[cfg(feature = "proc_macro_span")]
extern crate proc_macro;

use core::future::Future;
use core::pin::Pin;
use std::cell::RefCell;
use std::rc::Rc;

pub mod builtin_macros;
pub mod diagnostics;
pub mod expression_tree;
pub mod generator;
pub mod langtype;
pub mod layout;
pub mod lexer;
pub mod literals;
pub(crate) mod load_builtins;
pub mod lookup;
pub mod namedreference;
pub mod object_tree;
pub mod parser;
pub mod typeloader;
pub mod typeregister;

mod passes;

/// CompilationConfiguration allows configuring different aspects of the compiler.
#[derive(Clone)]
pub struct CompilerConfiguration {
    /// Indicate whether to embed resources such as images in the generated output or whether
    /// to retain references to the resources on the file system.
    pub embed_resources: bool,
    /// The compiler will look in these paths for components used in the file to compile.
    pub include_paths: Vec<std::path::PathBuf>,
    /// the name of the style. (eg: "native")
    pub style: Option<String>,

    /// Callback to load import files which is called if the file could not be found
    ///
    /// The callback should open the file specified by the given file name and
    /// return an future that provides the text content of the file as output.
    pub open_import_fallback: Option<
        Rc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<std::io::Result<String>>>>>>,
    >,
}

impl CompilerConfiguration {
    pub fn new(output_format: crate::generator::OutputFormat) -> Self {
        let embed_resources = match std::env::var("SIXTYFPS_EMBED_RESOURCES") {
            Ok(var) => {
                var.parse().unwrap_or_else(|_|{
                    panic!("SIXTYFPS_EMBED_RESOURCES has incorrect value. Must be either unset, 'true' or 'false'")
                })
            }
            Err(_) => {
                match output_format {
                    #[cfg(feature = "rust")]
                    crate::generator::OutputFormat::Rust => true,
                    _ => false,
                }
            }
        };

        Self {
            embed_resources,
            include_paths: Default::default(),
            style: Default::default(),
            open_import_fallback: Default::default(),
        }
    }
}

pub async fn compile_syntax_node(
    doc_node: parser::SyntaxNode,
    mut diagnostics: diagnostics::BuildDiagnostics,
    compiler_config: CompilerConfiguration,
) -> (object_tree::Document, diagnostics::BuildDiagnostics) {
    let global_type_registry = typeregister::TypeRegister::builtin();
    let type_registry =
        Rc::new(RefCell::new(typeregister::TypeRegister::new(&global_type_registry)));

    let doc_node: parser::syntax_nodes::Document = doc_node.into();

    let mut loader =
        typeloader::TypeLoader::new(global_type_registry, &compiler_config, &mut diagnostics);
    let foreign_imports =
        loader.load_dependencies_recursively(&doc_node, &mut diagnostics, &type_registry).await;

    let doc = crate::object_tree::Document::from_node(
        doc_node,
        foreign_imports,
        &mut diagnostics,
        &type_registry,
    );

    if let Some((_, node)) = &*doc.root_component.child_insertion_point.borrow() {
        diagnostics
            .push_error("@children placeholder not allowed in the final component".into(), node)
    }

    if !diagnostics.has_error() {
        // FIXME: ideally we would be able to run more passes, but currently we panic because invariant are not met.
        passes::run_passes(&doc, &mut diagnostics, &mut loader, &compiler_config).await;
    }

    diagnostics.all_loaded_files = loader.all_files().cloned().collect();

    (doc, diagnostics)
}

mod library {
    include!(env!("SIXTYFPS_WIDGETS_LIBRARY"));
}
