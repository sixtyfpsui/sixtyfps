/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::dynamic_component::InstanceRef;
use crate::eval::{self, ComponentInstance, EvalLocalContext};
use crate::Value;
use sixtyfps_compilerlib::expression_tree::Expression;
use sixtyfps_compilerlib::langtype::Type;
use sixtyfps_compilerlib::layout::{Layout, LayoutConstraints, LayoutGeometry, Orientation};
use sixtyfps_compilerlib::namedreference::NamedReference;
use sixtyfps_compilerlib::object_tree::ElementRc;
use sixtyfps_corelib::layout::{self as core_layout};
use sixtyfps_corelib::model::RepeatedComponent;
use sixtyfps_corelib::slice::Slice;
use sixtyfps_corelib::window::ComponentWindow;
use std::convert::TryInto;

pub(crate) fn to_runtime(o: Orientation) -> core_layout::Orientation {
    match o {
        Orientation::Horizontal => core_layout::Orientation::Horizontal,
        Orientation::Vertical => core_layout::Orientation::Vertical,
    }
}

pub(crate) fn from_runtime(o: core_layout::Orientation) -> Orientation {
    match o {
        core_layout::Orientation::Horizontal => Orientation::Horizontal,
        core_layout::Orientation::Vertical => Orientation::Vertical,
    }
}

pub(crate) fn compute_layout_info(
    lay: &Layout,
    orientation: Orientation,
    local_context: &mut EvalLocalContext,
) -> Value {
    let component = match local_context.component_instance {
        ComponentInstance::InstanceRef(c) => c,
        ComponentInstance::GlobalComponent(_) => panic!("Cannot compute layout from a Global"),
    };
    let expr_eval = |nr: &NamedReference| -> f32 {
        eval::load_property(component, &nr.element(), nr.name()).unwrap().try_into().unwrap()
    };
    match lay {
        Layout::GridLayout(grid_layout) => {
            let cells = grid_layout_data(grid_layout, orientation, component, &expr_eval);
            let (padding, spacing) =
                padding_and_spacing(&grid_layout.geometry, orientation, &expr_eval);
            core_layout::grid_layout_info(Slice::from(cells.as_slice()), spacing, &padding).into()
        }
        Layout::BoxLayout(box_layout) => {
            let (cells, alignment) =
                box_layout_data(box_layout, orientation, component, &expr_eval, None);
            let (padding, spacing) =
                padding_and_spacing(&box_layout.geometry, orientation, &expr_eval);
            if orientation == box_layout.orientation {
                core_layout::box_layout_info(
                    Slice::from(cells.as_slice()),
                    spacing,
                    &padding,
                    alignment,
                )
            } else {
                core_layout::box_layout_info_ortho(Slice::from(cells.as_slice()), &padding)
            }
            .into()
        }
        Layout::PathLayout(_) => unimplemented!(),
    }
}

pub(crate) fn solve_layout(
    lay: &Layout,
    orientation: Orientation,
    local_context: &mut EvalLocalContext,
) -> Value {
    let component = match local_context.component_instance {
        ComponentInstance::InstanceRef(c) => c,
        ComponentInstance::GlobalComponent(_) => panic!("Cannot compute layout from a Global"),
    };
    let expr_eval = |nr: &NamedReference| -> f32 {
        eval::load_property(component, &nr.element(), nr.name()).unwrap().try_into().unwrap()
    };

    match lay {
        Layout::GridLayout(grid_layout) => {
            let cells = grid_layout_data(grid_layout, orientation, component, &expr_eval);
            let (padding, spacing) =
                padding_and_spacing(&grid_layout.geometry, orientation, &expr_eval);

            let size_ref = grid_layout.geometry.rect.size_reference(orientation);
            core_layout::solve_grid_layout(&core_layout::GridLayoutData {
                size: size_ref.map(expr_eval).unwrap_or(0.),
                spacing,
                padding: &padding,
                cells: Slice::from(cells.as_slice()),
            })
            .into()
        }
        Layout::BoxLayout(box_layout) => {
            let mut repeated_indices = Vec::new();
            let (cells, alignment) = box_layout_data(
                box_layout,
                orientation,
                component,
                &expr_eval,
                Some(&mut repeated_indices),
            );
            let (padding, spacing) =
                padding_and_spacing(&box_layout.geometry, orientation, &expr_eval);
            let size_ref = match orientation {
                Orientation::Horizontal => &box_layout.geometry.rect.width_reference,
                Orientation::Vertical => &box_layout.geometry.rect.height_reference,
            };
            core_layout::solve_box_layout(
                &core_layout::BoxLayoutData {
                    size: size_ref.as_ref().map(expr_eval).unwrap_or(0.),
                    spacing,
                    padding: &padding,
                    alignment,
                    cells: Slice::from(cells.as_slice()),
                },
                Slice::from(repeated_indices.as_slice()),
            )
            .into()
        }
        Layout::PathLayout(path_layout) => {
            let repeated_indices = repeater_indices(&path_layout.elements, component);
            core_layout::solve_path_layout(
                &core_layout::PathLayoutData {
                    width: path_layout.rect.width_reference.as_ref().map(expr_eval).unwrap_or(0.),
                    height: path_layout.rect.height_reference.as_ref().map(expr_eval).unwrap_or(0.),
                    x: 0.,
                    y: 0.,
                    elements: &eval::eval_expression(
                        &Expression::PathElements { elements: path_layout.path.clone() },
                        local_context,
                    )
                    .try_into()
                    .unwrap(),
                    offset: path_layout.offset_reference.as_ref().map_or(0., expr_eval),
                    item_count: path_layout.elements.len() as u32,
                },
                Slice::from(repeated_indices.as_slice()),
            )
            .into()
        }
    }
}

fn padding_and_spacing(
    layout_geometry: &LayoutGeometry,
    orientation: Orientation,
    expr_eval: &impl Fn(&NamedReference) -> f32,
) -> (core_layout::Padding, f32) {
    let spacing = layout_geometry.spacing.as_ref().map_or(0., expr_eval);
    let (begin, end) = layout_geometry.padding.begin_end(orientation);
    let padding =
        core_layout::Padding { begin: begin.map_or(0., expr_eval), end: end.map_or(0., expr_eval) };
    (padding, spacing)
}

/// return the celldata, the padding, and the spacing of a grid layout
fn grid_layout_data(
    grid_layout: &sixtyfps_compilerlib::layout::GridLayout,
    orientation: Orientation,
    component: InstanceRef,
    expr_eval: &impl Fn(&NamedReference) -> f32,
) -> Vec<core_layout::GridLayoutCellData> {
    let cells = grid_layout
        .elems
        .iter()
        .map(|cell| {
            let mut layout_info = get_layout_info(
                &cell.item.element,
                component,
                &eval::window_ref(component).unwrap(),
                orientation,
            );
            fill_layout_info_constraints(
                &mut layout_info,
                &cell.item.constraints,
                orientation,
                &expr_eval,
            );
            let (col_or_row, span) = cell.col_or_row_and_span(orientation);
            core_layout::GridLayoutCellData { col_or_row, span, constraint: layout_info }
        })
        .collect::<Vec<_>>();
    cells
}

fn box_layout_data(
    box_layout: &sixtyfps_compilerlib::layout::BoxLayout,
    orientation: Orientation,
    component: InstanceRef,
    expr_eval: &impl Fn(&NamedReference) -> f32,
    mut repeater_indices: Option<&mut Vec<u32>>,
) -> (Vec<core_layout::BoxLayoutCellData>, core_layout::LayoutAlignment) {
    let window = eval::window_ref(component).unwrap();
    let mut cells = Vec::with_capacity(box_layout.elems.len());
    for cell in &box_layout.elems {
        if cell.element.borrow().repeated.is_some() {
            generativity::make_guard!(guard);
            let rep = crate::dynamic_component::get_repeater_by_name(
                component,
                cell.element.borrow().id.as_str(),
                guard,
            );
            rep.0.as_ref().ensure_updated(|| {
                let instance = crate::dynamic_component::instantiate(
                    rep.1.clone(),
                    Some(component.borrow()),
                    Some(window.clone()),
                );
                instance.run_setup_code();
                instance
            });
            let component_vec = rep.0.as_ref().components_vec();
            if let Some(ri) = repeater_indices.as_mut() {
                ri.push(cells.len() as _);
                ri.push(component_vec.len() as _);
            }
            cells.extend(
                component_vec
                    .iter()
                    .map(|x| x.as_pin_ref().box_layout_data(to_runtime(orientation))),
            );
        } else {
            let mut layout_info = get_layout_info(&cell.element, component, &window, orientation);
            fill_layout_info_constraints(
                &mut layout_info,
                &cell.constraints,
                orientation,
                &expr_eval,
            );
            cells.push(core_layout::BoxLayoutCellData { constraint: layout_info });
        }
    }
    let alignment = box_layout
        .geometry
        .alignment
        .as_ref()
        .map(|nr| {
            eval::load_property(component, &nr.element(), nr.name())
                .unwrap()
                .try_into()
                .unwrap_or_default()
        })
        .unwrap_or_default();
    (cells, alignment)
}

fn repeater_indices(children: &[ElementRc], component: InstanceRef) -> Vec<u32> {
    let window = eval::window_ref(component).unwrap();

    let mut idx = 0;
    let mut ri = Vec::new();
    for e in children {
        if e.borrow().repeated.is_some() {
            generativity::make_guard!(guard);
            let rep = crate::dynamic_component::get_repeater_by_name(
                component,
                e.borrow().id.as_str(),
                guard,
            );
            rep.0.as_ref().ensure_updated(|| {
                let instance = crate::dynamic_component::instantiate(
                    rep.1.clone(),
                    Some(component.borrow()),
                    Some(window.clone()),
                );
                instance.run_setup_code();
                instance
            });
            let component_vec = rep.0.as_ref().components_vec();
            ri.push(idx);
            ri.push(component_vec.len() as _);
            idx += component_vec.len() as u32;
        } else {
            idx += 1;
        }
    }
    ri
}

pub(crate) fn fill_layout_info_constraints(
    layout_info: &mut core_layout::LayoutInfo,
    constraints: &LayoutConstraints,
    orientation: Orientation,
    expr_eval: &impl Fn(&NamedReference) -> f32,
) {
    let is_percent =
        |nr: &NamedReference| Expression::PropertyReference(nr.clone()).ty() == Type::Percent;

    match orientation {
        Orientation::Horizontal => {
            constraints.min_width.as_ref().map(|e| {
                if !is_percent(e) {
                    layout_info.min = expr_eval(e)
                } else {
                    layout_info.min_percent = expr_eval(e)
                }
            });
            constraints.max_width.as_ref().map(|e| {
                if !is_percent(e) {
                    layout_info.max = expr_eval(e)
                } else {
                    layout_info.max_percent = expr_eval(e)
                }
            });
            constraints.preferred_width.as_ref().map(|e| {
                layout_info.preferred = expr_eval(e);
            });
            constraints.horizontal_stretch.as_ref().map(|e| layout_info.stretch = expr_eval(e));
        }
        Orientation::Vertical => {
            constraints.min_height.as_ref().map(|e| {
                if !is_percent(e) {
                    layout_info.min = expr_eval(e)
                } else {
                    layout_info.min_percent = expr_eval(e)
                }
            });
            constraints.max_height.as_ref().map(|e| {
                if !is_percent(e) {
                    layout_info.max = expr_eval(e)
                } else {
                    layout_info.max_percent = expr_eval(e)
                }
            });
            constraints.preferred_height.as_ref().map(|e| {
                layout_info.preferred = expr_eval(e);
            });
            constraints.vertical_stretch.as_ref().map(|e| layout_info.stretch = expr_eval(e));
        }
    }
}

/// Get the layout info for an element based on the layout_info_prop or the builtin item layouting_info
pub(crate) fn get_layout_info(
    elem: &ElementRc,
    component: InstanceRef,
    window: &ComponentWindow,
    orientation: Orientation,
) -> core_layout::LayoutInfo {
    let elem = elem.borrow();
    if let Some(nr) = elem.layout_info_prop(orientation) {
        eval::load_property(component, &nr.element(), nr.name()).unwrap().try_into().unwrap()
    } else {
        let item = &component
            .component_type
            .items
            .get(elem.id.as_str())
            .unwrap_or_else(|| panic!("Internal error: Item {} not found", elem.id));
        unsafe {
            item.item_from_component(component.as_ptr())
                .as_ref()
                .layouting_info(to_runtime(orientation), window)
        }
    }
}
