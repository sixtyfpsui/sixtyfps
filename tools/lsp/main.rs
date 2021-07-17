/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

mod completion;
mod goto;
mod lsp_ext;
mod preview;
mod util;

use std::collections::HashMap;
use structopt::StructOpt;

use lsp_server::{Connection, Message, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification};
use lsp_types::request::{
    CodeActionRequest, DocumentColor, DocumentSymbolRequest, ExecuteCommand, GotoDefinition,
};
use lsp_types::request::{ColorPresentationRequest, Completion, HoverRequest};
use lsp_types::{
    CodeActionOrCommand, CodeActionProviderCapability, Color, ColorInformation, ColorPresentation,
    Command, CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolResponse, ExecuteCommandOptions, Hover, InitializeParams, Location, OneOf,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, SymbolInformation,
    TextDocumentSyncCapability, Url, WorkDoneProgressOptions,
};
use sixtyfps_compilerlib::diagnostics::BuildDiagnostics;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::parser::{syntax_nodes, SyntaxKind, SyntaxNode, SyntaxToken};
use sixtyfps_compilerlib::typeloader::TypeLoader;
use sixtyfps_compilerlib::typeregister::TypeRegister;
use sixtyfps_compilerlib::CompilerConfiguration;

type Error = Box<dyn std::error::Error>;

const SHOW_PREVIEW_COMMAND: &str = "showPreview";

#[derive(StructOpt, Clone)]
struct Cli {
    #[structopt(
        short = "I",
        name = "Add include paths for the import statements",
        number_of_values = 1
    )]
    include_paths: Vec<std::path::PathBuf>,

    /// The style name for the preview ('native' or 'ugly')
    #[structopt(long, name = "style name", default_value)]
    style: String,

    /// The backend used for the preview ('GL' or 'Qt')
    #[structopt(long, name = "backend", default_value)]
    backend: String,
}

pub struct DocumentCache<'a> {
    documents: TypeLoader<'a>,
    newline_offsets: HashMap<Url, Vec<u32>>,
}

impl<'a> DocumentCache<'a> {
    fn new(config: &'a CompilerConfiguration) -> Self {
        let documents =
            TypeLoader::new(TypeRegister::builtin(), config, &mut BuildDiagnostics::default());
        Self { documents, newline_offsets: Default::default() }
    }

    fn newline_offsets_from_content(content: &str) -> Vec<u32> {
        let mut ln_offs = 0;
        content
            .split('\n')
            .map(|line| {
                let r = ln_offs;
                ln_offs += line.len() as u32 + 1;
                r
            })
            .collect()
    }

    pub fn byte_offset_to_position(
        &mut self,
        offset: u32,
        target_uri: &lsp_types::Url,
    ) -> Option<lsp_types::Position> {
        let newline_offsets = match self.newline_offsets.entry(target_uri.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(Self::newline_offsets_from_content(
                    &std::fs::read_to_string(target_uri.to_file_path().ok()?).ok()?,
                ))
            }
        };
        let pos = newline_offsets.binary_search(&offset).map_or_else(
            |line| {
                if line == 0 {
                    Position::new(0, offset)
                } else {
                    Position::new(
                        line as u32 - 1,
                        newline_offsets.get(line - 1).map_or(0, |x| offset - *x),
                    )
                }
            },
            |line| Position::new(line as u32, 0),
        );
        Some(pos)
    }
}

fn main() {
    let args: Cli = Cli::from_args();
    if !args.backend.is_empty() {
        std::env::set_var("SIXTYFPS_BACKEND", &args.backend);
    }

    let lsp_thread = std::thread::spawn(|| {
        /// Make sure we quit the event loop even if we panic
        struct QuitEventLoop;
        impl Drop for QuitEventLoop {
            fn drop(&mut self) {
                preview::quit_ui_event_loop();
            }
        }
        let _quit_ui_loop = QuitEventLoop;

        match run_lsp_server() {
            Ok(_) => {}
            Err(error) => {
                eprintln!("Error running LSP server: {}", error);
            }
        }
    });

    preview::start_ui_event_loop();
    lsp_thread.join().unwrap();
}

fn run_lsp_server() -> Result<(), Error> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            resolve_provider: None,
            trigger_characters: Some(vec![".".to_owned()]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        definition_provider: Some(OneOf::Left(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            lsp_types::TextDocumentSyncKind::Full,
        )),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![SHOW_PREVIEW_COMMAND.into()],
            ..Default::default()
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        color_provider: Some(true.into()),
        ..ServerCapabilities::default()
    };
    let server_capabilities = serde_json::to_value(&capabilities).unwrap();
    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(&connection, initialization_params)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: &Connection, params: serde_json::Value) -> Result<(), Error> {
    let cli_args: Cli = Cli::from_args();
    let params: InitializeParams = serde_json::from_value(params).unwrap();
    let mut compiler_config =
        CompilerConfiguration::new(sixtyfps_compilerlib::generator::OutputFormat::Interpreter);
    compiler_config.style =
        Some(if cli_args.style.is_empty() { "ugly".into() } else { cli_args.style });
    compiler_config.include_paths = cli_args.include_paths;

    let mut document_cache = DocumentCache::new(&compiler_config);
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(connection, req, &params, &mut document_cache)?;
            }
            Message::Response(_resp) => {}
            Message::Notification(notifi) => {
                handle_notification(connection, notifi, &mut document_cache)?
            }
        }
    }
    Ok(())
}

fn handle_request(
    connection: &Connection,
    req: Request,
    init_param: &InitializeParams,
    document_cache: &mut DocumentCache,
) -> Result<(), Error> {
    let mut req = Some(req);
    if let Some((id, params)) = cast::<GotoDefinition>(&mut req) {
        let result = token_descr(
            document_cache,
            params.text_document_position_params.text_document,
            params.text_document_position_params.position,
        )
        .and_then(|token| {
            if token.0.kind() == SyntaxKind::Comment {
                maybe_goto_preview(token.0, token.1, connection.sender.clone());
                return None;
            }
            goto::goto_definition(document_cache, token.0)
        });
        let resp = Response::new_ok(id, result);
        connection.sender.send(Message::Response(resp))?;
    } else if let Some((id, params)) = cast::<Completion>(&mut req) {
        let result = token_descr(
            document_cache,
            params.text_document_position.text_document,
            params.text_document_position.position,
        )
        .and_then(|token| {
            completion::completion_at(
                document_cache,
                token.0,
                token.1,
                init_param.capabilities.text_document.as_ref().and_then(|t| t.completion.as_ref()),
            )
        });
        let resp = Response::new_ok(id, result);
        connection.sender.send(Message::Response(resp))?;
    } else if let Some((id, _params)) = cast::<HoverRequest>(&mut req) {
        /*let result =
            token_descr(document_cache, params.text_document_position_params).map(|x| Hover {
                contents: lsp_types::HoverContents::Scalar(MarkedString::from_language_code(
                    "text".into(),
                    format!("{:?}", x.token),
                )),
                range: None,
            });
        let resp = Response::new_ok(id, result);
        connection.sender.send(Message::Response(resp))?;*/
        connection.sender.send(Message::Response(Response::new_ok(id, None::<Hover>)))?;
    } else if let Some((id, params)) = cast::<CodeActionRequest>(&mut req) {
        let result = token_descr(document_cache, params.text_document, params.range.start)
            .and_then(|token| get_code_actions(document_cache, token.0.parent()));
        connection.sender.send(Message::Response(Response::new_ok(id, result)))?;
    } else if let Some((id, params)) = cast::<ExecuteCommand>(&mut req) {
        match params.command.as_str() {
            SHOW_PREVIEW_COMMAND => {
                show_preview_command(&params.arguments, connection, document_cache)?
            }
            _ => (),
        }
        connection
            .sender
            .send(Message::Response(Response::new_ok(id, None::<serde_json::Value>)))?;
    } else if let Some((id, params)) = cast::<DocumentColor>(&mut req) {
        let result = get_document_color(document_cache, &params.text_document).unwrap_or_default();
        connection.sender.send(Message::Response(Response::new_ok(id, result)))?;
    } else if let Some((id, params)) = cast::<ColorPresentationRequest>(&mut req) {
        // Convert the color from the color picker to a string representation. This could try to produce a minimal
        // representation.
        let requested_color = params.color;

        let color_literal = if (requested_color.alpha - 1.).abs() > 10.0 * f32::EPSILON {
            format!(
                "#{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
                (requested_color.red * 255.) as u8,
                (requested_color.green * 255.) as u8,
                (requested_color.blue * 255.) as u8,
                (requested_color.alpha * 255.) as u8
            )
        } else {
            format!(
                "#{:0>2x}{:0>2x}{:0>2x}",
                (requested_color.red * 255.) as u8,
                (requested_color.green * 255.) as u8,
                (requested_color.blue * 255.) as u8,
            )
        };

        let result = vec![ColorPresentation { label: color_literal, ..Default::default() }];
        connection.sender.send(Message::Response(Response::new_ok(id, result)))?;
    } else if let Some((id, params)) = cast::<DocumentSymbolRequest>(&mut req) {
        let result = get_document_symbols(document_cache, &params.text_document);
        connection.sender.send(Message::Response(Response::new_ok(id, result)))?;
    };
    Ok(())
}

fn cast<Kind: lsp_types::request::Request>(
    req: &mut Option<Request>,
) -> Option<(RequestId, Kind::Params)> {
    match req.take().unwrap().extract::<Kind::Params>(Kind::METHOD) {
        Ok(value) => Some(value),
        Err(owned) => {
            *req = Some(owned);
            None
        }
    }
}

fn handle_notification(
    connection: &Connection,
    req: lsp_server::Notification,
    document_cache: &mut DocumentCache,
) -> Result<(), Error> {
    match &*req.method {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(req.params)?;
            reload_document(
                connection,
                params.text_document.text,
                params.text_document.uri,
                document_cache,
            )?;
        }
        DidChangeTextDocument::METHOD => {
            let mut params: DidChangeTextDocumentParams = serde_json::from_value(req.params)?;
            reload_document(
                connection,
                params.content_changes.pop().unwrap().text,
                params.text_document.uri,
                document_cache,
            )?;
        }
        "sixtyfps/showPreview" => {
            show_preview_command(
                req.params.as_array().map_or(&[], |x| x.as_slice()),
                connection,
                document_cache,
            )?;
        }
        _ => (),
    }
    Ok(())
}

fn show_preview_command(
    params: &[serde_json::Value],
    connection: &Connection,
    _document_cache: &DocumentCache,
) -> Result<(), Error> {
    let e = || -> Error { "InvalidParameter".into() };
    let path = if let serde_json::Value::String(s) = params.get(0).ok_or_else(e)? {
        std::path::PathBuf::from(s)
    } else {
        return Err(e());
    };
    let path_canon = dunce::canonicalize(&path).unwrap_or_else(|_| path.to_owned());
    let component = params.get(1).and_then(|v| v.as_str()).map(|v| v.to_string());
    preview::load_preview(
        connection.sender.clone(),
        preview::PreviewComponent { path: path_canon.into(), component },
        preview::PostLoadBehavior::ShowAfterLoad,
    );
    Ok(())
}

/// Workaround for editor that do not support code action: using the goto definition on a comment
/// that says "preview" will show the preview.
fn maybe_goto_preview(
    token: SyntaxToken,
    offset: u32,
    sender: crossbeam_channel::Sender<Message>,
) -> Option<()> {
    let text = token.text();
    let offset = offset.checked_sub(token.text_range().start().into())? as usize;
    if offset > text.len() || offset == 0 {
        return None;
    }
    let begin = text[..offset].rfind(|x: char| !x.is_ascii_alphanumeric())? + 1;
    let text = &text.as_bytes()[begin..];
    let rest = text.strip_prefix(b"preview").or_else(|| text.strip_prefix(b"PREVIEW"))?;
    if rest.get(0).map_or(true, |x| x.is_ascii_alphanumeric()) {
        return None;
    }

    // Ok, we were hovering on PREVIEW
    let mut node = token.parent();
    loop {
        if let Some(component) = syntax_nodes::Component::new(node.clone()) {
            let component_name =
                sixtyfps_compilerlib::parser::identifier_text(&component.DeclaredIdentifier())?;
            preview::load_preview(
                sender,
                preview::PreviewComponent {
                    path: token.source_file.path().into(),
                    component: Some(component_name),
                },
                preview::PostLoadBehavior::ShowAfterLoad,
            );
            return Some(());
        }
        node = node.parent()?;
    }
}

fn reload_document(
    connection: &Connection,
    content: String,
    uri: lsp_types::Url,
    document_cache: &mut DocumentCache,
) -> Result<(), Error> {
    let newline_offsets = DocumentCache::newline_offsets_from_content(&content);
    document_cache.newline_offsets.insert(uri.clone(), newline_offsets);

    let path = uri.to_file_path().unwrap();
    let path_canon = dunce::canonicalize(&path).unwrap_or_else(|_| path.to_owned());
    preview::set_contents(&path_canon, content.clone());
    let mut diag = BuildDiagnostics::default();
    spin_on::spin_on(document_cache.documents.load_file(&path_canon, &path, content, &mut diag));

    // Always provide diagnostics for all files. Empty diagnostics clear any previous ones.
    let mut lsp_diags: HashMap<Url, Vec<lsp_types::Diagnostic>> = core::iter::once(&path)
        .chain(diag.all_loaded_files.iter())
        .map(|path| {
            let uri = Url::from_file_path(path).unwrap();
            (uri, Default::default())
        })
        .collect();

    for d in diag.into_iter() {
        if d.source_file().unwrap().is_relative() {
            continue;
        }
        let uri = Url::from_file_path(d.source_file().unwrap()).unwrap();
        lsp_diags.entry(uri).or_default().push(util::to_lsp_diag(&d));
    }

    for (uri, diagnostics) in lsp_diags {
        connection.sender.send(Message::Notification(lsp_server::Notification::new(
            "textDocument/publishDiagnostics".into(),
            PublishDiagnosticsParams { uri, diagnostics, version: None },
        )))?;
    }

    Ok(())
}

/// return the token, and the offset within the file
fn token_descr(
    document_cache: &mut DocumentCache,
    text_document: lsp_types::TextDocumentIdentifier,
    pos: Position,
) -> Option<(SyntaxToken, u32)> {
    let o = document_cache.newline_offsets.get(&text_document.uri)?.get(pos.line as usize)?
        + pos.character as u32;

    let doc = document_cache.documents.get_document(&text_document.uri.to_file_path().ok()?)?;
    let node = doc.node.as_ref()?;
    if !node.text_range().contains(o.into()) {
        return None;
    }
    let mut taf = node.token_at_offset(o.into());
    let token = match (taf.next(), taf.next()) {
        (None, _) => return None,
        (Some(t), None) => t,
        (Some(l), Some(r)) => match (l.kind(), r.kind()) {
            // Prioritize identifier
            (SyntaxKind::Identifier, _) => l,
            (_, SyntaxKind::Identifier) => r,
            // then the dot
            (SyntaxKind::Dot, _) => l,
            (_, SyntaxKind::Dot) => r,
            // de-prioritize the white spaces
            (SyntaxKind::Whitespace, _) => r,
            (SyntaxKind::Comment, _) => r,
            (_, SyntaxKind::Whitespace) => l,
            (_, SyntaxKind::Comment) => l,
            _ => l,
        },
    };
    Some((SyntaxToken { token, source_file: node.source_file.clone() }, o))
}

fn get_code_actions(
    _document_cache: &mut DocumentCache,
    node: SyntaxNode,
) -> Option<Vec<CodeActionOrCommand>> {
    let component = syntax_nodes::Component::new(node.clone())
        .or_else(|| {
            syntax_nodes::DeclaredIdentifier::new(node.clone())
                .and_then(|n| n.parent())
                .and_then(|p| syntax_nodes::Component::new(p))
        })
        .or_else(|| {
            syntax_nodes::QualifiedName::new(node.clone())
                .and_then(|n| n.parent())
                .and_then(|p| syntax_nodes::Element::new(p))
                .and_then(|n| n.parent())
                .and_then(|p| syntax_nodes::Component::new(p))
        })?;

    let component_name =
        sixtyfps_compilerlib::parser::identifier_text(&component.DeclaredIdentifier())?;

    Some(vec![CodeActionOrCommand::Command(Command::new(
        "Show preview".into(),
        SHOW_PREVIEW_COMMAND.into(),
        Some(vec![component.source_file.path().to_string_lossy().into(), component_name.into()]),
    ))])
}

fn get_document_color(
    document_cache: &mut DocumentCache,
    text_document: &lsp_types::TextDocumentIdentifier,
) -> Option<Vec<ColorInformation>> {
    let mut result = Vec::new();
    let uri = &text_document.uri;
    let doc = document_cache.documents.get_document(&uri.to_file_path().ok()?)?;
    let root_node = &doc.node.as_ref()?.node;
    let mut token = root_node.first_token()?;
    loop {
        if token.kind() == SyntaxKind::ColorLiteral {
            (|| -> Option<()> {
                let range = token.text_range();
                let col = sixtyfps_compilerlib::literals::parse_color_literal(token.text())?;
                let shift = |s: u32| -> f32 { ((col >> s) & 0xff) as f32 / 255. };
                result.push(ColorInformation {
                    range: Range::new(
                        document_cache.byte_offset_to_position(range.start().into(), &uri)?,
                        document_cache.byte_offset_to_position(range.end().into(), &uri)?,
                    ),
                    color: Color {
                        alpha: shift(24),
                        red: shift(16),
                        green: shift(8),
                        blue: shift(0),
                    },
                });
                Some(())
            })();
        }
        token = match token.next_token() {
            Some(token) => token,
            None => break Some(result),
        }
    }
}

fn get_document_symbols(
    document_cache: &mut DocumentCache,
    text_document: &lsp_types::TextDocumentIdentifier,
) -> Option<DocumentSymbolResponse> {
    let uri = &text_document.uri;
    let doc = document_cache.documents.get_document(&uri.to_file_path().ok()?)?;

    // SymbolInformation doesn't implement default and some field depends on features or are deprecated
    let si: SymbolInformation = serde_json::from_value(
        serde_json::json!({ "name" : "", "kind": 255, "location" : Location::new(uri.clone(), Range::default()) })
    )
    .unwrap();

    let inner_components = doc.inner_components.clone();
    let inner_structs = doc.inner_structs.clone();
    let mut make_range = |node: &SyntaxNode| {
        let r = node.text_range();
        Some(Range::new(
            document_cache.byte_offset_to_position(r.start().into(), &uri)?,
            document_cache.byte_offset_to_position(r.end().into(), &uri)?,
        ))
    };

    let mut r = inner_components
        .iter()
        .filter_map(|c| {
            Some(SymbolInformation {
                location: Location::new(
                    uri.clone(),
                    make_range(c.root_element.borrow().node.as_ref()?)?,
                ),
                name: c.id.clone(),
                kind: lsp_types::SymbolKind::Object,
                ..si.clone()
            })
        })
        .collect::<Vec<_>>();

    r.extend(inner_structs.iter().filter_map(|c| match c {
        Type::Struct { name: Some(name), node: Some(node), .. } => Some(SymbolInformation {
            location: Location::new(uri.clone(), make_range(node.parent().as_ref()?)?),
            name: name.clone(),
            kind: lsp_types::SymbolKind::Struct,
            ..si.clone()
        }),
        _ => None,
    }));

    Some(r.into())

    // TODO: add the structs
}
