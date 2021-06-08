/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2020 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2020 Simon Hausmann <simon.hausmann@sixtyfps.io>

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

mod passes {
    pub mod apply_default_properties_from_style;
    pub mod binding_analysis;
    pub mod check_expressions;
    pub mod check_public_api;
    pub mod clip;
    pub mod collect_custom_fonts;
    pub mod collect_globals;
    pub mod collect_structs;
    pub mod compile_paths;
    pub mod deduplicate_property_read;
    pub mod default_geometry;
    pub mod embed_resources;
    pub mod ensure_window;
    pub mod flickable;
    pub mod focus_item;
    pub mod generate_item_indices;
    pub mod infer_aliases_types;
    pub mod inlining;
    pub mod lower_layout;
    pub mod lower_popups;
    pub mod lower_shadows;
    pub mod lower_states;
    pub mod materialize_fake_properties;
    pub mod move_declarations;
    pub mod remove_aliases;
    pub mod repeater_component;
    pub mod resolve_native_classes;
    pub mod resolving;
    pub mod transform_and_opacity;
    pub mod unique_id;
    pub mod z_order;
}

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
        run_passes(&doc, &mut diagnostics, &mut loader, &compiler_config).await;
    }

    diagnostics.all_loaded_files = loader.all_files().cloned().collect();

    (doc, diagnostics)
}

pub async fn run_passes(
    doc: &object_tree::Document,
    diag: &mut diagnostics::BuildDiagnostics,
    mut type_loader: &mut typeloader::TypeLoader<'_>,
    compiler_config: &CompilerConfiguration,
) {
    let global_type_registry = type_loader.global_type_registry.clone();
    passes::infer_aliases_types::resolve_aliases(doc, diag);
    passes::resolving::resolve_expressions(doc, &type_loader, diag);
    passes::check_expressions::check_expressions(doc, diag);
    passes::check_public_api::check_public_api(&doc.root_component, diag);
    passes::inlining::inline(doc);
    passes::compile_paths::compile_paths(&doc.root_component, &doc.local_registry, diag);
    passes::unique_id::assign_unique_id(&doc.root_component);
    passes::focus_item::resolve_element_reference_in_set_focus_calls(&doc.root_component, diag);
    passes::focus_item::determine_initial_focus_item(&doc.root_component, diag);
    passes::focus_item::erase_forward_focus_properties(&doc.root_component);
    passes::flickable::handle_flickable(&doc.root_component, &global_type_registry.borrow());
    if compiler_config.embed_resources {
        passes::embed_resources::embed_resources(&doc.root_component);
    }
    passes::lower_states::lower_states(&doc.root_component, &doc.local_registry, diag);
    passes::repeater_component::process_repeater_components(&doc.root_component);
    passes::lower_popups::lower_popups(&doc.root_component, &doc.local_registry, diag);
    passes::lower_layout::lower_layouts(&doc.root_component, &mut type_loader, diag).await;
    passes::z_order::reorder_by_z_order(&doc.root_component, diag);
    passes::lower_shadows::lower_shadow_properties(&doc.root_component, &doc.local_registry, diag);
    passes::clip::handle_clip(&doc.root_component, &global_type_registry.borrow(), diag);
    passes::transform_and_opacity::handle_transform_and_opacity(
        &doc.root_component,
        &global_type_registry.borrow(),
        diag,
    );
    passes::default_geometry::default_geometry(&doc.root_component, diag);
    passes::materialize_fake_properties::materialize_fake_properties(&doc.root_component);
    passes::apply_default_properties_from_style::apply_default_properties_from_style(
        &doc.root_component,
        &mut type_loader,
        diag,
    )
    .await;
    passes::collect_globals::collect_globals(&doc.root_component, diag);
    passes::binding_analysis::binding_analysis(&doc.root_component, diag);
    passes::deduplicate_property_read::deduplicate_property_read(&doc.root_component);
    passes::move_declarations::move_declarations(&doc.root_component, diag);
    passes::remove_aliases::remove_aliases(&doc.root_component, diag);
    passes::resolve_native_classes::resolve_native_classes(&doc.root_component);
    passes::ensure_window::ensure_window(&doc.root_component, &doc.local_registry);
    passes::collect_structs::collect_structs(&doc.root_component, diag);
    passes::generate_item_indices::generate_item_indices(&doc.root_component);
    passes::collect_custom_fonts::collect_custom_fonts(
        &doc.root_component,
        std::iter::once(&*doc).chain(type_loader.all_documents()),
        compiler_config.embed_resources,
    );
}

mod library {
    include!(env!("SIXTYFPS_WIDGETS_LIBRARY"));
}
