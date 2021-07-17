/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once

#include "sixtyfps_color_internal.h"
#include "sixtyfps_properties.h"

#include <stdint.h>

namespace sixtyfps {

namespace private_api {
class LinearGradientBrush;
}

class Color;

/// RgbaColor stores the red, green, blue and alpha components of a color
/// with the precision of the template parameter T. For example if T is float,
/// the values are normalized between 0 and 1. If T is uint8_t, they values range
/// is 0 to 255.
template<typename T>
struct RgbaColor
{
    /// The alpha component.
    T alpha;
    /// The red component.
    T red;
    /// The green component.
    T green;
    /// The blue component.
    T blue;

    /// Creates a new RgbaColor instance from a given color. This template function is
    /// specialized and thus implemented for T == uint8_t and T == float.
    RgbaColor(const Color &col);
};

/// Color represents a color in the SixtyFPS run-time, represented using 8-bit channels for
/// red, green, blue and the alpha (opacity).
class Color
{
public:
    /// Default constructs a new color that is entirely transparent.
    Color() { inner.red = inner.green = inner.blue = inner.alpha = 0; }
    /// Constructs a new color from the given RgbaColor<uint8_t> \a col.
    Color(const RgbaColor<uint8_t> &col)
    {
        inner.red = col.red;
        inner.green = col.green;
        inner.blue = col.blue;
        inner.alpha = col.alpha;
    }
    /// Constructs a new color from the given RgbaColor<float> \a col.
    Color(const RgbaColor<float> &col)
    {
        inner.red = uint8_t(col.red * 255);
        inner.green = uint8_t(col.green * 255);
        inner.blue = uint8_t(col.blue * 255);
        inner.alpha = uint8_t(col.alpha * 255);
    }

    /// Construct a color from an integer encoded as `0xAARRGGBB`
    static Color from_argb_encoded(uint32_t argb_encoded)
    {
        Color col;
        col.inner.red = (argb_encoded >> 16) & 0xff;
        col.inner.green = (argb_encoded >> 8) & 0xff;
        col.inner.blue = argb_encoded & 0xff;
        col.inner.alpha = (argb_encoded >> 24) & 0xff;
        return col;
    }

    /// Returns `(alpha, red, green, blue)` encoded as uint32_t.
    uint32_t as_argb_encoded() const
    {
        return (uint32_t(inner.red) << 16) | (uint32_t(inner.green) << 8) | uint32_t(inner.blue)
                | (uint32_t(inner.alpha) << 24);
    }

    /// Construct a color from the alpha, red, green and blue color channel parameters.
    static Color from_argb_uint8(uint8_t alpha, uint8_t red, uint8_t green, uint8_t blue)
    {
        Color col;
        col.inner.alpha = alpha;
        col.inner.red = red;
        col.inner.green = green;
        col.inner.blue = blue;
        return col;
    }

    /// Construct a color from the red, green and blue color channel parameters. The alpha
    /// channel will have the value 255.
    static Color from_rgb_uint8(uint8_t red, uint8_t green, uint8_t blue)
    {
        return from_argb_uint8(255, red, green, blue);
    }

    /// Construct a color from the alpha, red, green and blue color channel parameters.
    static Color from_argb_float(float alpha, float red, float green, float blue)
    {
        Color col;
        col.inner.alpha = uint8_t(alpha * 255);
        col.inner.red = uint8_t(red * 255);
        col.inner.green = uint8_t(green * 255);
        col.inner.blue = uint8_t(blue * 255);
        return col;
    }

    /// Construct a color from the red, green and blue color channel parameters. The alpha
    /// channel will have the value 255.
    static Color from_rgb_float(float red, float green, float blue)
    {
        return Color::from_argb_float(1.0, red, green, blue);
    }

    /// Converts this color to an RgbaColor struct for easy destructuring.
    inline RgbaColor<uint8_t> to_argb_uint() const;

    /// Converts this color to an RgbaColor struct for easy destructuring.
    inline RgbaColor<float> to_argb_float() const;

    /// Returns the red channel of the color as u8 in the range 0..255.
    uint8_t red() const { return inner.red; }

    /// Returns the green channel of the color as u8 in the range 0..255.
    uint8_t green() const { return inner.green; }

    /// Returns the blue channel of the color as u8 in the range 0..255.
    uint8_t blue() const { return inner.blue; }

    /// Returns the alpha channel of the color as u8 in the range 0..255.
    uint8_t alpha() const { return inner.alpha; }

    /// Returns a new version of this color that has the brightness increased
    /// by the specified factor. This is done by converting the color to the HSV
    /// color space and multiplying the brightness (value) with (1 + factor).
    /// The result is converted back to RGB and the alpha channel is unchanged.
    /// So for example `brighter(0.2)` will increase the brightness by 20%, and
    /// calling `brighter(-0.5)` will return a color that's 50% darker.
    inline Color brighter(float factor) const;
    /// Returns a new version of this color that has the brightness decreased
    /// by the specified factor. This is done by converting the color to the HSV
    /// color space and dividing the brightness (value) by (1 + factor). The
    /// result is converted back to RGB and the alpha channel is unchanged.
    /// So for example `darker(0.3)` will decrease the brightness by 30%.
    inline Color darker(float factor) const;

    /// Returns true if \a lhs has the same values for the individual color channels as \a rhs;
    /// false otherwise.
    friend bool operator==(const Color &lhs, const Color &rhs)
    {
        return lhs.inner.red == rhs.inner.red && lhs.inner.green == rhs.inner.green
                && lhs.inner.blue == rhs.inner.blue && lhs.inner.alpha == rhs.inner.alpha;
    }

    /// Returns true if \a lhs has any different values for the individual color channels as \a rhs;
    /// false otherwise.
    friend bool operator!=(const Color &lhs, const Color &rhs) { return !(lhs == rhs); }

    /// Writes the \a color to the specified \a stream and returns a reference to the
    /// stream.
    friend std::ostream &operator<<(std::ostream &stream, const Color &color)
    {
        // Cast to uint32_t to avoid the components being interpreted as char.
        return stream << "argb(" << uint32_t(color.inner.alpha) << ", " << uint32_t(color.inner.red)
                      << ", " << uint32_t(color.inner.green) << ", " << uint32_t(color.inner.blue)
                      << ")";
    }

#if !defined(DOXYGEN)
    // FIXME: we need this to create GradientStop
    operator const cbindgen_private::types::Color &() { return inner; }
#endif

private:
    cbindgen_private::types::Color inner;
    friend class private_api::LinearGradientBrush;
    friend class Brush;
};

inline Color Color::brighter(float factor) const
{
    Color result;
    cbindgen_private::types::sixtyfps_color_brighter(&inner, factor, &result.inner);
    return result;
}

inline Color Color::darker(float factor) const
{
    Color result;
    cbindgen_private::types::sixtyfps_color_darker(&inner, factor, &result.inner);
    return result;
}

/// Constructs a new RgbaColor<uint8_t> from the color \a color.
template<>
inline RgbaColor<uint8_t>::RgbaColor(const Color &color)
{
    red = color.red();
    green = color.green();
    blue = color.blue();
    alpha = color.alpha();
}

/// Constructs a new RgbaColor<float> from the color \a color.
template<>
inline RgbaColor<float>::RgbaColor(const Color &color)
{
    red = float(color.red()) / 255.f;
    green = float(color.green()) / 255.f;
    blue = float(color.blue()) / 255.f;
    alpha = float(color.alpha()) / 255.f;
}

RgbaColor<uint8_t> Color::to_argb_uint() const
{
    return RgbaColor<uint8_t>(*this);
}

RgbaColor<float> Color::to_argb_float() const
{
    return RgbaColor<float>(*this);
}

namespace private_api {

template<>
inline void
Property<Color>::set_animated_value(const Color &new_value,
                                    const cbindgen_private::PropertyAnimation &animation_data) const
{
    cbindgen_private::sixtyfps_property_set_animated_value_color(&inner, value, new_value,
                                                                 &animation_data);
}

} // namespace private_api

} // namespace sixtyfps
