/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Runtime support for layouts.

// cspell:ignore coord

use crate::{slice::Slice, SharedVector};

/// Vertical or Horizontal orientation
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

type Coord = f32;

/// The constraint that applies to an item
// NOTE: when adding new fields, the C++ operator== also need updates
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutInfo {
    /// The minimum size for this item.
    pub min: f32,
    /// The maximum size for the item.
    pub max: f32,

    /// The minimum size in percentage of the parent (value between 0 and 100).
    pub min_percent: f32,
    /// The maximum size in percentage of the parent (value between 0 and 100).
    pub max_percent: f32,

    /// the preferred size
    pub preferred: f32,

    /// the  stretch factor
    pub stretch: f32,
}

impl Default for LayoutInfo {
    fn default() -> Self {
        LayoutInfo {
            min: 0.,
            max: f32::MAX,
            min_percent: 0.,
            max_percent: 100.,
            preferred: 0.,
            stretch: 0.,
        }
    }
}

impl LayoutInfo {
    // Note: This "logic" is duplicated in the cpp generator's generated code for merging layout infos.
    pub fn merge(&self, other: &LayoutInfo) -> Self {
        Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
            min_percent: self.min_percent.max(other.min_percent),
            max_percent: self.max_percent.min(other.max_percent),
            preferred: self.preferred.max(other.preferred),
            stretch: self.stretch.min(other.stretch),
        }
    }

    /// Helper function to return a preferred size which is within the min/max constraints
    pub fn preferred_bounded(&self) -> f32 {
        self.preferred.min(self.max).max(self.min)
    }
}

impl core::ops::Add for LayoutInfo {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.merge(&rhs)
    }
}

mod grid_internal {
    use super::*;

    fn order_coord(a: &Coord, b: &Coord) -> std::cmp::Ordering {
        a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal)
    }

    #[derive(Debug, Clone)]
    pub struct LayoutData {
        // inputs
        pub min: Coord,
        pub max: Coord,
        pub pref: Coord,
        pub stretch: f32,

        // outputs
        pub pos: Coord,
        pub size: Coord,
    }

    impl Default for LayoutData {
        fn default() -> Self {
            LayoutData { min: 0., max: Coord::MAX, pref: 0., stretch: f32::MAX, pos: 0., size: 0. }
        }
    }

    trait Adjust {
        fn can_grow(_: &LayoutData) -> Coord;
        fn to_distribute(expected_size: Coord, current_size: Coord) -> Coord;
        fn distribute(_: &mut LayoutData, val: Coord);
    }

    struct Grow;
    impl Adjust for Grow {
        fn can_grow(it: &LayoutData) -> Coord {
            it.max - it.size
        }

        fn to_distribute(expected_size: Coord, current_size: Coord) -> Coord {
            expected_size - current_size
        }

        fn distribute(it: &mut LayoutData, val: Coord) {
            it.size += val;
        }
    }

    struct Shrink;
    impl Adjust for Shrink {
        fn can_grow(it: &LayoutData) -> Coord {
            it.size - it.min
        }

        fn to_distribute(expected_size: Coord, current_size: Coord) -> Coord {
            current_size - expected_size
        }

        fn distribute(it: &mut LayoutData, val: Coord) {
            it.size -= val;
        }
    }

    fn adjust_items<A: Adjust>(data: &mut [LayoutData], size_without_spacing: Coord) -> Option<()> {
        loop {
            let size_cannot_grow: Coord =
                data.iter().filter(|it| A::can_grow(it) <= 0.).map(|it| it.size).sum();

            let total_stretch: f32 =
                data.iter().filter(|it| A::can_grow(it) > 0.).map(|it| it.stretch).sum();

            let actual_stretch = |s: f32| if total_stretch <= 0. { 1. } else { s };

            let max_grow = data
                .iter()
                .filter(|it| A::can_grow(it) > 0.)
                .map(|it| A::can_grow(it) / actual_stretch(it.stretch))
                .min_by(order_coord)?;

            let current_size: Coord =
                data.iter().filter(|it| A::can_grow(it) > 0.).map(|it| it.size).sum();

            //let to_distribute = size_without_spacing - (size_cannot_grow + current_size);
            let to_distribute =
                A::to_distribute(size_without_spacing, size_cannot_grow + current_size);
            if to_distribute <= 0. || max_grow <= 0. {
                return Some(());
            }

            let grow = if total_stretch <= 0. {
                to_distribute / (data.iter().filter(|it| A::can_grow(it) > 0.).count() as Coord)
            } else {
                to_distribute / total_stretch
            }
            .min(max_grow);

            for it in data.iter_mut().filter(|it| A::can_grow(it) > 0.) {
                A::distribute(it, grow * actual_stretch(it.stretch));
            }
        }
    }

    pub fn layout_items(data: &mut [LayoutData], start_pos: Coord, size: Coord, spacing: Coord) {
        let size_without_spacing = size - spacing * (data.len() - 1) as Coord;

        let mut pref = 0.;
        for it in data.iter_mut() {
            it.size = it.pref;
            pref += it.pref;
        }
        if size_without_spacing >= pref {
            adjust_items::<Grow>(data, size_without_spacing);
        } else if size_without_spacing < pref {
            adjust_items::<Shrink>(data, size_without_spacing);
        }

        let mut pos = start_pos;
        for it in data.iter_mut() {
            it.pos = pos;
            pos += it.size + spacing;
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // We want bit-wise equality here
    fn test_layout_items() {
        let my_items = &mut [
            LayoutData { min: 100., max: 200., pref: 100., stretch: 1., ..Default::default() },
            LayoutData { min: 50., max: 300., pref: 100., stretch: 1., ..Default::default() },
            LayoutData { min: 50., max: 150., pref: 100., stretch: 1., ..Default::default() },
        ];

        layout_items(my_items, 100., 650., 0.);
        assert_eq!(my_items[0].size, 200.);
        assert_eq!(my_items[1].size, 300.);
        assert_eq!(my_items[2].size, 150.);

        layout_items(my_items, 100., 200., 0.);
        assert_eq!(my_items[0].size, 100.);
        assert_eq!(my_items[1].size, 50.);
        assert_eq!(my_items[2].size, 50.);

        layout_items(my_items, 100., 300., 0.);
        assert_eq!(my_items[0].size, 100.);
        assert_eq!(my_items[1].size, 100.);
        assert_eq!(my_items[2].size, 100.);
    }

    /// Create a vector of LayoutData for an array of GridLayoutCellData
    pub fn to_layout_data(
        data: &[GridLayoutCellData],
        spacing: Coord,
        size: Option<Coord>,
    ) -> Vec<LayoutData> {
        let mut num = 0;
        for cell in data {
            num = num.max(cell.col_or_row + cell.span);
        }
        if num < 1 {
            return Default::default();
        }
        let mut layout_data =
            vec![grid_internal::LayoutData { stretch: 1., ..Default::default() }; num as usize];
        let mut has_spans = false;
        for cell in data {
            let constraint = &cell.constraint;
            let mut max = constraint.max;
            if let Some(size) = size {
                max = max.min(size * constraint.max_percent / 100.);
            }
            for c in 0..(cell.span as usize) {
                let cdata = &mut layout_data[cell.col_or_row as usize + c];
                cdata.max = cdata.max.min(max);
            }
            if cell.span == 1 {
                let mut min = constraint.min;
                if let Some(size) = size {
                    min = min.max(size * constraint.min_percent / 100.);
                }
                let pref = constraint.preferred.min(max).max(min);
                let cdata = &mut layout_data[cell.col_or_row as usize];
                cdata.min = cdata.min.max(min);
                cdata.pref = cdata.pref.max(pref);
                cdata.stretch = cdata.stretch.min(constraint.stretch);
            } else {
                has_spans = true;
            }
        }
        if has_spans {
            // Adjust minimum sizes
            for cell in data.iter().filter(|cell| cell.span > 1) {
                let span_data = &mut layout_data
                    [(cell.col_or_row as usize)..(cell.col_or_row + cell.span) as usize];
                let mut min = cell.constraint.min;
                if let Some(size) = size {
                    min = min.max(size * cell.constraint.min_percent / 100.);
                }
                grid_internal::layout_items(span_data, 0., min, spacing);
                for cdata in span_data {
                    if cdata.min < cdata.size {
                        cdata.min = cdata.size;
                    }
                }
            }
            // Adjust maximum sizes
            for cell in data.iter().filter(|cell| cell.span > 1) {
                let span_data = &mut layout_data
                    [(cell.col_or_row as usize)..(cell.col_or_row + cell.span) as usize];
                let mut max = cell.constraint.max;
                if let Some(size) = size {
                    max = max.min(size * cell.constraint.max_percent / 100.);
                }
                grid_internal::layout_items(span_data, 0., max, spacing);
                for cdata in span_data {
                    if cdata.max > cdata.size {
                        cdata.max = cdata.size;
                    }
                }
            }
            // Adjust preferred sizes
            for cell in data.iter().filter(|cell| cell.span > 1) {
                let span_data = &mut layout_data
                    [(cell.col_or_row as usize)..(cell.col_or_row + cell.span) as usize];
                grid_internal::layout_items(span_data, 0., cell.constraint.preferred, spacing);
                for cdata in span_data {
                    cdata.pref = cdata.pref.max(cdata.size).min(cdata.max).max(cdata.min);
                }
            }
            // Adjust stretches
            for cell in data.iter().filter(|cell| cell.span > 1) {
                let span_data = &mut layout_data
                    [(cell.col_or_row as usize)..(cell.col_or_row + cell.span) as usize];
                let total_stretch: f32 = span_data.iter().map(|c| c.stretch).sum();
                if total_stretch > cell.constraint.stretch {
                    for cdata in span_data {
                        cdata.stretch *= cell.constraint.stretch / total_stretch;
                    }
                }
            }
        }
        layout_data
    }
}

#[repr(C)]
pub struct Constraint {
    pub min: Coord,
    pub max: Coord,
}

impl Default for Constraint {
    fn default() -> Self {
        Constraint { min: 0., max: Coord::MAX }
    }
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct Padding {
    pub begin: Coord,
    pub end: Coord,
}

#[repr(C)]
#[derive(Debug)]
pub struct GridLayoutData<'a> {
    pub size: Coord,
    pub spacing: Coord,
    pub padding: &'a Padding,
    pub cells: Slice<'a, GridLayoutCellData>,
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct GridLayoutCellData {
    /// col, or row.
    pub col_or_row: u16,
    /// colspan or rowspan
    pub span: u16,
    pub constraint: LayoutInfo,
}

/// return, an array which is of size `data.cells.len() * 2` which for each cell we give the pos, size
pub fn solve_grid_layout(data: &GridLayoutData) -> SharedVector<Coord> {
    let mut layout_data =
        grid_internal::to_layout_data(data.cells.as_slice(), data.spacing, Some(data.size));

    if layout_data.is_empty() {
        return Default::default();
    }

    grid_internal::layout_items(
        &mut layout_data,
        data.padding.begin,
        data.size - (data.padding.begin + data.padding.end),
        data.spacing,
    );

    let mut result = SharedVector::with_capacity(4 * data.cells.len());
    for cell in data.cells.iter() {
        let cdata = &layout_data[cell.col_or_row as usize];
        result.push(cdata.pos);
        result.push({
            let first_cell = &layout_data[cell.col_or_row as usize];
            let last_cell = &layout_data[cell.col_or_row as usize + cell.span as usize - 1];
            last_cell.pos + last_cell.size - first_cell.pos
        });
    }
    result
}

pub fn grid_layout_info(
    cells: Slice<GridLayoutCellData>,
    spacing: Coord,
    padding: &Padding,
) -> LayoutInfo {
    let layout_data = grid_internal::to_layout_data(cells.as_slice(), spacing, None);
    if layout_data.is_empty() {
        return Default::default();
    }
    let spacing_w = spacing * (layout_data.len() - 1) as Coord + padding.begin + padding.end;
    let min = layout_data.iter().map(|data| data.min).sum::<Coord>() + spacing_w;
    let max = layout_data.iter().map(|data| data.max).sum::<Coord>() + spacing_w;
    let preferred = layout_data.iter().map(|data| data.pref).sum::<Coord>() + spacing_w;
    let stretch = layout_data.iter().map(|data| data.stretch).sum::<Coord>();
    LayoutInfo { min, max, min_percent: 0., max_percent: 100., preferred, stretch }
}

/// Enum representing the alignment property of a BoxLayout or HorizontalLayout
#[derive(Copy, Clone, Debug, PartialEq, strum_macros::EnumString, strum_macros::Display)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum LayoutAlignment {
    stretch,
    center,
    start,
    end,
    space_between,
    space_around,
}

impl Default for LayoutAlignment {
    fn default() -> Self {
        Self::stretch
    }
}

#[repr(C)]
#[derive(Debug)]
/// The BoxLayoutData is used to represent both a Horizontal and Vertical layout.
/// The width/height x/y correspond to that of a horizontal layout.
/// For vertical layout, they are inverted
pub struct BoxLayoutData<'a> {
    pub size: Coord,
    pub spacing: Coord,
    pub padding: &'a Padding,
    pub alignment: LayoutAlignment,
    pub cells: Slice<'a, BoxLayoutCellData>,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct BoxLayoutCellData {
    pub constraint: LayoutInfo,
}

/// Solve a BoxLayout
pub fn solve_box_layout(data: &BoxLayoutData, repeater_indexes: Slice<u32>) -> SharedVector<Coord> {
    let mut layout_data: Vec<_> = data
        .cells
        .iter()
        .map(|c| {
            let min = c.constraint.min.max(c.constraint.min_percent * data.size / 100.);
            let max = c.constraint.max.min(c.constraint.max_percent * data.size / 100.);
            grid_internal::LayoutData {
                min,
                max,
                pref: c.constraint.preferred.min(max).max(min),
                stretch: c.constraint.stretch,
                ..Default::default()
            }
        })
        .collect();

    let size_without_padding = data.size - data.padding.begin - data.padding.end;
    let pref_size: Coord = layout_data.iter().map(|it| it.pref).sum();
    let num_spacings = (layout_data.len() - 1) as Coord;
    let spacings = data.spacing * num_spacings;

    let align = match data.alignment {
        LayoutAlignment::stretch => {
            grid_internal::layout_items(
                &mut layout_data,
                data.padding.begin,
                size_without_padding,
                data.spacing,
            );
            None
        }
        _ if size_without_padding <= pref_size + spacings => {
            grid_internal::layout_items(
                &mut layout_data,
                data.padding.begin,
                size_without_padding,
                data.spacing,
            );
            None
        }
        LayoutAlignment::center => Some((
            data.padding.begin + (size_without_padding - pref_size - spacings) / 2.,
            data.spacing,
        )),
        LayoutAlignment::start => Some((data.padding.begin, data.spacing)),
        LayoutAlignment::end => {
            Some((data.padding.begin + (size_without_padding - pref_size - spacings), data.spacing))
        }
        LayoutAlignment::space_between => {
            Some((data.padding.begin, (size_without_padding - pref_size) / num_spacings))
        }
        LayoutAlignment::space_around => {
            let spacing = (size_without_padding - pref_size) / (num_spacings + 1.);
            Some((data.padding.begin + spacing / 2., spacing))
        }
    };
    if let Some((mut pos, spacing)) = align {
        for it in &mut layout_data {
            it.pos = pos;
            it.size = it.pref;
            pos += spacing + it.size;
        }
    }

    let mut result = SharedVector::<f32>::default();
    result.resize(data.cells.len() * 2 + repeater_indexes.len(), 0.);
    let res = result.as_slice_mut();

    // The index/2 in result in which we should add the next repeated item
    let mut repeat_offset =
        res.len() / 2 - repeater_indexes.iter().skip(1).step_by(2).sum::<u32>() as usize;
    // The index/2  in repeater_indexes
    let mut next_rep = 0;
    // The index/2 in result in which we should add the next non-repeated item
    let mut current_offset = 0;
    for (idx, layout) in layout_data.iter().enumerate() {
        let o = loop {
            if let Some(nr) = repeater_indexes.get(next_rep * 2) {
                let nr = *nr as usize;
                if nr == idx {
                    for o in 0..2 {
                        res[current_offset * 2 + o] = (repeat_offset * 2 + o) as _;
                    }
                    current_offset += 1;
                }
                if idx >= nr {
                    if idx - nr == repeater_indexes[next_rep * 2 + 1] as usize {
                        next_rep += 1;
                        continue;
                    }
                    repeat_offset += 1;
                    break repeat_offset - 1;
                }
            }
            current_offset += 1;
            break current_offset - 1;
        };
        res[o * 2] = layout.pos;
        res[o * 2 + 1] = layout.size;
    }
    result
}

/// Return the LayoutInfo for a BoxLayout with the given cells.
pub fn box_layout_info(
    cells: Slice<BoxLayoutCellData>,
    spacing: Coord,
    padding: &Padding,
    alignment: LayoutAlignment,
) -> LayoutInfo {
    let count = cells.len();
    if count < 1 {
        return LayoutInfo { max: 0., ..LayoutInfo::default() };
    };
    let is_stretch = alignment == LayoutAlignment::stretch;
    let extra_w = padding.begin + padding.end + spacing * (count - 1) as Coord;
    let min = cells.iter().map(|c| c.constraint.min).sum::<Coord>() + extra_w;
    let max = if is_stretch {
        (cells.iter().map(|c| c.constraint.max).sum::<Coord>() + extra_w).max(min)
    } else {
        f32::MAX
    };
    let preferred = cells.iter().map(|c| c.constraint.preferred_bounded()).sum::<Coord>() + extra_w;
    let stretch = cells.iter().map(|c| c.constraint.stretch).sum::<f32>();
    LayoutInfo { min, max, min_percent: 0., max_percent: 100., preferred, stretch }
}

pub fn box_layout_info_ortho(cells: Slice<BoxLayoutCellData>, padding: &Padding) -> LayoutInfo {
    let count = cells.len();
    if count < 1 {
        return LayoutInfo { max: 0., ..LayoutInfo::default() };
    };
    let extra_w = padding.begin + padding.end;

    let mut fold =
        cells.iter().fold(LayoutInfo { stretch: f32::MAX, ..Default::default() }, |a, b| {
            a.merge(&b.constraint)
        });
    fold.max = fold.max.max(fold.min);
    fold.preferred = fold.preferred.clamp(fold.min, fold.max);
    fold.min += extra_w;
    fold.max += extra_w;
    fold.preferred += extra_w;
    fold
}

#[repr(C)]
pub struct PathLayoutData<'a> {
    pub elements: &'a crate::graphics::PathData,
    pub item_count: u32,
    pub x: Coord,
    pub y: Coord,
    pub width: Coord,
    pub height: Coord,
    pub offset: f32,
}

#[repr(C)]
#[derive(Default)]
pub struct PathLayoutItemData {
    pub width: Coord,
    pub height: Coord,
}

pub fn solve_path_layout(data: &PathLayoutData, repeater_indexes: Slice<u32>) -> SharedVector<f32> {
    use lyon_geom::*;
    use lyon_path::iterator::PathIterator;

    // Clone of path elements is cheap because it's a clone of underlying SharedVector
    let mut path_iter = data.elements.clone().iter();
    path_iter.fit(data.width, data.height, None);

    let tolerance: f32 = 0.1; // lyon::tessellation::StrokeOptions::DEFAULT_TOLERANCE

    let segment_lengths: Vec<Coord> = path_iter
        .iter()
        .bezier_segments()
        .map(|segment| match segment {
            BezierSegment::Linear(line_segment) => line_segment.length(),
            BezierSegment::Quadratic(quadratic_segment) => {
                quadratic_segment.approximate_length(tolerance)
            }
            BezierSegment::Cubic(cubic_segment) => cubic_segment.approximate_length(tolerance),
        })
        .collect();

    let path_length: Coord = segment_lengths.iter().sum();
    // the max(2) is there to put the item in the middle when there is a single item
    let item_distance = 1. / ((data.item_count - 1) as f32).max(2.);

    let mut i = 0;
    let mut next_t: f32 = data.offset;
    if data.item_count == 1 {
        next_t += item_distance;
    }

    let mut result = SharedVector::<f32>::default();
    result.resize(data.item_count as usize * 2 + repeater_indexes.len(), 0.);
    let res = result.as_slice_mut();

    // The index/2 in result in which we should add the next repeated item
    let mut repeat_offset =
        res.len() / 2 - repeater_indexes.iter().skip(1).step_by(2).sum::<u32>() as usize;
    // The index/2  in repeater_indexes
    let mut next_rep = 0;
    // The index/2 in result in which we should add the next non-repeated item
    let mut current_offset = 0;

    'main_loop: while i < data.item_count {
        let mut current_length: f32 = 0.;
        next_t %= 1.;

        for (seg_idx, segment) in path_iter.iter().bezier_segments().enumerate() {
            let seg_len = segment_lengths[seg_idx];
            let seg_start = current_length;
            current_length += seg_len;

            let seg_end_t = (seg_start + seg_len) / path_length;

            while next_t <= seg_end_t {
                let local_t = ((next_t * path_length) - seg_start) / seg_len;

                let item_pos = segment.sample(local_t);

                let o = loop {
                    if let Some(nr) = repeater_indexes.get(next_rep * 2) {
                        let nr = *nr;
                        if nr == i {
                            for o in 0..4 {
                                res[current_offset * 4 + o] = (repeat_offset * 4 + o) as _;
                            }
                            current_offset += 1;
                        }
                        if i >= nr {
                            if i - nr == repeater_indexes[next_rep * 2 + 1] {
                                next_rep += 1;
                                continue;
                            }
                            repeat_offset += 1;
                            break repeat_offset - 1;
                        }
                    }
                    current_offset += 1;
                    break current_offset - 1;
                };

                res[o * 2] = item_pos.x + data.x;
                res[o * 2 + 1] = item_pos.y + data.y;
                i += 1;
                next_t += item_distance;
                if i >= data.item_count {
                    break 'main_loop;
                }
            }

            if next_t > 1. {
                break;
            }
        }
    }

    result
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    use super::*;

    #[no_mangle]
    pub extern "C" fn sixtyfps_solve_grid_layout(
        data: &GridLayoutData,
        result: &mut SharedVector<Coord>,
    ) {
        *result = super::solve_grid_layout(data)
    }

    #[no_mangle]
    pub extern "C" fn sixtyfps_grid_layout_info(
        cells: Slice<GridLayoutCellData>,
        spacing: Coord,
        padding: &Padding,
    ) -> LayoutInfo {
        super::grid_layout_info(cells, spacing, padding)
    }

    #[no_mangle]
    pub extern "C" fn sixtyfps_solve_box_layout(
        data: &BoxLayoutData,
        repeater_indexes: Slice<u32>,
        result: &mut SharedVector<Coord>,
    ) {
        *result = super::solve_box_layout(data, repeater_indexes)
    }

    #[no_mangle]
    /// Return the LayoutInfo for a BoxLayout with the given cells.
    pub extern "C" fn sixtyfps_box_layout_info(
        cells: Slice<BoxLayoutCellData>,
        spacing: Coord,
        padding: &Padding,
        alignment: LayoutAlignment,
    ) -> LayoutInfo {
        super::box_layout_info(cells, spacing, padding, alignment)
    }

    #[no_mangle]
    /// Return the LayoutInfo for a BoxLayout with the given cells.
    pub extern "C" fn sixtyfps_box_layout_info_ortho(
        cells: Slice<BoxLayoutCellData>,
        padding: &Padding,
    ) -> LayoutInfo {
        super::box_layout_info_ortho(cells, padding)
    }

    #[no_mangle]
    pub extern "C" fn sixtyfps_solve_path_layout(
        data: &PathLayoutData,
        repeater_indexes: Slice<u32>,
        result: &mut SharedVector<Coord>,
    ) {
        *result = super::solve_path_layout(data, repeater_indexes)
    }
}
