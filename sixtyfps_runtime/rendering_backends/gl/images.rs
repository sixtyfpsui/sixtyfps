/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use sixtyfps_corelib::Property;
use sixtyfps_corelib::{graphics::Size, slice::Slice, ImageInner, SharedString};

use super::{CanvasRc, GLItemRenderer};

struct Texture {
    id: femtovg::ImageId,
    canvas: CanvasRc,
}

impl Texture {
    fn size(&self) -> Option<Size> {
        self.canvas
            .borrow()
            .image_info(self.id)
            .map(|info| [info.width() as f32, info.height() as f32].into())
            .ok()
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        self.canvas.borrow_mut().delete_image(self.id);
    }
}

#[cfg(target_arch = "wasm32")]
struct HTMLImage {
    dom_element: web_sys::HtmlImageElement,
    /// If present, this boolean property indicates whether the image has been uploaded yet or
    /// if that operation is still pending. If not present, then the image *is* available. This is
    /// used for remote HTML image loading and the property will be used to correctly track dependencies
    /// to graphics items that query for the size.
    image_load_pending: core::pin::Pin<Rc<Property<bool>>>,
}

#[cfg(target_arch = "wasm32")]
impl HTMLImage {
    fn new(url: &str) -> Self {
        let dom_element = web_sys::HtmlImageElement::new().unwrap();

        let image_load_pending = Rc::pin(Property::new(true));

        let event_loop_proxy = crate::eventloop::with_window_target(|event_loop| {
            event_loop.event_loop_proxy().clone()
        });

        dom_element.set_cross_origin(Some("anonymous"));
        dom_element.set_onload(Some(
            &wasm_bindgen::closure::Closure::once_into_js({
                let image_load_pending = image_load_pending.clone();
                move || {
                    image_load_pending.as_ref().set(false);

                    // As you can paint on a HTML canvas at any point in time, request_redraw()
                    // on a winit window only queues an additional internal event, that'll be
                    // be dispatched as the next event. We are however not in an event loop
                    // call, so we also need to wake up the event loop and redraw then.
                    event_loop_proxy
                        .send_event(crate::eventloop::CustomEvent::RedrawAllWindows)
                        .ok();
                }
            })
            .into(),
        ));
        dom_element.set_src(&url);

        Self { dom_element, image_load_pending }
    }

    fn size(&self) -> Option<Size> {
        match self.image_load_pending.as_ref().get() {
            true => None,
            false => Some(Size::new(self.dom_element.width() as _, self.dom_element.height() as _)),
        }
    }
}

#[derive(derive_more::From)]
enum ImageData {
    Texture(Texture),
    DecodedImage(image::DynamicImage),
    #[cfg(feature = "svg")]
    SVG(usvg::Tree),
    #[cfg(target_arch = "wasm32")]
    HTMLImage(HTMLImage),
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use image::GenericImageView;
        match self {
            ImageData::Texture(t) => {
                write!(f, "ImageData::Texture({:?})", t.id.0)
            }
            ImageData::DecodedImage(i) => {
                write!(f, "ImageData::DecodedImage({}x{})", i.width(), i.height())
            }
            ImageData::SVG(_) => {
                write!(f, "ImageData::SVG(...)")
            }
            #[cfg(target_arch = "wasm32")]
            ImageData::HTMLImage(html_image) => {
                write!(
                    f,
                    "ImageData::HTMLImage({}x{})",
                    html_image.dom_element.width(),
                    html_image.dom_element.height()
                )
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct CachedImage(RefCell<ImageData>);

impl CachedImage {
    fn new_on_cpu(decoded_image: image::DynamicImage) -> Self {
        Self(RefCell::new(ImageData::DecodedImage(decoded_image)))
    }

    pub fn new_on_gpu(canvas: &CanvasRc, image_id: femtovg::ImageId) -> Self {
        Self(RefCell::new(Texture { id: image_id, canvas: canvas.clone() }.into()))
    }

    pub fn new_empty_on_gpu(canvas: &CanvasRc, width: usize, height: usize) -> Self {
        let image_id = canvas
            .borrow_mut()
            .create_image_empty(
                width,
                height,
                femtovg::PixelFormat::Rgba8,
                femtovg::ImageFlags::PREMULTIPLIED | femtovg::ImageFlags::FLIP_Y,
            )
            .unwrap();
        Self::new_on_gpu(canvas, image_id)
    }

    #[cfg(feature = "svg")]
    fn new_on_cpu_svg(tree: usvg::Tree) -> Self {
        Self(RefCell::new(ImageData::SVG(tree)))
    }

    pub fn new_from_resource(resource: &ImageInner) -> Option<Self> {
        match resource {
            ImageInner::None => None,
            ImageInner::AbsoluteFilePath(path) => Self::new_from_path(path),
            ImageInner::EmbeddedData(data) => Self::new_from_data(data),
            ImageInner::EmbeddedRgbaImage { .. } => todo!(),
        }
    }

    fn new_from_path(path: &SharedString) -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(feature = "svg")]
            if path.ends_with(".svg") {
                return Some(Self::new_on_cpu_svg(
                    super::svg::load_from_path(std::path::Path::new(&path.as_str())).map_or_else(
                        |err| {
                            eprintln!("Error loading SVG from {}: {}", &path, err);
                            None
                        },
                        |svg_tree| Some(svg_tree),
                    )?,
                ));
            }
            Some(Self::new_on_cpu(image::open(std::path::Path::new(&path.as_str())).map_or_else(
                |decode_err| {
                    eprintln!("Error loading image from {}: {}", &path, decode_err);
                    None
                },
                |image| Some(image),
            )?))
        }
        #[cfg(target_arch = "wasm32")]
        Some(Self(RefCell::new(ImageData::HTMLImage(HTMLImage::new(path)))))
    }

    fn new_from_data(data: &Slice<u8>) -> Option<Self> {
        #[cfg(feature = "svg")]
        if data.starts_with(b"<svg") {
            return Some(CachedImage::new_on_cpu_svg(
                super::svg::load_from_data(data.as_slice()).map_or_else(
                    |svg_err| {
                        eprintln!("Error loading SVG: {}", svg_err);
                        None
                    },
                    |svg_tree| Some(svg_tree),
                )?,
            ));
        }
        Some(CachedImage::new_on_cpu(image::load_from_memory(data.as_slice()).map_or_else(
            |decode_err| {
                eprintln!("Error decoding image: {}", decode_err);
                None
            },
            |decoded_image| Some(decoded_image),
        )?))
    }

    // Upload the image to the GPU? if that hasn't happened yet. This function could take just a canvas
    // as parameter, but since an upload requires a current context, this is "enforced" by taking
    // a renderer instead (which implies a current context).
    pub fn ensure_uploaded_to_gpu(&self, current_renderer: &GLItemRenderer) -> femtovg::ImageId {
        use std::convert::TryFrom;

        let canvas = &current_renderer.shared_data.canvas;

        let img = &mut *self.0.borrow_mut();
        if let ImageData::DecodedImage(decoded_image) = img {
            let image_id = match femtovg::ImageSource::try_from(&*decoded_image) {
                Ok(image_source) => {
                    canvas.borrow_mut().create_image(image_source, femtovg::ImageFlags::empty())
                }
                Err(_) => {
                    let converted = image::DynamicImage::ImageRgba8(decoded_image.to_rgba8());
                    let image_source = femtovg::ImageSource::try_from(&converted).unwrap();
                    canvas.borrow_mut().create_image(image_source, femtovg::ImageFlags::empty())
                }
            }
            .unwrap();

            *img = Texture { id: image_id, canvas: canvas.clone() }.into()
        };

        #[cfg(target_arch = "wasm32")]
        if let ImageData::HTMLImage(html_image) = img {
            let image_id = canvas
                .borrow_mut()
                .create_image(&html_image.dom_element, femtovg::ImageFlags::empty())
                .unwrap();
            *img = Texture { id: image_id, canvas: canvas.clone() }.into()
        }

        match &img {
            ImageData::Texture(Texture { id, .. }) => *id,
            _ => unreachable!(),
        }
    }

    // Upload the image to the GPU. This function could take just a canvas as parameter,
    // but since an upload requires a current context, this is "enforced" by taking
    // a renderer instead (which implies a current context).
    pub fn upload_to_gpu(
        &self,
        current_renderer: &GLItemRenderer,
        target_size: euclid::default::Size2D<u32>,
    ) -> Option<Self> {
        use std::convert::TryFrom;

        let canvas = &current_renderer.shared_data.canvas;

        match &*self.0.borrow() {
            ImageData::Texture(_) => None, // internal error: Cannot call upload_to_gpu on previously uploaded image,
            ImageData::DecodedImage(decoded_image) => {
                let image_id = match femtovg::ImageSource::try_from(&*decoded_image) {
                    Ok(image_source) => {
                        canvas.borrow_mut().create_image(image_source, femtovg::ImageFlags::empty())
                    }
                    Err(_) => {
                        let converted = image::DynamicImage::ImageRgba8(decoded_image.to_rgba8());
                        let image_source = femtovg::ImageSource::try_from(&converted).unwrap();
                        canvas.borrow_mut().create_image(image_source, femtovg::ImageFlags::empty())
                    }
                }
                .unwrap();

                Some(Self::new_on_gpu(canvas, image_id))
            }
            #[cfg(feature = "svg")]
            ImageData::SVG(svg_tree) => match super::svg::render(&svg_tree, target_size) {
                Ok(rendered_svg_image) => Some(Self::new_on_cpu(rendered_svg_image)),
                Err(err) => {
                    eprintln!("Error rendering SVG: {}", err);
                    return None;
                }
            },
            #[cfg(target_arch = "wasm32")]
            ImageData::HTMLImage(html_image) => html_image.size().map(|_| {
                let image_id = canvas
                    .borrow_mut()
                    .create_image(&html_image.dom_element, femtovg::ImageFlags::empty())
                    .unwrap();
                Self::new_on_gpu(canvas, image_id)
            }),
        }
    }

    pub fn size(&self) -> Option<Size> {
        use image::GenericImageView;

        match &*self.0.borrow() {
            ImageData::Texture(texture) => texture.size(),
            ImageData::DecodedImage(decoded_image) => {
                let (width, height) = decoded_image.dimensions();
                Some([width as f32, height as f32].into())
            }

            #[cfg(feature = "svg")]
            ImageData::SVG(tree) => {
                let size = tree.svg_node().size.to_screen_size();
                Some([size.width() as f32, size.height() as f32].into())
            }

            #[cfg(target_arch = "wasm32")]
            ImageData::HTMLImage(html_image) => html_image.size(),
        }
    }

    pub(crate) fn as_render_target(&self) -> femtovg::RenderTarget {
        match &*self.0.borrow() {
            ImageData::Texture(tex) => femtovg::RenderTarget::Image(tex.id),
            _ => panic!(
                "internal error: CachedImage::as_render_target() called on non-texture image"
            ),
        }
    }

    pub(crate) fn filter(&self, canvas: &CanvasRc, filter: femtovg::ImageFilter) -> Self {
        let (image_id, size) = match &*self.0.borrow() {
            ImageData::Texture(texture) => texture.size().map(|size| (texture.id, size)),
            _ => None,
        }
        .expect("internal error: Cannot filter non-GPU images");

        let filtered_image = Self::new_empty_on_gpu(
            &canvas,
            size.width.ceil() as usize,
            size.height.ceil() as usize,
        );

        let filtered_image_id = match &*filtered_image.0.borrow() {
            ImageData::Texture(tex) => tex.id,
            _ => panic!("internal error: CachedImage::new_empty_on_gpu did not return texture!"),
        };

        canvas.borrow_mut().filter_image(filtered_image_id, filter, image_id);

        filtered_image
    }

    pub(crate) fn as_paint(&self) -> femtovg::Paint {
        match &*self.0.borrow() {
            ImageData::Texture(tex) => {
                let size = tex
                    .size()
                    .expect("internal error: CachedImage::as_paint() called on zero-sized texture");
                femtovg::Paint::image(tex.id, 0., 0., size.width, size.height, 0., 1.)
            }
            _ => panic!("internal error: CachedImage::as_paint() called on non-texture image"),
        }
    }

    pub(crate) fn is_on_gpu(&self) -> bool {
        match &*self.0.borrow() {
            ImageData::Texture(_) => true,
            _ => false,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum ImageCacheKey {
    Path(String),
    EmbeddedData(by_address::ByAddress<&'static [u8]>),
}

impl ImageCacheKey {
    pub fn new(resource: &ImageInner) -> Option<Self> {
        Some(match resource {
            ImageInner::None => return None,
            ImageInner::AbsoluteFilePath(path) => {
                if path.is_empty() {
                    return None;
                }
                Self::Path(path.to_string())
            }
            ImageInner::EmbeddedData(data) => {
                Self::EmbeddedData(by_address::ByAddress(data.as_slice()))
            }
            ImageInner::EmbeddedRgbaImage { .. } => return None,
        })
    }
}

// Cache used to avoid repeatedly decoding images from disk. Entries with a count
// of 1 are drained after flushing the renderer commands to the screen.
#[derive(Default)]
pub(crate) struct ImageCache(HashMap<ImageCacheKey, Rc<CachedImage>>);

impl ImageCache {
    // Look up the given image cache key in the image cache and upgrade the weak reference to a strong one if found,
    // otherwise a new image is created/loaded from the given callback.
    pub(crate) fn lookup_image_in_cache_or_create(
        &mut self,
        cache_key: ImageCacheKey,
        image_create_fn: impl Fn() -> Option<Rc<CachedImage>>,
    ) -> Option<Rc<CachedImage>> {
        Some(match self.0.entry(cache_key) {
            std::collections::hash_map::Entry::Occupied(existing_entry) => {
                existing_entry.get().clone()
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let new_image = image_create_fn()?;
                vacant_entry.insert(new_image.clone());
                new_image
            }
        })
    }

    // Try to load the image the given resource points to
    pub(crate) fn load_image_resource(&mut self, resource: &ImageInner) -> Option<Rc<CachedImage>> {
        let cache_key = ImageCacheKey::new(resource)?;

        self.lookup_image_in_cache_or_create(cache_key, || {
            CachedImage::new_from_resource(resource).map(Rc::new)
        })
    }

    pub(crate) fn drain(&mut self) {
        self.0.retain(|_, cached_image| {
            // * Retain images that are used by elements, so that they can be effectively
            // shared (one image element refers to foo.png, another element is created
            // and refers to the same -> share).
            // * Also retain images that are still loading (async HTML), where the size
            // is not known yet. Otherwise we end up in a loop where an image is not loaded
            // yet, we report (0, 0) to the layout, the image gets removed here, the closure
            // still triggers a load and marks the layout as dirt, which loads the
            // image again, etc.
            Rc::strong_count(cached_image) > 1 || cached_image.size().is_none()
        });
    }

    pub(crate) fn remove_textures(&mut self) {
        self.0.retain(|_, cached_image| !cached_image.is_on_gpu());
    }
}
