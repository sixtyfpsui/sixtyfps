/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Passe that apply the default property from the style.
//!
//! Note that the layout default property are handled in the lower_layout pass

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{Expression, NamedReference};
use crate::langtype::Type;
use crate::object_tree::Component;

/// Ideally we would be able to write this in builtin.60,  but the StyleMetrics is not available there
pub async fn apply_default_properties_from_style<'a>(
    root_component: &std::rc::Rc<Component>,
    type_loader: &mut crate::typeloader::TypeLoader<'a>,
    _diag: &mut BuildDiagnostics,
) {
    // Ignore import errors
    let mut build_diags_to_ignore = BuildDiagnostics::default();
    let style_metrics = type_loader
        .import_type("sixtyfps_widgets.60", "StyleMetrics", &mut build_diags_to_ignore)
        .await;
    let style_metrics = if let Some(Type::Component(c)) = style_metrics { c } else { return };

    crate::object_tree::recurse_elem_including_sub_components(
        root_component,
        &(),
        &mut |elem, _| {
            let mut elem = elem.borrow_mut();
            match elem.native_class().as_ref().map(|nc| nc.class_name.as_str()) {
                Some("TextInput") => {
                    elem.bindings.entry("text_cursor_width".into()).or_insert_with(|| {
                        Expression::PropertyReference(NamedReference::new(
                            &style_metrics.root_element,
                            "text_cursor_width",
                        ))
                        .into()
                    });
                }
                Some("Window") => {
                    elem.bindings.entry("background".into()).or_insert_with(|| {
                        Expression::PropertyReference(NamedReference::new(
                            &style_metrics.root_element,
                            "window_background",
                        ))
                        .into()
                    });
                }

                _ => {}
            }
        },
    )
}
