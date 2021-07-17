/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
Make sure that the Repeated expression are just components without any children
 */

use crate::expression_tree::NamedReference;
use crate::langtype::Type;
use crate::object_tree::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn process_repeater_components(component: &Rc<Component>) {
    create_repeater_components(component);
    adjust_references(component);
}

fn create_repeater_components(component: &Rc<Component>) {
    recurse_elem(&component.root_element, &(), &mut |elem, _| {
        if elem.borrow().repeated.is_none() {
            return;
        }
        let parent_element = Rc::downgrade(elem);
        let mut elem = elem.borrow_mut();

        let comp = Rc::new(Component {
            root_element: Rc::new(RefCell::new(Element {
                id: elem.id.clone(),
                base_type: std::mem::take(&mut elem.base_type),
                bindings: std::mem::take(&mut elem.bindings),
                property_analysis: std::mem::take(&mut elem.property_analysis),
                children: std::mem::take(&mut elem.children),
                property_declarations: std::mem::take(&mut elem.property_declarations),
                property_animations: std::mem::take(&mut elem.property_animations),
                named_references: Default::default(),
                repeated: None,
                node: elem.node.clone(),
                enclosing_component: Default::default(),
                states: std::mem::take(&mut elem.states),
                transitions: std::mem::take(&mut elem.transitions),
                child_of_layout: elem.child_of_layout,
                layout_info_prop: elem.layout_info_prop.take(),
                is_flickable_viewport: elem.is_flickable_viewport,
                item_index: Default::default(), // Not determined yet
            })),
            parent_element,
            ..Component::default()
        });

        let weak = Rc::downgrade(&comp);
        recurse_elem(&comp.root_element, &(), &mut |e, _| {
            e.borrow_mut().enclosing_component = weak.clone()
        });
        create_repeater_components(&comp);
        elem.base_type = Type::Component(comp);
    });
}

/// Make sure that references to property within the repeated element actually point to the reference
/// to the root of the newly created component
fn adjust_references(comp: &Rc<Component>) {
    visit_all_named_references(comp, &mut |nr| {
        if nr.name() == "$model" {
            return;
        }
        let e = nr.element();
        if e.borrow().repeated.is_some() {
            if let Type::Component(c) = e.borrow().base_type.clone() {
                *nr = NamedReference::new(&c.root_element, nr.name())
            };
        }
    });
}
