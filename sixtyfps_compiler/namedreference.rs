/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This module contains the [`NamedReference`] and its helper
*/

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::{Rc, Weak};

use crate::langtype::Type;
use crate::object_tree::{Element, ElementRc};

/// Reference to a property or callback of a given name within an element.
#[derive(Clone)]
pub struct NamedReference(Rc<NamedReferenceInner>);

pub fn pretty_print_element_ref(
    f: &mut dyn std::fmt::Write,
    element: &Weak<RefCell<Element>>,
) -> std::fmt::Result {
    match element.upgrade() {
        Some(e) => match e.try_borrow() {
            Ok(el) => write!(f, "{}", el.id),
            Err(_) => write!(f, "<borrowed>"),
        },
        None => write!(f, "<null>"),
    }
}

impl std::fmt::Debug for NamedReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        pretty_print_element_ref(f, &self.0.element)?;
        write!(f, ".{}", self.0.name)
    }
}

impl NamedReference {
    pub fn new(element: &ElementRc, name: &str) -> Self {
        Self(NamedReferenceInner::from_name(element, name))
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn element(&self) -> ElementRc {
        self.0.element.upgrade().unwrap()
    }
    pub fn ty(&self) -> Type {
        self.element().borrow().lookup_property(self.name()).property_type
    }

    /// return true if the property has a constant value for the lifetime of the program
    pub fn is_constant(&self) -> bool {
        self.is_constant_impl(true)
    }

    /// return true if we know that this property is changed by other means than its own binding
    pub fn is_externally_modified(&self) -> bool {
        !self.is_constant_impl(false)
    }

    /// return true if the property has a constant value for the lifetime of the program
    fn is_constant_impl(&self, mut check_binding: bool) -> bool {
        let mut elem = self.element();
        let e = elem.borrow();
        if let Some(decl) = e.property_declarations.get(self.name()) {
            if decl.expose_in_public_api {
                // could be set by the public API
                return false;
            }
        }
        drop(e);

        loop {
            let e = elem.borrow();
            if e.property_analysis.borrow().get(self.name()).map_or(false, |a| a.is_set) {
                // if the property is set somewhere, it is not constant
                return false;
            }

            if check_binding {
                if let Some(b) = e.bindings.get(self.name()) {
                    if !b.analysis.borrow().as_ref().map_or(false, |a| a.is_const) {
                        return false;
                    }
                    check_binding = false;
                }
            }
            if e.property_declarations.contains_key(self.name()) {
                return true;
            }
            match &e.base_type {
                Type::Native(_) => return false, // after resolving we don't know anymore if the property can be changed natively
                Type::Component(c) => {
                    let next = c.root_element.clone();
                    drop(e);
                    elem = next;
                    continue;
                }
                Type::Builtin(b) => {
                    return b.properties.get(self.name()).map_or(true, |pi| !pi.is_native_output)
                }
                _ => return true,
            }
        }
    }
}

impl Eq for NamedReference {}

impl PartialEq for NamedReference {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Hash for NamedReference {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

struct NamedReferenceInner {
    /// The element.
    element: Weak<RefCell<Element>>,
    /// The property name
    name: String,
}

impl NamedReferenceInner {
    fn check_invariant(&self) {
        debug_assert!(std::ptr::eq(
            self as *const Self,
            Rc::as_ptr(
                &self.element.upgrade().unwrap().borrow().named_references.0.borrow()[&self.name]
            )
        ))
    }

    pub fn from_name(element: &ElementRc, name: &str) -> Rc<Self> {
        let elem = element.borrow();
        let mut named_references = elem.named_references.0.borrow_mut();
        let result = if let Some(r) = named_references.get(name) {
            r.clone()
        } else {
            let r = Rc::new(Self { element: Rc::downgrade(element), name: name.to_owned() });
            named_references.insert(name.to_owned(), r.clone());
            r
        };
        drop(named_references);
        result.check_invariant();
        result
    }
}

/// Must be put inside the Element and owns all the NamedReferenceInner
#[derive(Default)]
pub struct NamedReferenceContainer(RefCell<HashMap<String, Rc<NamedReferenceInner>>>);

impl NamedReferenceContainer {
    /// Returns true if there is at least one NamedReference pointing to the property `name` in this element.
    pub fn is_referenced(&self, name: &str) -> bool {
        if let Some(nri) = self.0.borrow().get(name) {
            // one reference for the hashmap itself
            Rc::strong_count(nri) > 1
        } else {
            false
        }
    }
}
