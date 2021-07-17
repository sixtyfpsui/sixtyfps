/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! This wasm library can be loaded from JS to load and display the content of .60 files
#![cfg(target_arch = "wasm32")]

use std::path::Path;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
#[allow(dead_code)]
pub struct CompilationResult {
    component: Option<WrappedCompiledComp>,
    diagnostics: js_sys::Array,
    error_string: String,
}

#[wasm_bindgen]
impl CompilationResult {
    #[wasm_bindgen(getter)]
    pub fn component(&self) -> Option<WrappedCompiledComp> {
        self.component.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn diagnostics(&self) -> js_sys::Array {
        self.diagnostics.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn error_string(&self) -> String {
        self.error_string.clone()
    }
}

/// Compile the content of a string.
///
/// Returns a promise to a compiled component which can be run with ".run()"
#[wasm_bindgen]
pub async fn compile_from_string(
    source: String,
    base_url: String,
    optional_import_callback: Option<js_sys::Function>,
) -> Result<CompilationResult, JsValue> {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    let mut compiler = sixtyfps_interpreter::ComponentCompiler::default();

    if let Some(load_callback) = optional_import_callback {
        let open_import_fallback = move |file_name: &Path| -> core::pin::Pin<
            Box<dyn core::future::Future<Output = Option<std::io::Result<String>>>>,
        > {
            Box::pin({
                let load_callback = load_callback.clone();
                let file_name: String = file_name.to_string_lossy().into();
                async move {
                    let result = load_callback.call1(&JsValue::UNDEFINED, &file_name.into());
                    let promise: js_sys::Promise = result.unwrap().into();
                    let future = wasm_bindgen_futures::JsFuture::from(promise);
                    match future.await {
                        Ok(js_ok) => Some(Ok(js_ok.as_string().unwrap_or_default())),
                        Err(js_err) => Some(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            js_err.as_string().unwrap_or_default(),
                        ))),
                    }
                }
            })
        };
        compiler.set_file_loader(open_import_fallback);
    }

    let c = compiler.build_from_source(source, base_url.into()).await;

    let line_key = JsValue::from_str("lineNumber");
    let column_key = JsValue::from_str("columnNumber");
    let message_key = JsValue::from_str("message");
    let file_key = JsValue::from_str("fileName");
    let level_key = JsValue::from_str("level");
    let mut error_as_string = String::new();
    let array = js_sys::Array::new();
    for d in compiler.diagnostics().into_iter() {
        let filename =
            d.source_file().as_ref().map_or(String::new(), |sf| sf.to_string_lossy().into());

        let filename_js = JsValue::from_str(&filename);
        let (line, column) = d.line_column();

        if d.level() == sixtyfps_interpreter::DiagnosticLevel::Error {
            if !error_as_string.is_empty() {
                error_as_string.push_str("\n");
            }
            use std::fmt::Write;

            write!(&mut error_as_string, "{}:{}:{}", filename, line, d).unwrap();
        }

        let error_obj = js_sys::Object::new();
        js_sys::Reflect::set(&error_obj, &message_key, &JsValue::from_str(&d.message()))?;
        js_sys::Reflect::set(&error_obj, &line_key, &JsValue::from_f64(line as f64))?;
        js_sys::Reflect::set(&error_obj, &column_key, &JsValue::from_f64(column as f64))?;
        js_sys::Reflect::set(&error_obj, &file_key, &filename_js)?;
        js_sys::Reflect::set(&error_obj, &level_key, &JsValue::from_f64(d.level() as i8 as f64))?;
        array.push(&error_obj);
    }

    Ok(CompilationResult {
        component: c.map(|c| WrappedCompiledComp(c)),
        diagnostics: array,
        error_string: error_as_string,
    })
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct WrappedCompiledComp(sixtyfps_interpreter::ComponentDefinition);

#[wasm_bindgen]
impl WrappedCompiledComp {
    /// Run this compiled component in a canvas.
    /// The HTML must contains a <canvas> element with the given `canvas_id`
    /// where the result is gonna be rendered
    #[wasm_bindgen]
    pub fn run(&self, canvas_id: String) {
        let component = self.0.create_with_canvas_id(&canvas_id);
        component.run();
    }
}
