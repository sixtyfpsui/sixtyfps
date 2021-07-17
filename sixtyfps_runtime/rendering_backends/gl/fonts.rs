/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use femtovg::TextContext;
#[cfg(target_os = "windows")]
use font_kit::loader::Loader;
use sixtyfps_corelib::graphics::{FontMetrics as FontMetricsTrait, FontRequest, Size};
use sixtyfps_corelib::{SharedString, SharedVector};
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::ItemGraphicsCache;

pub const DEFAULT_FONT_SIZE: f32 = 12.;
pub const DEFAULT_FONT_WEIGHT: i32 = 400; // CSS normal

thread_local! {
    /// Database used to keep track of fonts added by the application
    static APPLICATION_FONTS: RefCell<fontdb::Database> = RefCell::new(fontdb::Database::new())
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WASM_FONT_REGISTERED: Cell<bool> = Cell::new(false)
}

/// This function can be used to register a custom TrueType font with SixtyFPS,
/// for use with the `font-family` property. The provided slice must be a valid TrueType
/// font.
pub fn register_font_from_memory(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    APPLICATION_FONTS.with(|fontdb| fontdb.borrow_mut().load_font_data(data.into()));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn register_font_from_path(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let requested_path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    APPLICATION_FONTS.with(|fontdb| {
        for face_info in fontdb.borrow().faces() {
            match &*face_info.source {
                fontdb::Source::Binary(_) => {}
                fontdb::Source::File(loaded_path) => {
                    if *loaded_path == requested_path {
                        return Ok(());
                    }
                }
            }
        }

        fontdb.borrow_mut().load_font_file(requested_path).map_err(|e| e.into())
    })
}

#[cfg(target_arch = "wasm32")]
pub fn register_font_from_path(_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    return Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Registering fonts from paths is not supported in WASM builds",
    )
    .into());
}

pub(crate) fn try_load_app_font(
    text_context: &TextContext,
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
            font_db.with_face_data(id, |data, _index| {
                // pass index to femtovg once femtovg/femtovg/pull/21 is merged
                text_context.add_font_mem(&data).unwrap()
            })
        })
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_system_font(
    text_context: &TextContext,
    request: &FontRequest,
) -> femtovg::FontId {
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
        font_kit::handle::Handle::Path { path, font_index: _ } => text_context.add_font_file(path),
        font_kit::handle::Handle::Memory { bytes, font_index: _ } => {
            text_context.add_font_mem(bytes.as_slice())
        }
    }
    .unwrap()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_system_font(
    text_context: &TextContext,
    request: &FontRequest,
) -> femtovg::FontId {
    WASM_FONT_REGISTERED.with(|registered| {
        if !registered.get() {
            registered.set(true);
            register_font_from_memory(include_bytes!("fonts/DejaVuSans.ttf")).unwrap();
        }
    });
    let mut fallback_request = request.clone();
    fallback_request.family = Some("DejaVu Sans".into());
    try_load_app_font(text_context, &fallback_request).unwrap()
}

#[cfg(target_os = "macos")]
pub(crate) fn font_fallbacks_for_request(
    _request: &FontRequest,
    _reference_text: &str,
) -> Vec<FontRequest> {
    let requested_font = match core_text::font::new_from_name(
        &_request.family.as_ref().map_or_else(|| "", |s| s.as_str()),
        _request.pixel_size.unwrap_or_default() as f64,
    ) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

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
    .take(1) // Take only the top from the fallback list until we mmap the llaaarge font files
    .collect::<Vec<_>>()
}

#[cfg(target_os = "windows")]
pub(crate) fn font_fallbacks_for_request(
    _request: &FontRequest,
    _reference_text: &str,
) -> Vec<FontRequest> {
    let family_name =
        _request.family.as_ref().map_or(font_kit::family_name::FamilyName::SansSerif, |family| {
            font_kit::family_name::FamilyName::Title(family.to_string())
        });

    let handle = font_kit::source::SystemSource::new()
        .select_best_match(
            &[family_name, font_kit::family_name::FamilyName::SansSerif],
            &font_kit::properties::Properties::new()
                .weight(font_kit::properties::Weight(_request.weight.unwrap() as f32)),
        )
        .unwrap()
        .load()
        .unwrap();

    handle
        .get_fallbacks(_reference_text, "")
        .fonts
        .iter()
        .map(|fallback_font| FontRequest {
            family: Some(fallback_font.font.family_name().into()),
            weight: _request.weight,
            pixel_size: _request.pixel_size,
            letter_spacing: _request.letter_spacing,
        })
        .collect()
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub(crate) fn font_fallbacks_for_request(
    _request: &FontRequest,
    _reference_text: &str,
) -> Vec<FontRequest> {
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

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontCacheKey {
    family: SharedString,
    weight: i32,
}

#[derive(Clone)]
pub struct Font {
    fonts: SharedVector<femtovg::FontId>,
    pixel_size: f32,
    text_context: TextContext,
}

impl Font {
    pub fn measure(&self, letter_spacing: f32, text: &str) -> femtovg::TextMetrics {
        self.text_context
            .measure_text(0., 0., text, self.init_paint(letter_spacing, femtovg::Paint::default()))
            .unwrap()
    }

    pub fn height(&self) -> f32 {
        self.text_context
            .measure_font(self.init_paint(
                /*letter spacing does not influence height*/ 0.,
                femtovg::Paint::default(),
            ))
            .unwrap()
            .height()
    }

    pub fn init_paint(&self, letter_spacing: f32, mut paint: femtovg::Paint) -> femtovg::Paint {
        paint.set_font(&self.fonts);
        paint.set_font_size(self.pixel_size);
        paint.set_text_baseline(femtovg::Baseline::Top);
        paint.set_letter_spacing(letter_spacing);
        paint
    }

    pub fn text_size(&self, letter_spacing: f32, text: &str, max_width: Option<f32>) -> Size {
        let paint = self.init_paint(letter_spacing, femtovg::Paint::default());
        let font_metrics = self.text_context.measure_font(paint).unwrap();
        let mut lines = 0;
        let mut width = 0.;
        let mut start = 0;
        if let Some(max_width) = max_width {
            while start < text.len() {
                let index = self.text_context.break_text(max_width, &text[start..], paint).unwrap();
                if index == 0 {
                    break;
                }
                let index = start + index;
                let measure =
                    self.text_context.measure_text(0., 0., &text[start..index], paint).unwrap();
                start = index;
                lines += 1;
                width = measure.width().max(width);
            }
        } else {
            for line in text.lines() {
                let measure = self.text_context.measure_text(0., 0., line, paint).unwrap();
                lines += 1;
                width = measure.width().max(width);
            }
        }
        euclid::size2(width, lines as f32 * font_metrics.height())
    }
}

pub struct FontMetrics {
    font: Font,
    letter_spacing: Option<f32>,
    scale_factor: f32,
}

impl FontMetrics {
    pub(crate) fn new(
        graphics_cache: &mut ItemGraphicsCache,
        item_graphics_cache_data: &sixtyfps_corelib::item_rendering::CachedRenderingData,
        font_request_fn: impl Fn() -> sixtyfps_corelib::graphics::FontRequest,
        scale_factor: core::pin::Pin<&sixtyfps_corelib::Property<f32>>,
        reference_text: core::pin::Pin<&sixtyfps_corelib::Property<SharedString>>,
    ) -> Self {
        let font = graphics_cache
            .load_item_graphics_cache_with_function(item_graphics_cache_data, || {
                Some(super::ItemGraphicsCacheEntry::Font(FONT_CACHE.with(|cache| {
                    cache.borrow_mut().font(
                        font_request_fn(),
                        scale_factor.get(),
                        &reference_text.get(),
                    )
                })))
            })
            .unwrap()
            .as_font()
            .clone();
        Self {
            font,
            letter_spacing: font_request_fn().letter_spacing,
            scale_factor: scale_factor.get(),
        }
    }
}

impl FontMetricsTrait for FontMetrics {
    fn text_size(&self, text: &str, max_width: Option<f32>) -> Size {
        self.font.text_size(
            self.letter_spacing.unwrap_or_default(),
            text,
            max_width.map(|x| x * self.scale_factor),
        ) / self.scale_factor
    }

    fn line_height(&self) -> f32 {
        self.font.height()
    }

    fn text_offset_for_x_position<'a>(&self, text: &'a str, x: f32) -> usize {
        let x = x * self.scale_factor;
        let metrics = self.font.measure(self.letter_spacing.unwrap_or_default(), text);
        let mut current_x = 0.;
        for glyph in metrics.glyphs {
            if current_x + glyph.advance_x / 2. >= x {
                return glyph.byte_index;
            }
            current_x += glyph.advance_x;
        }
        return text.len();
    }
}

pub struct FontCache {
    fonts: HashMap<FontCacheKey, femtovg::FontId>,
    pub(crate) text_context: TextContext,
}

impl Default for FontCache {
    fn default() -> Self {
        Self { fonts: HashMap::new(), text_context: Default::default() }
    }
}

thread_local! {
    pub static FONT_CACHE: RefCell<FontCache> = RefCell::new(Default::default())
}

impl FontCache {
    fn load_single_font(&mut self, request: &FontRequest) -> femtovg::FontId {
        let text_context = self.text_context.clone();
        self.fonts
            .entry(FontCacheKey {
                family: request.family.clone().unwrap_or_default(),
                weight: request.weight.unwrap(),
            })
            .or_insert_with(|| {
                try_load_app_font(&text_context, &request)
                    .unwrap_or_else(|| load_system_font(&text_context, &request))
            })
            .clone()
    }

    pub fn font(
        &mut self,
        mut request: FontRequest,
        scale_factor: f32,
        reference_text: &str,
    ) -> Font {
        request.pixel_size = Some(request.pixel_size.unwrap_or(DEFAULT_FONT_SIZE) * scale_factor);
        request.weight = request.weight.or(Some(DEFAULT_FONT_WEIGHT));

        let primary_font = self.load_single_font(&request);
        let fallbacks = font_fallbacks_for_request(&request, reference_text);

        let fonts = core::iter::once(primary_font)
            .chain(
                fallbacks.iter().map(|fallback_request| self.load_single_font(&fallback_request)),
            )
            .collect::<SharedVector<_>>();

        Font {
            fonts,
            text_context: self.text_context.clone(),
            pixel_size: request.pixel_size.unwrap(),
        }
    }
}
