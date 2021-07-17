/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once
#include <string_view>
#include "sixtyfps_color.h"
#include "sixtyfps_brush_internal.h"
#include "sixtyfps_string.h"

namespace sixtyfps {

namespace private_api {

using cbindgen_private::types::GradientStop;

/// \private
/// LinearGradientBrush represents a gradient for a brush that is a linear sequence of color stops,
/// that are aligned at a specific angle.
class LinearGradientBrush
{
public:
    /// Constructs an empty linear gradient with no color stops.
    LinearGradientBrush() = default;
    /// Constructs a new linear gradient with the specified \a angle. The color stops will be
    /// constructed from the stops array pointed to be \a firstStop, with the length \a stopCount.
    LinearGradientBrush(float angle, const GradientStop *firstStop, int stopCount)
        : inner(make_linear_gradient(angle, firstStop, stopCount))
    {
    }

    /// Returns the linear gradient's angle in degrees.
    float angle() const
    {
        // The gradient's first stop is a fake stop to store the angle
        return inner[0].position;
    }

    /// Returns the number of gradient stops.
    int stopCount() const { return int(inner.size()) - 1; }

    /// Returns a pointer to the first gradient stop; undefined if the gradient has not stops.
    const GradientStop *stopsBegin() const { return inner.begin() + 1; }
    /// Returns a pointer past the last gradient stop. The returned pointer cannot be dereferenced,
    /// it can only be used for comparison.
    const GradientStop *stopsEnd() const { return inner.end(); }

private:
    cbindgen_private::types::LinearGradientBrush inner;

    friend class sixtyfps::Brush;

    static SharedVector<private_api::GradientStop>
    make_linear_gradient(float angle, const GradientStop *firstStop, int stopCount)
    {
        SharedVector<private_api::GradientStop> gradient;
        gradient.push_back({ Color::from_argb_encoded(0).inner, angle });
        for (int i = 0; i < stopCount; ++i, ++firstStop)
            gradient.push_back(*firstStop);
        return gradient;
    }
};

}

/// Brush is used to declare how to fill or outline shapes, such as rectangles, paths or text. A
/// brush is either a solid color or a linear gradient.
class Brush
{
public:
    /// Constructs a new brush that is a transparent color.
    Brush() : Brush(Color {}) { }
    /// Constructs a new brush that is of color \a color.
    Brush(const Color &color) : data(Inner::SolidColor(color.inner)) { }
    /// \private
    /// Constructs a new brush that is the gradient \a gradient.
    Brush(const private_api::LinearGradientBrush &gradient)
        : data(Inner::LinearGradient(gradient.inner))
    {
    }

    /// Returns the color of the brush. If the brush is a gradient, this function returns the color
    /// of the first stop.
    inline Color color() const;

    /// Returns true if \a a is equal to \a b. If \a a holds a color, then \a b must also hold a
    /// color that is identical to \a a's color. If it holds a gradient, then the gradients must be
    /// identical. Returns false if the brushes differ in what they hold or their respective color
    /// or gradient are not equal.
    friend bool operator==(const Brush &a, const Brush &b) { return a.data == b.data; }
    /// Returns false if \a is not equal to \a b; true otherwise.
    friend bool operator!=(const Brush &a, const Brush &b) { return a.data != b.data; }

private:
    using Tag = cbindgen_private::types::Brush::Tag;
    using Inner = cbindgen_private::types::Brush;
    Inner data;
};

Color Brush::color() const
{
    Color result;
    switch (data.tag) {
    case Tag::SolidColor: {
        result.inner = data.solid_color._0;
        break;
    }
    case Tag::LinearGradient:
        if (data.linear_gradient._0.size() > 1) {
            result.inner = data.linear_gradient._0[1].color;
        }
        break;
    }
    return result;
}

}
