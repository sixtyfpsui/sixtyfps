/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! This pass creates properties that are used but are otherwise not real.
//!
//! Must be run after lower_layout and default_geometry passes

use crate::diagnostics::Spanned;
use crate::expression_tree::{BindingExpression, BuiltinFunction, Expression, Unit};
use crate::langtype::Type;
use crate::layout::Orientation;
use crate::object_tree::*;
use std::collections::BTreeMap;
use std::rc::Rc;

pub fn materialize_fake_properties(component: &Rc<Component>) {
    let mut to_initialize = std::collections::HashSet::new();

    recurse_elem_including_sub_components_no_borrow(component, &(), &mut |elem, _| {
        visit_all_named_references_in_element(elem, |nr| {
            let elem = nr.element();
            let mut elem = elem.borrow_mut();
            let elem = &mut *elem;
            if maybe_materialize(&mut elem.property_declarations, &elem.base_type, nr.name())
                && !elem.bindings.contains_key(nr.name())
            {
                to_initialize.insert(nr.clone());
            }
        });
        let mut elem = elem.borrow_mut();
        let elem = &mut *elem;
        for prop in elem.bindings.keys() {
            maybe_materialize(&mut elem.property_declarations, &elem.base_type, prop);
        }
    });

    for nr in to_initialize {
        let elem = nr.element();
        if !elem.borrow().bindings.contains_key(nr.name()) {
            initialize(elem, nr.name())
        }
    }
}

fn maybe_materialize(
    property_declarations: &mut BTreeMap<String, PropertyDeclaration>,
    base_type: &Type,
    prop: &str,
) -> bool {
    if property_declarations.contains_key(prop) {
        return false;
    }
    let has_declared_property = match &base_type {
        Type::Component(c) => has_declared_property(&c.root_element.borrow(), prop),
        Type::Builtin(b) => b.properties.contains_key(prop),
        Type::Native(n) => {
            n.lookup_property(prop).map_or(false, |prop_type| prop_type.is_property_type())
        }
        _ => false,
    };

    if !has_declared_property {
        let ty = crate::typeregister::reserved_property(prop).property_type;
        if ty != Type::Invalid {
            property_declarations.insert(
                prop.to_owned(),
                PropertyDeclaration { property_type: ty, ..PropertyDeclaration::default() },
            );
            return true;
        }
    }
    false
}

/// Returns true if the property is declared in this element or parent
/// (as opposed to being implicitly declared)
fn has_declared_property(elem: &Element, prop: &str) -> bool {
    if elem.property_declarations.contains_key(prop) {
        return true;
    }
    match &elem.base_type {
        Type::Component(c) => has_declared_property(&c.root_element.borrow(), prop),
        Type::Builtin(b) => b.properties.contains_key(prop),
        Type::Native(n) => n.lookup_property(prop).is_some(),
        _ => false,
    }
}

/// Initialize a sensible default binding for the now materialized property
fn initialize(elem: ElementRc, name: &str) {
    let expr = match name {
        "min_height" => layout_constraint_prop(&elem, "min", Orientation::Vertical),
        "min_width" => layout_constraint_prop(&elem, "min", Orientation::Horizontal),
        "max_height" => layout_constraint_prop(&elem, "max", Orientation::Vertical),
        "max_width" => layout_constraint_prop(&elem, "max", Orientation::Horizontal),
        "preferred_height" => layout_constraint_prop(&elem, "preferred", Orientation::Vertical),
        "preferred_width" => layout_constraint_prop(&elem, "preferred", Orientation::Horizontal),
        "horizontal_stretch" => layout_constraint_prop(&elem, "stretch", Orientation::Horizontal),
        "vertical_stretch" => layout_constraint_prop(&elem, "stretch", Orientation::Vertical),
        "opacity" => Expression::NumberLiteral(1., Unit::None),
        _ => return,
    };
    let b = BindingExpression::new_with_span(expr, elem.borrow().to_source_location());
    elem.borrow_mut().bindings.insert(name.into(), b);
}

fn layout_constraint_prop(elem: &ElementRc, field: &str, orient: Orientation) -> Expression {
    let expr = match elem.borrow().layout_info_prop(orient) {
        Some(e) => Expression::PropertyReference(e.clone()),
        None => Expression::FunctionCall {
            function: Box::new(Expression::BuiltinFunctionReference(
                BuiltinFunction::ImplicitLayoutInfo(orient),
                None,
            )),
            arguments: vec![Expression::ElementReference(Rc::downgrade(elem))],
            source_location: None,
        },
    };
    Expression::StructFieldAccess { base: expr.into(), name: field.into() }
}
