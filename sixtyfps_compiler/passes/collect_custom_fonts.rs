/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! Passes that fills the root component used_global

use crate::{
    expression_tree::{BuiltinFunction, Expression, Unit},
    object_tree::*,
};
use std::collections::BTreeSet;
use std::rc::Rc;

/// Fill the root_component's used_globals
pub fn collect_custom_fonts<'a>(
    root_component: &Rc<Component>,
    all_docs: impl Iterator<Item = &'a crate::object_tree::Document> + 'a,
    embed_fonts: bool,
) {
    let mut all_fonts = BTreeSet::new();

    for doc in all_docs {
        all_fonts.extend(doc.custom_fonts.iter())
    }

    let registration_function = if embed_fonts {
        Expression::BuiltinFunctionReference(BuiltinFunction::RegisterCustomFontByMemory, None)
    } else {
        Expression::BuiltinFunctionReference(BuiltinFunction::RegisterCustomFontByPath, None)
    };

    let prepare_font_registration_argument: Box<dyn Fn(&String) -> Expression> = if embed_fonts {
        Box::new(|font_path| {
            Expression::NumberLiteral(
                {
                    let mut resources = root_component.embedded_file_resources.borrow_mut();
                    let resource_id = match resources.get(font_path) {
                        Some(id) => *id,
                        None => {
                            let id = resources.len();
                            resources.insert(font_path.clone(), id);
                            id
                        }
                    };
                    resource_id as _
                },
                Unit::None,
            )
        })
    } else {
        Box::new(|font_path| Expression::StringLiteral(font_path.clone()))
    };

    root_component.setup_code.borrow_mut().extend(all_fonts.into_iter().map(|font_path| {
        Expression::FunctionCall {
            function: Box::new(registration_function.clone()),
            arguments: vec![prepare_font_registration_argument(font_path)],
            source_location: None,
        }
    }));
}
