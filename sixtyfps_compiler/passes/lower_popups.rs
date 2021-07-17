/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Passe that compute the layout constraint

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{Expression, NamedReference};
use crate::langtype::Type;
use crate::object_tree::*;
use crate::typeregister::TypeRegister;
use std::rc::Rc;

/// Currently this just removes the layout from the tree
pub fn lower_popups(
    component: &Rc<Component>,
    type_register: &TypeRegister,
    diag: &mut BuildDiagnostics,
) {
    let window_type = Type::Native(
        type_register.lookup_element("Window").unwrap().as_builtin().native_class.clone(),
    );

    recurse_elem_including_sub_components_no_borrow(
        component,
        &Vec::new(),
        &mut |elem, parent_stack: &Vec<ElementRc>| {
            let is_popup = elem.borrow().base_type.to_string() == "PopupWindow";
            if is_popup {
                lower_popup_window(elem, parent_stack, &window_type, diag);
            }
            // this could be implemented in a better way with less cloning of the state
            let mut parent_stack = parent_stack.clone();
            parent_stack.push(elem.clone());
            parent_stack
        },
    )
}

fn lower_popup_window(
    popup_window_element: &ElementRc,
    parent_stack: &[ElementRc],
    window_type: &Type,
    diag: &mut BuildDiagnostics,
) {
    let parent_element = match parent_stack.last() {
        None => {
            diag.push_error(
                "PopupWindow cannot be the top level".into(),
                &*popup_window_element.borrow(),
            );
            return;
        }
        Some(parent_element) => parent_element,
    };

    let parent_component = parent_element.borrow().enclosing_component.upgrade().unwrap();

    // Remove the popup_window_element from its parent
    parent_element.borrow_mut().children.retain(|child| !Rc::ptr_eq(child, popup_window_element));

    popup_window_element.borrow_mut().base_type = window_type.clone();

    let popup_comp = Rc::new(Component {
        root_element: popup_window_element.clone(),
        parent_element: Rc::downgrade(parent_element),
        ..Component::default()
    });

    let weak = Rc::downgrade(&popup_comp);
    recurse_elem(&popup_comp.root_element, &(), &mut |e, _| {
        e.borrow_mut().enclosing_component = weak.clone()
    });

    // Generate a x and y property, relative to the window coordinate
    // FIXME: this is a hack that doesn't always work, perhaps should we store an item ref or something
    let coord_x = create_coordinate(&popup_comp, parent_stack, "x");
    let coord_y = create_coordinate(&popup_comp, parent_stack, "y");

    // Throw error when accessing the popup from outside
    // FIXME:
    // - the span is the span of the PopupWindow, that's wrong, we should have the span of the reference
    // - There are other object reference than in the NamedReference
    // - Maybe this should actually be allowed
    visit_all_named_references(&parent_component, &mut |nr| {
        if std::rc::Weak::ptr_eq(&nr.element().borrow().enclosing_component, &weak) {
            diag.push_error(
                "Cannot access the inside of a PopupWindow from enclosing component".into(),
                &*popup_window_element.borrow(),
            );
        }
    });

    parent_component.popup_windows.borrow_mut().push(PopupWindow {
        component: popup_comp,
        x: coord_x,
        y: coord_y,
    });
}

fn create_coordinate(
    popup_comp: &Rc<Component>,
    parent_stack: &[ElementRc],
    coord: &str,
) -> NamedReference {
    let mut expression = popup_comp
        .root_element
        .borrow()
        .bindings
        .get(coord)
        .map(|e| e.expression.clone())
        .unwrap_or(Expression::NumberLiteral(0., crate::expression_tree::Unit::Phx));

    for parent in parent_stack {
        expression = Expression::BinaryExpression {
            lhs: Box::new(expression),
            rhs: Box::new(Expression::PropertyReference(NamedReference::new(parent, coord))),
            op: '+',
        };
    }
    let parent_element = parent_stack.last().unwrap();
    let property_name = format!("{}_popup_{}", popup_comp.root_element.borrow().id, coord);
    parent_element
        .borrow_mut()
        .property_declarations
        .insert(property_name.clone(), Type::LogicalLength.into());
    parent_element.borrow_mut().bindings.insert(property_name.clone(), expression.into());
    NamedReference::new(parent_element, &property_name)
}
