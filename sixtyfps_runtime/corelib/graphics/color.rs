/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This module contains color related types for the run-time library.
*/

use crate::properties::InterpolatedPropertyValue;

/// RgbaColor stores the red, green, blue and alpha components of a color
/// with the precision of the generic parameter T. For example if T is f32,
/// the values are normalized between 0 and 1. If T is u8, they values range
/// is 0 to 255.
/// This is merely a helper class for use with [`Color`].
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct RgbaColor<T> {
    /// The alpha component.
    pub alpha: T,
    /// The red channel.
    pub red: T,
    /// The green channel.
    pub green: T,
    /// The blue channel.
    pub blue: T,
}

/// Color represents a color in the SixtyFPS run-time, represented using 8-bit channels for
/// red, green, blue and the alpha (opacity).
/// It can be conveniently converted using the `to_` and `from_` (a)rgb helper functions:
/// ```
/// # fn do_something_with_red_and_green(_:f32, _:f32) {}
/// # fn do_something_with_red(_:u8) {}
/// # use sixtyfps_corelib::graphics::{Color, RgbaColor};
/// # let some_color = Color::from_rgb_u8(0, 0, 0);
/// let col = some_color.to_argb_f32();
/// do_something_with_red_and_green(col.red, col.green);
///
/// let RgbaColor { red, blue, green, .. } = some_color.to_argb_u8();
/// do_something_with_red(red);
///
/// let new_col = Color::from(RgbaColor{ red: 0.5, green: 0.65, blue: 0.32, alpha: 1.});
/// ```
#[derive(Copy, Clone, PartialEq, Debug, Default)]
#[repr(C)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl From<RgbaColor<u8>> for Color {
    fn from(col: RgbaColor<u8>) -> Self {
        Self { red: col.red, green: col.green, blue: col.blue, alpha: col.alpha }
    }
}

impl From<Color> for RgbaColor<u8> {
    fn from(col: Color) -> Self {
        RgbaColor { red: col.red, green: col.green, blue: col.blue, alpha: col.alpha }
    }
}

impl From<RgbaColor<u8>> for RgbaColor<f32> {
    fn from(col: RgbaColor<u8>) -> Self {
        Self {
            red: (col.red as f32) / 255.0,
            green: (col.green as f32) / 255.0,
            blue: (col.blue as f32) / 255.0,
            alpha: (col.alpha as f32) / 255.0,
        }
    }
}

impl From<Color> for RgbaColor<f32> {
    fn from(col: Color) -> Self {
        let u8col: RgbaColor<u8> = col.into();
        u8col.into()
    }
}

impl From<RgbaColor<f32>> for Color {
    fn from(col: RgbaColor<f32>) -> Self {
        Self {
            red: (col.red * 255.) as u8,
            green: (col.green * 255.) as u8,
            blue: (col.blue * 255.) as u8,
            alpha: (col.alpha * 255.) as u8,
        }
    }
}

impl Color {
    /// Construct a color from an integer encoded as `0xAARRGGBB`
    pub const fn from_argb_encoded(encoded: u32) -> Color {
        Self {
            red: (encoded >> 16) as u8,
            green: (encoded >> 8) as u8,
            blue: encoded as u8,
            alpha: (encoded >> 24) as u8,
        }
    }

    /// Returns `(alpha, red, green, blue)` encoded as u32
    pub fn as_argb_encoded(&self) -> u32 {
        ((self.red as u32) << 16)
            | ((self.green as u32) << 8)
            | (self.blue as u32)
            | ((self.alpha as u32) << 24)
    }

    /// Construct a color from the alpha, red, green and blue color channel parameters.
    pub fn from_argb_u8(alpha: u8, red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue, alpha }
    }

    /// Construct a color from the red, green and blue color channel parameters. The alpha
    /// channel will have the value 255.
    pub fn from_rgb_u8(red: u8, green: u8, blue: u8) -> Self {
        Self::from_argb_u8(255, red, green, blue)
    }

    /// Construct a color from the alpha, red, green and blue color channel parameters.
    pub fn from_argb_f32(alpha: f32, red: f32, green: f32, blue: f32) -> Self {
        RgbaColor { alpha, red, green, blue }.into()
    }

    /// Construct a color from the red, green and blue color channel parameters. The alpha
    /// channel will have the value 255.
    pub fn from_rgb_f32(red: f32, green: f32, blue: f32) -> Self {
        Self::from_argb_f32(1.0, red, green, blue)
    }

    /// Converts this color to an RgbaColor struct for easy destructuring.
    pub fn to_argb_u8(&self) -> RgbaColor<u8> {
        RgbaColor::from(*self)
    }

    /// Converts this color to an RgbaColor struct for easy destructuring.
    pub fn to_argb_f32(&self) -> RgbaColor<f32> {
        RgbaColor::from(*self)
    }

    /// Returns the red channel of the color as u8 in the range 0..255.
    pub fn red(self) -> u8 {
        self.red
    }

    /// Returns the green channel of the color as u8 in the range 0..255.
    pub fn green(self) -> u8 {
        self.green
    }

    /// Returns the blue channel of the color as u8 in the range 0..255.
    pub fn blue(self) -> u8 {
        self.blue
    }

    /// Returns the alpha channel of the color as u8 in the range 0..255.
    pub fn alpha(self) -> u8 {
        self.alpha
    }

    /// Returns a new version of this color that has the brightness increased
    /// by the specified factor. This is done by converting the color to the HSV
    /// color space and multiplying the brightness (value) with (1 + factor).
    /// The result is converted back to RGB and the alpha channel is unchanged.
    /// So for example `brighter(0.2)` will increase the brightness by 20%, and
    /// calling `brighter(-0.5)` will return a color that's 50% darker.
    pub fn brighter(&self, factor: f32) -> Self {
        let rgba: RgbaColor<f32> = (*self).into();
        let mut hsva: HsvaColor = rgba.into();
        hsva.v *= 1. + factor;
        let rgba: RgbaColor<f32> = hsva.into();
        rgba.into()
    }

    /// Returns a new version of this color that has the brightness decreased
    /// by the specified factor. This is done by converting the color to the HSV
    /// color space and dividing the brightness (value) by (1 + factor). The
    /// result is converted back to RGB and the alpha channel is unchanged.
    /// So for example `darker(0.3)` will decrease the brightness by 30%.
    pub fn darker(&self, factor: f32) -> Self {
        let rgba: RgbaColor<f32> = (*self).into();
        let mut hsva: HsvaColor = rgba.into();
        hsva.v /= 1. + factor;
        let rgba: RgbaColor<f32> = hsva.into();
        rgba.into()
    }
}

impl InterpolatedPropertyValue for Color {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        Self {
            red: self.red.interpolate(&target_value.red, t),
            green: self.green.interpolate(&target_value.green, t),
            blue: self.blue.interpolate(&target_value.blue, t),
            alpha: self.alpha.interpolate(&target_value.alpha, t),
        }
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "argb({}, {}, {}, {})", self.alpha, self.red, self.green, self.blue)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HsvaColor {
    h: f32,
    s: f32,
    v: f32,
    alpha: f32,
}

impl From<RgbaColor<f32>> for HsvaColor {
    fn from(col: RgbaColor<f32>) -> Self {
        // RGB to HSL conversion from https://en.wikipedia.org/wiki/HSL_and_HSV#Color_conversion_formulae

        let red = col.red;
        let green = col.green;
        let blue = col.blue;

        let min = red.min(green).min(blue);
        let max = red.max(green).max(blue);
        let chroma = max - min;

        #[allow(clippy::float_cmp)] // `max` is either `red`, `green` or `blue`
        let hue = 60.
            * if chroma == 0. {
                0.0
            } else if max == red {
                ((green - blue) / chroma) % 6.0
            } else if max == green {
                2. + (blue - red) / chroma
            } else {
                4. + (red - green) / chroma
            };

        let saturation = if max == 0. { 0. } else { chroma / max };

        Self { h: hue, s: saturation, v: max, alpha: col.alpha }
    }
}

impl From<HsvaColor> for RgbaColor<f32> {
    fn from(col: HsvaColor) -> Self {
        // RGB to HSL conversion from https://en.wikipedia.org/wiki/HSL_and_HSV#Color_conversion_formulae

        let chroma = col.s * col.v;

        let x = chroma * (1. - ((col.h / 60.) % 2. - 1.).abs());

        let (red, green, blue) = match (col.h / 60.0) as usize {
            0 => (chroma, x, 0.),
            1 => (x, chroma, 0.),
            2 => (0., chroma, x),
            3 => (0., x, chroma),
            4 => (x, 0., chroma),
            5 => (chroma, 0., x),
            _ => (0., 0., 0.),
        };

        let m = col.v - chroma;

        Self { red: red + m, green: green + m, blue: blue + m, alpha: col.alpha }
    }
}

#[test]
fn test_rgb_to_hsv() {
    // White
    assert_eq!(
        HsvaColor::from(RgbaColor::<f32> { red: 1., green: 1., blue: 1., alpha: 0.5 }),
        HsvaColor { h: 0., s: 0., v: 1., alpha: 0.5 }
    );
    assert_eq!(
        RgbaColor::<f32>::from(HsvaColor { h: 0., s: 0., v: 1., alpha: 0.3 }),
        RgbaColor::<f32> { red: 1., green: 1., blue: 1., alpha: 0.3 }
    );

    // Bright greenish, verified via colorizer.org
    assert_eq!(
        HsvaColor::from(RgbaColor::<f32> { red: 0., green: 0.9, blue: 0., alpha: 1.0 }),
        HsvaColor { h: 120., s: 1., v: 0.9, alpha: 1.0 }
    );
    assert_eq!(
        RgbaColor::<f32>::from(HsvaColor { h: 120., s: 1., v: 0.9, alpha: 1.0 }),
        RgbaColor::<f32> { red: 0., green: 0.9, blue: 0., alpha: 1.0 }
    );
}

#[test]
fn test_brighter_darker() {
    let blue = Color::from_rgb_u8(0, 0, 128);
    assert_eq!(blue.brighter(0.5), Color::from_rgb_u8(0, 0, 192));
    assert_eq!(blue.darker(0.5), Color::from_rgb_u8(0, 0, 85));
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]
    use super::*;

    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_color_brighter(col: &Color, factor: f32, out: *mut Color) {
        core::ptr::write(out, col.brighter(factor))
    }

    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_color_darker(col: &Color, factor: f32, out: *mut Color) {
        core::ptr::write(out, col.darker(factor))
    }
}
