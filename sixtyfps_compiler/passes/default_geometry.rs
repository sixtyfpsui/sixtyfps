/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

/*! Set the width and height of Rectangle, TouchArea, ... to 100%,
    the implicit width or aspect ratio preserving for Images.
    Also set the Image.image-fit default depending on the presence of a
    layout parent.

    This pass must be run after lower_layout
*/

use std::rc::Rc;

use crate::diagnostics::BuildDiagnostics;
use crate::diagnostics::Spanned;
use crate::expression_tree::BindingExpression;
use crate::langtype::DefaultSizeBinding;
use crate::langtype::Type;
use crate::layout::Orientation;
use crate::object_tree::{Component, ElementRc};
use crate::{
    expression_tree::{BuiltinFunction, Expression, NamedReference},
    langtype::PropertyLookupResult,
};

pub fn default_geometry(root_component: &Rc<Component>, diag: &mut BuildDiagnostics) {
    crate::object_tree::recurse_elem_including_sub_components(
        &root_component,
        &None,
        &mut |elem: &ElementRc, parent: &Option<ElementRc>| {
            fix_percent_size(elem, parent, "width", diag);
            fix_percent_size(elem, parent, "height", diag);

            gen_layout_info_prop(elem);

            let base_type = elem.borrow().base_type.clone();
            if let (Some(parent), Type::Builtin(builtin_type)) = (parent, base_type) {
                match builtin_type.default_size_binding {
                    DefaultSizeBinding::None => {}
                    DefaultSizeBinding::ExpandsToParentGeometry => {
                        if !elem.borrow().child_of_layout {
                            make_default_100(elem, parent, "width");
                            make_default_100(elem, parent, "height");
                        }
                    }
                    DefaultSizeBinding::ImplicitSize => {
                        let has_length_property_binding = |elem: &ElementRc, property: &str| {
                            debug_assert!({
                                let PropertyLookupResult { resolved_name: _, property_type } =
                                    elem.borrow().lookup_property(property);
                                property_type == Type::LogicalLength
                            });

                            elem.borrow_mut()
                                .bindings
                                .get(property)
                                .map_or(false, |b| b.priority > 0)
                        };

                        let width_specified = has_length_property_binding(elem, "width");
                        let height_specified = has_length_property_binding(elem, "height");

                        let is_image = matches!(elem.borrow().builtin_type(), Some(builtin) if builtin.name == "Image");

                        if !elem.borrow().child_of_layout {
                            // Add aspect-ratio preserving width or height bindings
                            if is_image && width_specified && !height_specified {
                                make_default_aspect_ratio_preserving_binding(
                                    elem, "height", "width",
                                )
                            } else if is_image && height_specified && !width_specified {
                                make_default_aspect_ratio_preserving_binding(
                                    elem, "width", "height",
                                )
                            } else {
                                make_default_implicit(elem, "width", Orientation::Horizontal);
                                make_default_implicit(elem, "height", Orientation::Vertical);
                            }
                        } else if is_image {
                            // If an image is in a layout and has no explicit width or height specified, change the default for image-fit
                            // to `contain`
                            if !width_specified || !height_specified {
                                let PropertyLookupResult {
                                    resolved_name: image_fit_prop_name,
                                    property_type: image_fit_prop_type,
                                } = elem.borrow().lookup_property("image_fit");

                                elem.borrow_mut()
                                    .bindings
                                    .entry(image_fit_prop_name.into())
                                    .or_insert_with(|| {
                                        Expression::EnumerationValue(
                                            image_fit_prop_type
                                                .as_enum()
                                                .clone()
                                                .try_value_from_string("contain")
                                                .unwrap(),
                                        )
                                        .into()
                                    });
                            }
                        }
                    }
                }
            }
            Some(elem.clone())
        },
    )
}

/// Generate a layout_info_prop based on the children layouts
fn gen_layout_info_prop(elem: &ElementRc) {
    if elem.borrow().layout_info_prop.is_some() || elem.borrow().is_flickable_viewport {
        return;
    }

    let child_infos = elem
        .borrow()
        .children
        .iter()
        .filter_map(|c| c.borrow().layout_info_prop.clone())
        .collect::<Vec<_>>();

    if child_infos.is_empty() {
        return;
    }

    let li_v = super::lower_layout::create_new_prop(
        elem,
        "layoutinfo_v",
        crate::layout::layout_info_type(),
    );
    let li_h = super::lower_layout::create_new_prop(
        elem,
        "layoutinfo_h",
        crate::layout::layout_info_type(),
    );
    elem.borrow_mut().layout_info_prop = Some((li_h.clone(), li_v.clone()));
    let mut expr_h = implicit_layout_info_call(elem, Orientation::Horizontal);
    let mut expr_v = implicit_layout_info_call(elem, Orientation::Vertical);

    for child_info in child_infos {
        expr_h = Expression::BinaryExpression {
            lhs: Box::new(std::mem::take(&mut expr_h)),
            rhs: Box::new(Expression::PropertyReference(child_info.0)),
            op: '+',
        };
        expr_v = Expression::BinaryExpression {
            lhs: Box::new(std::mem::take(&mut expr_v)),
            rhs: Box::new(Expression::PropertyReference(child_info.1)),
            op: '+',
        };
    }

    let expr_v = BindingExpression::new_with_span(expr_v, elem.borrow().to_source_location());
    li_v.element().borrow_mut().bindings.insert(li_v.name().into(), expr_v);
    let expr_h = BindingExpression::new_with_span(expr_h, elem.borrow().to_source_location());
    li_h.element().borrow_mut().bindings.insert(li_h.name().into(), expr_h);
}

/// Replace expression such as  `"width: 30%;` with `width: 0.3 * parent.width;`
fn fix_percent_size(
    elem: &ElementRc,
    parent: &Option<ElementRc>,
    property: &str,
    diag: &mut BuildDiagnostics,
) {
    if !elem.borrow().bindings.get(property).map_or(false, |b| b.ty() == Type::Percent) {
        return;
    }
    let mut elem = elem.borrow_mut();
    let b = elem.bindings.get_mut(property).unwrap();
    if let Some(parent) = parent {
        debug_assert_eq!(
            parent.borrow().lookup_property(property),
            PropertyLookupResult {
                resolved_name: property.into(),
                property_type: Type::LogicalLength
            }
        );
        b.expression = Expression::BinaryExpression {
            lhs: Box::new(std::mem::take(&mut b.expression).maybe_convert_to(
                Type::Float32,
                &b.span,
                diag,
            )),
            rhs: Box::new(Expression::PropertyReference(NamedReference::new(parent, property))),
            op: '*',
        }
    } else {
        diag.push_error("Cannot find parent property to apply relative length".into(), &b.span);
    }
}

fn make_default_100(elem: &ElementRc, parent_element: &ElementRc, property: &str) {
    let PropertyLookupResult { resolved_name, property_type } =
        parent_element.borrow().lookup_property(property);
    if property_type != Type::LogicalLength {
        return;
    }
    elem.borrow_mut().bindings.entry(resolved_name.to_string()).or_insert_with(|| {
        Expression::PropertyReference(NamedReference::new(parent_element, resolved_name.as_ref()))
            .into()
    });
}

fn make_default_implicit(elem: &ElementRc, property: &str, orientation: Orientation) {
    elem.borrow_mut().bindings.entry(property.into()).or_insert_with(|| {
        Expression::StructFieldAccess {
            base: implicit_layout_info_call(elem, orientation).into(),
            name: "preferred".into(),
        }
        .into()
    });
}

// For an element with `width`, `height`, `preferred-width` and `preferred-height`, make an aspect
// ratio preserving binding. This is currently only called for Image elements. For example when for an
// image the `width` is specified and there is no `height` binding, it is called with with `missing_size_property = height`
// and `given_size_property = width` and install a binding like this:
//
//    height: self.width * self.preferred_height / self.preferred_width;
//
fn make_default_aspect_ratio_preserving_binding(
    elem: &ElementRc,
    missing_size_property: &str,
    given_size_property: &str,
) {
    if elem.borrow().bindings.contains_key(missing_size_property) {
        return;
    }

    debug_assert_eq!(elem.borrow().lookup_property("source").property_type, Type::Image);

    let implicit_size_var = Box::new(Expression::ReadLocalVariable {
        name: "image_implicit_size".into(),
        ty: match BuiltinFunction::ImageSize.ty() {
            Type::Function { return_type, .. } => *return_type,
            _ => panic!("invalid type for ImplicitItemSize built-in function"),
        },
    });

    let binding = Expression::CodeBlock(vec![
        Expression::StoreLocalVariable {
            name: "image_implicit_size".into(),
            value: Box::new(Expression::FunctionCall {
                function: Box::new(Expression::BuiltinFunctionReference(
                    BuiltinFunction::ImageSize,
                    None,
                )),
                arguments: vec![Expression::PropertyReference(NamedReference::new(elem, "source"))],
                source_location: None,
            }),
        },
        Expression::BinaryExpression {
            lhs: Box::new(Expression::BinaryExpression {
                lhs: Expression::PropertyReference(NamedReference::new(elem, given_size_property))
                    .into(),
                rhs: Box::new(Expression::StructFieldAccess {
                    base: implicit_size_var.clone(),
                    name: missing_size_property.into(),
                }),
                op: '*',
            }),
            rhs: Box::new(Expression::StructFieldAccess {
                base: implicit_size_var,
                name: given_size_property.into(),
            }),
            op: '/',
        },
    ]);

    elem.borrow_mut().bindings.insert(missing_size_property.to_string(), binding.into());
}

fn implicit_layout_info_call(elem: &ElementRc, orientation: Orientation) -> Expression {
    Expression::FunctionCall {
        function: Box::new(Expression::BuiltinFunctionReference(
            BuiltinFunction::ImplicitLayoutInfo(orientation),
            None,
        )),
        arguments: vec![Expression::ElementReference(Rc::downgrade(elem))],
        source_location: None,
    }
}
