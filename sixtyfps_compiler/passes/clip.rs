/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! Pass that lowers synthetic `clip` properties to Clip element

use std::cell::RefCell;
use std::rc::Rc;

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{BindingExpression, Expression, NamedReference};
use crate::langtype::{NativeClass, Type};
use crate::object_tree::{Component, Element, ElementRc};
use crate::typeregister::TypeRegister;

pub fn handle_clip(
    component: &Rc<Component>,
    type_register: &TypeRegister,
    diag: &mut BuildDiagnostics,
) {
    let native_clip = type_register.lookup("Clip").as_builtin().native_class.clone();

    crate::object_tree::recurse_elem_including_sub_components(
        component,
        &(),
        &mut |elem_rc: &ElementRc, _| {
            let mut elem = elem_rc.borrow_mut();
            if let Some(clip_prop) = elem.bindings.remove("clip") {
                match elem.builtin_type().as_ref().map(|ty| ty.name.as_str()) {
                    Some("Rectangle") => {}
                    Some("Path") => {
                        // it's an actual property, so keep the binding
                        elem.bindings.insert("clip".into(), clip_prop);
                        return;
                    }
                    _ => {
                        diag.push_error(
                            "The 'clip' property can only be applied to a Rectangle or a Path for now"
                                .into(),
                            &clip_prop.span,
                        );
                        return;
                    }
                }
                // Was added by the materialize_fake_properties pass
                elem.property_declarations.remove("clip");
                match &clip_prop.expression {
                    Expression::BoolLiteral(false) => {}
                    Expression::BoolLiteral(true) => {
                        drop(elem);
                        create_clip_element(elem_rc, &native_clip);
                    }
                    _ => diag.push_error(
                        "The 'clip' property can only be a boolean literal (true or false) for now"
                            .into(),
                        &clip_prop.span,
                    ),
                }
            }
        },
    )
}

fn create_clip_element(parent_elem: &ElementRc, native_clip: &Rc<NativeClass>) {
    let mut parent = parent_elem.borrow_mut();
    let clip = Rc::new(RefCell::new(Element {
        id: format!("{}_clip", parent.id),
        base_type: Type::Native(native_clip.clone()),
        children: std::mem::take(&mut parent.children),
        enclosing_component: parent.enclosing_component.clone(),
        ..Element::default()
    }));

    parent.children.push(clip.clone());
    drop(parent); // NamedReference::new will borrow() the parent, so we can't hold a mutable ref
    clip.borrow_mut().bindings = ["width", "height"]
        .iter()
        .map(|prop| -> (String, BindingExpression) {
            (
                (*prop).to_owned(),
                Expression::PropertyReference(NamedReference::new(parent_elem, prop)).into(),
            )
        })
        .collect();
    for optional_binding in ["border_radius", "border_width"].iter() {
        if parent_elem.borrow().bindings.contains_key(*optional_binding) {
            clip.borrow_mut().bindings.insert(
                optional_binding.to_string(),
                Expression::PropertyReference(NamedReference::new(parent_elem, optional_binding))
                    .into(),
            );
        }
    }
}
