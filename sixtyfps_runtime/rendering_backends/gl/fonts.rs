/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2020 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2020 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use super::CanvasRc;
use sixtyfps_corelib::graphics::FontRequest;
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::rc::Rc;

thread_local! {
    /// Database used to keep track of fonts added by the application
    static APPLICATION_FONTS: RefCell<fontdb::Database> = RefCell::new(fontdb::Database::new())
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WASM_FONT_REGISTERED: Cell<bool> = Cell::new(false)
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static MAPPED_FONTS: RefCell<HashMap<std::path::PathBuf, Rc<memmap2::Mmap>>> = RefCell::default()
}

pub enum AppOrSystemFont {
    AppFont(fontdb::ID), // TODO: add true type index
    #[cfg(not(target_arch = "wasm32"))]
    SystemFont(Rc<memmap2::Mmap>),
    Vec(Vec<u8>),
}

impl femtovg::FontData for AppOrSystemFont {
    fn with_data<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
        match self {
            AppOrSystemFont::AppFont(id) => APPLICATION_FONTS
                .with(|font_db| font_db.borrow().with_face_data(*id, |data, _| f(data)).unwrap()),
            #[cfg(not(target_arch = "wasm32"))]
            AppOrSystemFont::SystemFont(mmap) => f(mmap.as_ref()),
            AppOrSystemFont::Vec(vec) => f(&vec),
        }
    }

    fn from_slice(data: &[u8]) -> Self {
        Self::Vec(data.to_vec())
    }
}

/// This function can be used to register a custom TrueType font with SixtyFPS,
/// for use with the `font-family` property. The provided slice must be a valid TrueType
/// font.
pub fn register_font_from_memory(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    APPLICATION_FONTS.with(|fontdb| fontdb.borrow_mut().load_font_data(data.into()));
    Ok(())
}

pub fn register_font_from_path(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    // Should use FontDb::load_font_file but that requires the `fs` feature, for which I can't figure
    // out how to exclude it from the wasm build. It's inclusion implies mmap, which doesn't compile
    // with wasm.
    let data = std::fs::read(path)?;
    APPLICATION_FONTS.with(|fontdb| fontdb.borrow_mut().load_font_data(data));
    Ok(())
}

pub(crate) fn try_load_app_font(
    canvas: &CanvasRc,
    request: &FontRequest,
) -> Option<femtovg::FontId> {
    let family = request
        .family
        .as_ref()
        .map_or(fontdb::Family::SansSerif, |family| fontdb::Family::Name(&family));

    let query = fontdb::Query {
        families: &[family],
        weight: fontdb::Weight(request.weight.unwrap() as u16),
        ..Default::default()
    };
    APPLICATION_FONTS.with(|font_db| {
        let font_db = font_db.borrow();
        font_db.query(&query).and_then(|id| {
            // TODO index
            canvas.borrow_mut().add_font_object(AppOrSystemFont::AppFont(id)).ok()
        })
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_system_font(canvas: &CanvasRc, request: &FontRequest) -> femtovg::FontId {
    let family_name =
        request.family.as_ref().map_or(font_kit::family_name::FamilyName::SansSerif, |family| {
            font_kit::family_name::FamilyName::Title(family.to_string())
        });

    let handle = font_kit::source::SystemSource::new()
        .select_best_match(
            &[family_name, font_kit::family_name::FamilyName::SansSerif],
            &font_kit::properties::Properties::new()
                .weight(font_kit::properties::Weight(request.weight.unwrap() as f32)),
        )
        .unwrap();

    // pass index to femtovg once femtovg/femtovg/pull/21 is merged
    match handle {
        font_kit::handle::Handle::Path { path, font_index: _ } => {
            let mmapped_font_file = MAPPED_FONTS.with(|mapped_fonts| {
                mapped_fonts
                    .borrow_mut()
                    .entry(path)
                    .or_insert_with_key(|path| {
                        let r = Rc::new(unsafe {
                            memmap2::Mmap::map(&std::fs::File::open(path).unwrap()).unwrap()
                        });
                        r
                    })
                    .clone()
            });
            canvas
                .borrow_mut()
                .add_font_object(AppOrSystemFont::SystemFont(mmapped_font_file.clone()))
        }
        font_kit::handle::Handle::Memory { bytes, font_index: _ } => {
            canvas.borrow_mut().add_font_mem(bytes.as_slice())
        }
    }
    .unwrap()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_fallback_font(canvas: &CanvasRc, request: &FontRequest) -> femtovg::FontId {
    let handle = font_kit::source::SystemSource::new()
        .select_by_postscript_name(request.family.as_ref().unwrap())
        .unwrap();

    // pass index to femtovg once femtovg/femtovg/pull/21 is merged
    match handle {
        font_kit::handle::Handle::Path { path, font_index: _ } => {
            let mmapped_font_file = MAPPED_FONTS.with(|mapped_fonts| {
                mapped_fonts
                    .borrow_mut()
                    .entry(path)
                    .or_insert_with_key(|path| {
                        let r = Rc::new(unsafe {
                            memmap2::Mmap::map(&std::fs::File::open(path).unwrap()).unwrap()
                        });
                        r
                    })
                    .clone()
            });
            canvas
                .borrow_mut()
                .add_font_object(AppOrSystemFont::SystemFont(mmapped_font_file.clone()))
        }
        font_kit::handle::Handle::Memory { bytes, font_index: _ } => {
            canvas.borrow_mut().add_font_mem(bytes.as_slice())
        }
    }
    .unwrap()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_fallback_font(canvas: &CanvasRc, request: &FontRequest) -> femtovg::FontId {
    load_system_font(canvas, request)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_system_font(canvas: &CanvasRc, request: &FontRequest) -> femtovg::FontId {
    WASM_FONT_REGISTERED.with(|registered| {
        if !registered.get() {
            registered.set(true);
            register_font_from_memory(include_bytes!("fonts/DejaVuSans.ttf")).unwrap();
        }
    });
    let mut fallback_request = request.clone();
    fallback_request.family = Some("DejaVu Sans".into());
    try_load_app_font(canvas, &fallback_request).unwrap()
}

#[cfg(target_os = "macos")]
pub(crate) fn font_fallbacks_for_request(_request: &FontRequest) -> Vec<FontRequest> {
    _request
        .family
        .as_ref()
        .and_then(|family| {
            core_text::font::new_from_name(&family, _request.pixel_size.unwrap_or_default() as f64)
                .ok()
        })
        .map(|requested_font| {
            core_text::font::cascade_list_for_languages(
                &requested_font,
                &core_foundation::array::CFArray::from_CFTypes(&[]),
            )
            .iter()
            .map(|fallback_descriptor| FontRequest {
                family: Some(fallback_descriptor.family_name().into()),
                weight: _request.weight,
                pixel_size: _request.pixel_size,
                letter_spacing: _request.letter_spacing,
            })
            .filter(|fallback| !fallback.family.as_ref().unwrap().starts_with(".")) // font-kit asserts when loading `.Apple Fallback`
            .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn font_fallbacks_for_request(_request: &FontRequest) -> Vec<FontRequest> {
    vec![
        #[cfg(target_arch = "wasm32")]
        FontRequest {
            family: Some("DejaVu Sans".into()),
            weight: _request.weight,
            pixel_size: _request.pixel_size,
            letter_spacing: _request.letter_spacing,
        },
    ]
}
