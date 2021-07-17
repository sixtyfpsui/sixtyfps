/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This module contains brush related types for the run-time library.
*/

use super::{Color, Point};
use crate::properties::InterpolatedPropertyValue;
use crate::SharedVector;

/// A brush is a data structure that is used to describe how
/// a shape, such as a rectangle, path or even text, shall be filled.
/// A brush can also be applied to the outline of a shape, that means
/// the fill of the outline itself.
#[derive(Clone, PartialEq, Debug, derive_more::From)]
#[repr(C)]
#[non_exhaustive]
pub enum Brush {
    /// The color variant of brush is a plain color that is to be used for the fill.
    SolidColor(Color),
    /// The linear gradient variant of a brush describes the gradient stops for a fill
    /// where all color stops are along a line that's rotated by the specified angle.
    LinearGradient(LinearGradientBrush),
}

/// Construct a brush with transparent color
impl Default for Brush {
    fn default() -> Self {
        Self::SolidColor(Color::default())
    }
}

impl Brush {
    /// If the brush is SolidColor, the contained color is returned.
    /// If the brush is a LinearGradient, the color of the first stop is returned.
    pub fn color(&self) -> Color {
        match self {
            Brush::SolidColor(col) => *col,
            Brush::LinearGradient(gradient) => {
                gradient.stops().next().map(|stop| stop.color).unwrap_or_default()
            }
        }
    }

    /// Returns true if this brush contains a fully transparent color (alpha value is zero)
    ///
    /// ```
    /// # use sixtyfps_corelib::graphics::*;
    /// assert!(Brush::default().is_transparent());
    /// assert!(Brush::SolidColor(Color::from_argb_u8(0, 255, 128, 140)).is_transparent());
    /// assert!(!Brush::SolidColor(Color::from_argb_u8(25, 128, 140, 210)).is_transparent());
    /// ```
    pub fn is_transparent(&self) -> bool {
        match self {
            Brush::SolidColor(c) => c.alpha() == 0,
            Brush::LinearGradient(_) => false,
        }
    }
}

/// The LinearGradientBrush describes a way of filling a shape with different colors, which
/// are interpolated between different stops. The colors are aligned with a line that's rotated
/// by the LinearGradient's angle.
#[derive(Clone, PartialEq, Debug)]
#[repr(transparent)]
pub struct LinearGradientBrush(SharedVector<GradientStop>);

impl LinearGradientBrush {
    /// Creates a new linear gradient, described by the specified angle and the provided color stops.
    ///
    /// The angle need to be specified in degrees.
    /// The stops don't need to be sorted as this function will sort them.
    pub fn new(angle: f32, stops: impl IntoIterator<Item = GradientStop>) -> Self {
        let stop_iter = stops.into_iter();
        let mut encoded_angle_and_stops = SharedVector::with_capacity(stop_iter.size_hint().0 + 1);
        // The gradient's first stop is a fake stop to store the angle
        encoded_angle_and_stops.push(GradientStop { color: Default::default(), position: angle });
        encoded_angle_and_stops.extend(stop_iter);
        Self(encoded_angle_and_stops)
    }
    /// Returns the angle of the linear gradient in degrees.
    pub fn angle(&self) -> f32 {
        self.0[0].position
    }
    /// Returns the color stops of the linear gradient.
    /// The stops are sorted by positions.
    pub fn stops(&self) -> impl Iterator<Item = &GradientStop> {
        // skip the first fake stop that just contains the angle
        self.0.iter().skip(1)
    }
}

/// GradientStop describes a single color stop in a gradient. The colors between multiple
/// stops are interpolated.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GradientStop {
    /// The color to draw at this stop.
    pub color: Color,
    /// The position of this stop on the entire shape, as a normalized value between 0 and 1.
    pub position: f32,
}

/// Returns the start / end points of a gradient within the [-0.5; 0.5] unit square, based on the angle (in degree).
pub fn line_for_angle(angle: f32) -> (Point, Point) {
    let angle = angle.to_radians();
    let r = (angle.sin().abs() + angle.cos().abs()) / 2.;
    let (y, x) = (angle - std::f32::consts::PI / 2.).sin_cos();
    let (y, x) = (y * r, x * r);
    let start = Point::new(0.5 - x, 0.5 - y);
    let end = Point::new(0.5 + x, 0.5 + y);
    (start, end)
}

impl InterpolatedPropertyValue for Brush {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        match (self, target_value) {
            (Brush::SolidColor(source_col), Brush::SolidColor(target_col)) => {
                Brush::SolidColor(source_col.interpolate(target_col, t))
            }
            (Brush::SolidColor(col), Brush::LinearGradient(grad)) => {
                let mut new_grad = grad.clone();
                for x in new_grad.0.as_slice_mut().iter_mut().skip(1) {
                    x.color = col.interpolate(&x.color, t);
                }
                Brush::LinearGradient(new_grad)
            }
            (a @ Brush::LinearGradient(_), b @ Brush::SolidColor(_)) => {
                Self::interpolate(b, a, 1. - t)
            }
            (Brush::LinearGradient(lhs), Brush::LinearGradient(rhs)) => {
                if lhs.0.len() < rhs.0.len() {
                    Self::interpolate(target_value, self, 1. - t)
                } else {
                    let mut new_grad = lhs.clone();
                    let mut iter = new_grad.0.as_slice_mut().iter_mut();
                    {
                        let angle = &mut iter.next().unwrap().position;
                        *angle = angle.interpolate(&rhs.angle(), t);
                    }
                    let mut rhs_stops = rhs.stops();
                    while let (Some(s1), Some(s2)) = (iter.next(), rhs_stops.next()) {
                        s1.color = s1.color.interpolate(&s2.color, t);
                        s1.position = s1.position.interpolate(&s2.position, t);
                    }
                    for x in iter {
                        x.position = x.position.interpolate(&1.0, t);
                    }
                    Brush::LinearGradient(new_grad)
                }
            }
        }
    }
}

#[test]
#[allow(clippy::float_cmp)] // We want bit-wise equality here
fn test_linear_gradient_encoding() {
    let stops: SharedVector<GradientStop> = [
        GradientStop { position: 0.0, color: Color::from_argb_u8(255, 255, 0, 0) },
        GradientStop { position: 0.5, color: Color::from_argb_u8(255, 0, 255, 0) },
        GradientStop { position: 1.0, color: Color::from_argb_u8(255, 0, 0, 255) },
    ]
    .into();
    let grad = LinearGradientBrush::new(256., stops.clone());
    assert_eq!(grad.angle(), 256.);
    assert!(grad.stops().eq(stops.iter()));
}
