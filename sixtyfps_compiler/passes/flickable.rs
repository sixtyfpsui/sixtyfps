/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Flickable pass
//!
//! The Flickable element is special in the sense that it has a viewport
//! which is not exposed. This passes create the viewport and fixes all property access
//!
//! It will also initialize proper geometry
//! This pass must be called before the materialize_fake_properties as it going to be generate
//! binding reference to fake properties

use std::cell::RefCell;
use std::rc::Rc;

use itertools::Itertools;

use crate::expression_tree::{Expression, NamedReference};
use crate::langtype::{NativeClass, Type};
use crate::object_tree::{Component, Element, ElementRc};
use crate::typeregister::TypeRegister;

pub fn handle_flickable(root_component: &Rc<Component>, tr: &TypeRegister) {
    let mut native_rect = tr.lookup("Rectangle").as_builtin().native_class.clone();
    while let Some(p) = native_rect.parent.clone() {
        native_rect = p;
    }

    crate::object_tree::recurse_elem_including_sub_components(
        root_component,
        &(),
        &mut |elem: &ElementRc, _| {
            if !matches!(&elem.borrow().base_type, Type::Builtin(n) if n.name == "Flickable") {
                return;
            }

            fixup_geometry(elem);
            create_viewport_element(elem, &native_rect);
        },
    )
}

fn create_viewport_element(flickable_elem: &ElementRc, native_rect: &Rc<NativeClass>) {
    let mut flickable = flickable_elem.borrow_mut();
    let flickable = &mut *flickable;
    let viewport = Rc::new(RefCell::new(Element {
        id: format!("{}_viewport", flickable.id),
        base_type: Type::Native(native_rect.clone()),
        children: std::mem::take(&mut flickable.children),
        enclosing_component: flickable.enclosing_component.clone(),
        is_flickable_viewport: true,
        ..Element::default()
    }));
    for (prop, info) in &flickable.base_type.as_builtin().properties {
        if let Some(vp_prop) = prop.strip_prefix("viewport_") {
            let nr = NamedReference::new(&viewport, vp_prop);
            flickable.property_declarations.insert(prop.to_owned(), info.ty.clone().into());
            match flickable.bindings.entry(prop.to_owned()) {
                std::collections::btree_map::Entry::Occupied(entry) => {
                    let entry = entry.into_mut();
                    entry.expression = Expression::TwoWayBinding(
                        nr,
                        Some(Box::new(std::mem::take(&mut entry.expression))),
                    )
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(Expression::TwoWayBinding(nr, None).into());
                }
            }
            // Workaround for https://github.com/sixtyfpsui/sixtyfps/issues/193
            if let Some(a) = flickable.property_animations.remove(prop) {
                viewport.borrow_mut().property_animations.insert(vp_prop.into(), a);
            }
        }
    }
    flickable.children.push(viewport);
}

fn fixup_geometry(flickable_elem: &ElementRc) {
    let forward_minmax_of = |prop: &str, op: char| {
        set_binding_if_not_explicit(flickable_elem, prop, || {
            flickable_elem
                .borrow()
                .children
                .iter()
                .filter(|x| is_layout(&x.borrow().base_type))
                .map(|x| Expression::PropertyReference(NamedReference::new(x, prop)))
                .fold1(|lhs, rhs| crate::builtin_macros::min_max_expression(lhs, rhs, op))
        })
    };

    if !flickable_elem.borrow().bindings.contains_key("height") {
        forward_minmax_of("max_height", '<');
        forward_minmax_of("preferred_height", '<');
    }
    if !flickable_elem.borrow().bindings.contains_key("width") {
        forward_minmax_of("max_width", '<');
        forward_minmax_of("preferred_width", '<');
    }
    set_binding_if_not_explicit(flickable_elem, "viewport_width", || {
        Some(
            flickable_elem
                .borrow()
                .children
                .iter()
                .filter(|x| is_layout(&x.borrow().base_type))
                .map(|x| Expression::PropertyReference(NamedReference::new(x, "min_width")))
                .fold(
                    Expression::PropertyReference(NamedReference::new(flickable_elem, "width")),
                    |lhs, rhs| crate::builtin_macros::min_max_expression(lhs, rhs, '>'),
                ),
        )
    });
    set_binding_if_not_explicit(flickable_elem, "viewport_height", || {
        Some(
            flickable_elem
                .borrow()
                .children
                .iter()
                .filter(|x| is_layout(&x.borrow().base_type))
                .map(|x| Expression::PropertyReference(NamedReference::new(x, "min_height")))
                .fold(
                    Expression::PropertyReference(NamedReference::new(flickable_elem, "height")),
                    |lhs, rhs| crate::builtin_macros::min_max_expression(lhs, rhs, '>'),
                ),
        )
    });
}

/// Return true if this type is a layout that has constraints
fn is_layout(base_type: &Type) -> bool {
    if let Type::Builtin(be) = base_type {
        match be.name.as_str() {
            "GridLayout" | "HorizontalLayout" | "VerticalLayout" => true,
            "PathLayout" => false,
            _ => false,
        }
    } else {
        false
    }
}

/// Set the property binding on the given element to the given expression (computed lazily).
/// The parameter to the lazily calculation is the element's children
pub fn set_binding_if_not_explicit(
    elem: &ElementRc,
    property: &str,
    expression: impl FnOnce() -> Option<Expression>,
) {
    if !elem.borrow().bindings.contains_key(property) {
        if let Some(e) = expression() {
            elem.borrow_mut().bindings.insert(property.into(), e.into());
        }
    }
}
