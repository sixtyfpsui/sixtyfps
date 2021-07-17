/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::diagnostics::{BuildDiagnostics, Spanned};
use crate::object_tree::{self, Document};
use crate::parser::{syntax_nodes, NodeOrToken, SyntaxKind, SyntaxToken};
use crate::typeregister::TypeRegister;
use crate::CompilerConfiguration;

/// Storage for a cache of all loaded documents
#[derive(Default)]
pub struct LoadedDocuments {
    /// maps from the canonical file name to the object_tree::Document
    docs: HashMap<PathBuf, Document>,
    currently_loading: HashSet<PathBuf>,
}

pub struct VirtualFile<'a> {
    pub path: &'a str,
    pub contents: &'a str,
}

pub type VirtualDirectory<'a> = [&'a VirtualFile<'a>];

pub struct ImportedTypes {
    pub import_token: SyntaxToken,
    pub imported_types: syntax_nodes::ImportSpecifier,
    pub file: String,
}

#[derive(Debug)]
pub struct ImportedName {
    // name of export to match in the other file
    pub external_name: String,
    // name to be used locally
    pub internal_name: String,
}

impl ImportedName {
    pub fn extract_imported_names(
        import: &syntax_nodes::ImportSpecifier,
    ) -> Option<impl Iterator<Item = ImportedName>> {
        import
            .ImportIdentifierList()
            .map(|import_identifiers| import_identifiers.ImportIdentifier().map(Self::from_node))
    }

    pub fn from_node(importident: syntax_nodes::ImportIdentifier) -> Self {
        let external_name = importident.ExternalName().text().to_string().trim().to_string();

        let internal_name = match importident.InternalName() {
            Some(name_ident) => name_ident.text().to_string().trim().to_string(),
            None => external_name.clone(),
        };

        ImportedName { internal_name, external_name }
    }
}

pub struct TypeLoader<'a> {
    pub global_type_registry: Rc<RefCell<TypeRegister>>,
    pub compiler_config: &'a CompilerConfiguration,
    pub builtin_library: Option<&'a VirtualDirectory<'a>>,
    all_documents: LoadedDocuments,
}

impl<'a> TypeLoader<'a> {
    pub fn new(
        global_type_registry: Rc<RefCell<TypeRegister>>,
        compiler_config: &'a CompilerConfiguration,
        diag: &mut BuildDiagnostics,
    ) -> Self {
        let style = compiler_config
        .style
        .as_ref()
        .map(Cow::from)
        .or_else(|| std::env::var("SIXTYFPS_STYLE").map(Cow::from).ok())
        .unwrap_or_else(|| {
            let is_wasm = cfg!(target_arch = "wasm32")
                || std::env::var("TARGET").map_or(false, |t| t.starts_with("wasm"));
            if !is_wasm {
                diag.push_diagnostic_with_span("SIXTYFPS_STYLE not defined, defaulting to 'ugly', see https://github.com/sixtyfpsui/sixtyfps/issues/83 for more info".to_owned(),
                    Default::default(),
                    crate::diagnostics::DiagnosticLevel::Warning
                );
            }
            Cow::from("ugly")
        });

        let builtin_library =
            crate::library::widget_library().iter().find(|x| x.0 == style).map(|x| x.1);

        Self {
            global_type_registry,
            compiler_config,
            builtin_library,
            all_documents: Default::default(),
        }
    }

    /// Imports of files that don't have the .60 extension are returned.
    pub async fn load_dependencies_recursively(
        &mut self,
        doc: &syntax_nodes::Document,
        diagnostics: &mut BuildDiagnostics,
        registry_to_populate: &Rc<RefCell<TypeRegister>>,
    ) -> Vec<ImportedTypes> {
        let dependencies = self.collect_dependencies(doc, diagnostics).await;
        let mut foreign_imports = vec![];
        for mut import in dependencies.into_iter() {
            if import.file.ends_with(".60") {
                if let Some(imported_types) =
                    ImportedName::extract_imported_names(&import.imported_types)
                {
                    self.load_dependency(import, imported_types, registry_to_populate, diagnostics)
                        .await;
                } else {
                    diagnostics.push_error(
                    "Import names are missing. Please specify which types you would like to import"
                        .into(),
                    &import.import_token,
                );
                }
            } else {
                import.file = self
                    .resolve_import_path(Some(&import.import_token.clone().into()), &import.file)
                    .0
                    .to_string_lossy()
                    .to_string();
                foreign_imports.push(import);
            }
        }
        foreign_imports
    }

    pub async fn import_type(
        &mut self,
        file_to_import: &str,
        type_name: &str,
        diagnostics: &mut BuildDiagnostics,
    ) -> Option<crate::langtype::Type> {
        let doc_path = match self.ensure_document_loaded(file_to_import, None, diagnostics).await {
            Some(doc_path) => doc_path,
            None => return None,
        };

        let doc = self.all_documents.docs.get(&doc_path).unwrap();

        doc.exports().iter().find_map(|(export_name, ty)| {
            if type_name == *export_name {
                Some(ty.clone())
            } else {
                None
            }
        })
    }

    /// Append a possibly relative path to a base path. Returns the data if it resolves to a built-in (compiled-in)
    /// file.
    pub fn resolve_import_path(
        &self,
        import_token: Option<&NodeOrToken>,
        maybe_relative_path_or_url: &str,
    ) -> (std::path::PathBuf, Option<&'_ str>) {
        let referencing_file_or_url =
            import_token.and_then(|tok| tok.source_file().map(|s| s.path()));

        self.find_file_in_include_path(referencing_file_or_url, maybe_relative_path_or_url)
            .map(|path| (path, None))
            .or_else(|| {
                self.builtin_library.and_then(|library| {
                    library
                        .iter()
                        .find(|virtual_file| virtual_file.path == maybe_relative_path_or_url)
                        .map(|virtual_file| {
                            (
                                format!("builtin:/{}", maybe_relative_path_or_url).into(),
                                Some(virtual_file.contents),
                            )
                        })
                })
            })
            .unwrap_or_else(|| {
                (
                    referencing_file_or_url
                        .and_then(|base_path_or_url| {
                            let base_path_or_url_str = base_path_or_url.to_string_lossy();
                            if base_path_or_url_str.contains("://") {
                                url::Url::parse(&base_path_or_url_str).ok().and_then(|base_url| {
                                    base_url
                                        .join(maybe_relative_path_or_url)
                                        .ok()
                                        .map(|url| url.to_string().into())
                                })
                            } else {
                                base_path_or_url.parent().and_then(|base_dir| {
                                    dunce::canonicalize(base_dir.join(maybe_relative_path_or_url))
                                        .ok()
                                })
                            }
                        })
                        .unwrap_or_else(|| maybe_relative_path_or_url.into()),
                    None,
                )
            })
    }

    async fn ensure_document_loaded<'b>(
        &'b mut self,
        file_to_import: &'b str,
        import_token: Option<NodeOrToken>,
        diagnostics: &'b mut BuildDiagnostics,
    ) -> Option<PathBuf> {
        let (path, is_builtin) = self.resolve_import_path(import_token.as_ref(), file_to_import);

        let path_canon = dunce::canonicalize(&path).unwrap_or_else(|_| path.to_owned());

        if self.all_documents.docs.get(path_canon.as_path()).is_some() {
            return Some(path_canon);
        }

        // Drop &self lifetime attached to is_builtin, in order to mutable borrow self below
        let is_builtin = is_builtin.map(|s| s.to_owned());

        if !self.all_documents.currently_loading.insert(path_canon.clone()) {
            diagnostics
                .push_error(format!("Recursive import of {}", path.display()), &import_token);
            return None;
        }

        let source_code_result = if let Some(builtin) = is_builtin {
            Ok(builtin)
        } else if let Some(fallback) = &self.compiler_config.open_import_fallback {
            let result = fallback(path_canon.to_string_lossy().into()).await;
            result.unwrap_or_else(|| std::fs::read_to_string(&path_canon))
        } else {
            std::fs::read_to_string(&path_canon)
        };

        let source_code = match source_code_result {
            Ok(source) => source,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                diagnostics.push_error(
                    format!(
                        "Cannot find requested import {} in the include search path",
                        file_to_import
                    ),
                    &import_token,
                );
                return None;
            }
            Err(err) => {
                diagnostics.push_error(
                    format!("Error reading requested import {}: {}", path.display(), err),
                    &import_token,
                );
                return None;
            }
        };

        self.load_file(&path_canon, &path, source_code, diagnostics).await;
        let _ok = self.all_documents.currently_loading.remove(path_canon.as_path());
        assert!(_ok);
        Some(path_canon)
    }

    /// Load a file, and its dependency not run the passes.
    ///
    /// the path must be the canonical path
    pub async fn load_file(
        &mut self,
        path: &Path,
        source_path: &Path,
        source_code: String,
        diagnostics: &mut BuildDiagnostics,
    ) {
        let dependency_doc: syntax_nodes::Document =
            crate::parser::parse(source_code, Some(source_path), diagnostics).into();

        let dependency_registry =
            Rc::new(RefCell::new(TypeRegister::new(&self.global_type_registry)));
        let foreign_imports = self
            .load_dependencies_recursively(&dependency_doc, diagnostics, &dependency_registry)
            .await;

        if diagnostics.has_error() {
            // If there was error (esp parse error) we don't want to report further error in this document.
            // because they might be nonsense (TODO: we should check that the parse error were really in this document).
            // But we still want to create a document to give better error messages in the root document.
            let mut ignore_diag = BuildDiagnostics::default();
            ignore_diag.push_error_with_span(
                "Dummy error because some of the code asserts there was an error".into(),
                Default::default(),
            );
            let doc = crate::object_tree::Document::from_node(
                dependency_doc,
                foreign_imports,
                &mut ignore_diag,
                &dependency_registry,
            );
            self.all_documents.docs.insert(path.to_owned(), doc);
            return;
        }
        let doc = crate::object_tree::Document::from_node(
            dependency_doc,
            foreign_imports,
            diagnostics,
            &dependency_registry,
        );
        crate::passes::run_import_passes(&doc, self, diagnostics);

        self.all_documents.docs.insert(path.to_owned(), doc);
    }

    fn load_dependency<'b>(
        &'b mut self,
        import: ImportedTypes,
        imported_types: impl Iterator<Item = ImportedName> + 'b,
        registry_to_populate: &'b Rc<RefCell<TypeRegister>>,
        build_diagnostics: &'b mut BuildDiagnostics,
    ) -> core::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'b>> {
        Box::pin(async move {
            let doc_path = match self
                .ensure_document_loaded(
                    &import.file,
                    Some(import.import_token.clone().into()),
                    build_diagnostics,
                )
                .await
            {
                Some(path) => path,
                None => return,
            };

            let doc = self.all_documents.docs.get(&doc_path).unwrap();
            let exports = doc.exports();

            for import_name in imported_types {
                let imported_type = exports.iter().find_map(|(export_name, ty)| {
                    if import_name.external_name == *export_name {
                        Some(ty.clone())
                    } else {
                        None
                    }
                });

                let imported_type = match imported_type {
                    Some(ty) => ty,
                    None => {
                        build_diagnostics.push_error(
                            format!(
                                "No exported type called {} found in {}",
                                import_name.external_name, import.file
                            ),
                            &import.import_token,
                        );
                        continue;
                    }
                };

                registry_to_populate
                    .borrow_mut()
                    .insert_type_with_name(imported_type, import_name.internal_name);
            }
        })
    }

    /// Lookup a filename and try to find the absolute filename based on the include path or
    /// the current file directory
    pub(crate) fn find_file_in_include_path(
        &self,
        referencing_file: Option<&std::path::Path>,
        file_to_import: &str,
    ) -> Option<PathBuf> {
        // The directory of the current file is the first in the list of include directories.
        let maybe_current_directory =
            referencing_file.and_then(|path| path.parent()).map(|p| p.to_path_buf());
        maybe_current_directory
            .clone()
            .into_iter()
            .chain(self.compiler_config.include_paths.iter().map(PathBuf::as_path).map({
                |include_path| {
                    if include_path.is_relative() && maybe_current_directory.as_ref().is_some() {
                        maybe_current_directory.as_ref().unwrap().join(include_path)
                    } else {
                        include_path.to_path_buf()
                    }
                }
            }))
            .find_map(|include_dir| {
                let candidate = include_dir.join(file_to_import);
                candidate.exists().then(|| candidate)
            })
    }

    async fn collect_dependencies(
        &mut self,
        doc: &syntax_nodes::Document,
        doc_diagnostics: &mut BuildDiagnostics,
    ) -> impl Iterator<Item = ImportedTypes> {
        type DependenciesByFile = BTreeMap<String, ImportedTypes>;
        let mut dependencies = DependenciesByFile::new();

        for import in doc.ImportSpecifier() {
            let import_uri = match import.child_token(SyntaxKind::StringLiteral) {
                Some(import_uri) => import_uri,
                None => {
                    debug_assert!(doc_diagnostics.has_error());
                    continue;
                }
            };
            let path_to_import = import_uri.text().to_string();
            let path_to_import = path_to_import.trim_matches('\"').to_string();
            if path_to_import.is_empty() {
                doc_diagnostics.push_error("Unexpected empty import url".to_owned(), &import_uri);
                continue;
            }

            dependencies.entry(path_to_import.clone()).or_insert_with(|| ImportedTypes {
                import_token: import_uri,
                imported_types: import,
                file: path_to_import,
            });
        }

        dependencies.into_iter().map(|(_, value)| value)
    }

    /// Return a document if it was already loaded
    pub fn get_document<'b>(&'b self, path: &Path) -> Option<&'b object_tree::Document> {
        dunce::canonicalize(path).map_or_else(
            |_| self.all_documents.docs.get(path),
            |path| self.all_documents.docs.get(&path),
        )
    }

    /// Return an iterator over all the loaded file path
    pub fn all_files<'b>(&'b self) -> impl Iterator<Item = &PathBuf> + 'b {
        self.all_documents.docs.keys()
    }

    /// Returns an iterator over all the loaded documents
    pub fn all_documents(&self) -> impl Iterator<Item = &object_tree::Document> + '_ {
        self.all_documents.docs.values()
    }
}

#[test]
fn test_dependency_loading() {
    let test_source_path: std::path::PathBuf =
        [env!("CARGO_MANIFEST_DIR"), "tests", "typeloader"].iter().collect();

    let mut incdir = test_source_path.clone();
    incdir.push("incpath");

    let mut compiler_config =
        CompilerConfiguration::new(crate::generator::OutputFormat::Interpreter);
    compiler_config.include_paths = vec![incdir];
    compiler_config.style = Some("ugly".into());

    let mut main_test_path = test_source_path.clone();
    main_test_path.push("dependency_test_main.60");

    let mut test_diags = crate::diagnostics::BuildDiagnostics::default();
    let doc_node = crate::parser::parse_file(main_test_path.clone(), &mut test_diags).unwrap();

    let doc_node: syntax_nodes::Document = doc_node.into();

    let global_registry = TypeRegister::builtin();

    let registry = Rc::new(RefCell::new(TypeRegister::new(&global_registry)));

    let mut build_diagnostics = BuildDiagnostics::default();

    let mut loader = TypeLoader::new(global_registry, &compiler_config, &mut build_diagnostics);

    spin_on::spin_on(loader.load_dependencies_recursively(
        &doc_node,
        &mut build_diagnostics,
        &registry,
    ));

    assert!(!test_diags.has_error());
    assert!(!build_diagnostics.has_error());
}

#[test]
fn test_load_from_callback_ok() {
    let ok = Rc::new(core::cell::Cell::new(false));
    let ok_ = ok.clone();

    let mut compiler_config =
        CompilerConfiguration::new(crate::generator::OutputFormat::Interpreter);
    compiler_config.style = Some("ugly".into());
    compiler_config.open_import_fallback = Some(Rc::new(move |path| {
        let ok_ = ok_.clone();
        Box::pin(async move {
            assert_eq!(path, "../FooBar.60");
            assert_eq!(ok_.get(), false);
            ok_.set(true);
            Some(Ok("export XX := Rectangle {} ".to_owned()))
        })
    }));

    let mut test_diags = crate::diagnostics::BuildDiagnostics::default();
    let doc_node = crate::parser::parse(
        r#"
/* ... */
import { XX } from "../FooBar.60";
X := XX {}
"#
        .into(),
        Some(&std::path::Path::new("HELLO")),
        &mut test_diags,
    );

    let doc_node: syntax_nodes::Document = doc_node.into();
    let global_registry = TypeRegister::builtin();
    let registry = Rc::new(RefCell::new(TypeRegister::new(&global_registry)));
    let mut build_diagnostics = BuildDiagnostics::default();
    let mut loader = TypeLoader::new(global_registry, &compiler_config, &mut build_diagnostics);
    spin_on::spin_on(loader.load_dependencies_recursively(
        &doc_node,
        &mut build_diagnostics,
        &registry,
    ));
    assert_eq!(ok.get(), true);
    assert!(!test_diags.has_error());
    assert!(!build_diagnostics.has_error());
}

#[test]
fn test_manual_import() {
    let mut compiler_config =
        CompilerConfiguration::new(crate::generator::OutputFormat::Interpreter);
    compiler_config.style = Some("ugly".into());
    let global_registry = TypeRegister::builtin();
    let mut build_diagnostics = BuildDiagnostics::default();
    let mut loader = TypeLoader::new(global_registry, &compiler_config, &mut build_diagnostics);

    let maybe_button_type = spin_on::spin_on(loader.import_type(
        "sixtyfps_widgets.60",
        "Button",
        &mut build_diagnostics,
    ));

    assert!(!build_diagnostics.has_error());
    assert!(maybe_button_type.is_some());
}
