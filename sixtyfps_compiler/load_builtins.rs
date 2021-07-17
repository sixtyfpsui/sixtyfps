/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

/*!
    Parse the contents of builtins.60 and fill the builtin type registry
*/

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::expression_tree::Expression;
use crate::langtype::{BuiltinElement, DefaultSizeBinding, NativeClass, Type};
use crate::object_tree::{self, *};
use crate::parser::{identifier_text, syntax_nodes, SyntaxKind, SyntaxNode};
use crate::typeregister::TypeRegister;

/// Parse the contents of builtins.60 and fill the builtin type registry
/// `register` is the register to fill with the builtin types.
/// At this point, it really should already contain the basic Types (string, int, ...)
pub fn load_builtins(register: &mut TypeRegister) {
    let mut diag = crate::diagnostics::BuildDiagnostics::default();
    let node = crate::parser::parse(include_str!("builtins.60").into(), None, &mut diag);
    if !diag.is_empty() {
        let vec = diag.to_string_vec();
        #[cfg(feature = "display-diagnostics")]
        diag.print();
        panic!("Error parsing the builtin elements: {:?}", vec);
    }

    assert_eq!(node.kind(), crate::parser::SyntaxKind::Document);
    let doc: syntax_nodes::Document = node.into();

    // parse structs
    for s in doc.StructDeclaration().chain(doc.ExportsList().flat_map(|e| e.StructDeclaration())) {
        let external_name = identifier_text(&s.DeclaredIdentifier()).unwrap();
        let mut ty = object_tree::type_struct_from_node(s.ObjectType(), &mut diag, register);
        if let Type::Struct { name, .. } = &mut ty {
            *name = Some(
                parse_annotation("name", &s.ObjectType())
                    .map_or_else(|| external_name.clone(), |s| s.unwrap())
                    .to_owned(),
            );
        } else {
            unreachable!()
        }
        register.insert_type_with_name(ty, external_name);
    }

    let mut natives = HashMap::<String, Rc<BuiltinElement>>::new();

    let exports = doc
        .ExportsList()
        .flat_map(|e| {
            e.Component()
                .map(|x| {
                    let x = identifier_text(&x.DeclaredIdentifier()).unwrap();
                    (x.clone(), x)
                })
                .into_iter()
                .chain(e.ExportSpecifier().map(|e| {
                    (
                        identifier_text(&e.ExportIdentifier()).unwrap(),
                        identifier_text(&e.ExportName().unwrap()).unwrap(),
                    )
                }))
        })
        .collect::<HashMap<_, _>>();

    for c in doc.Component().chain(doc.ExportsList().filter_map(|e| e.Component())) {
        let id = identifier_text(&c.DeclaredIdentifier()).unwrap();
        let e = c.Element();
        let diag = RefCell::new(&mut diag);
        let mut n = NativeClass::new_with_properties(
            &id,
            e.PropertyDeclaration()
                .filter(|p| p.TwoWayBinding().is_none()) // aliases are handled further down
                .map(|p| {
                    (
                        identifier_text(&p.DeclaredIdentifier()).unwrap(),
                        object_tree::type_from_node(
                            p.Type().unwrap(),
                            *diag.borrow_mut(),
                            register,
                        ),
                    )
                })
                .chain(e.CallbackDeclaration().map(|s| {
                    (
                        identifier_text(&s.DeclaredIdentifier()).unwrap(),
                        Type::Callback {
                            args: s
                                .Type()
                                .map(|a| {
                                    object_tree::type_from_node(a, *diag.borrow_mut(), register)
                                })
                                .collect(),
                            return_type: s.ReturnType().map(|a| {
                                Box::new(object_tree::type_from_node(
                                    a.Type(),
                                    *diag.borrow_mut(),
                                    register,
                                ))
                            }),
                        },
                    )
                })),
        );
        n.deprecated_aliases = e
            .PropertyDeclaration()
            .flat_map(|p| {
                if let Some(twb) = p.TwoWayBinding() {
                    let alias_name = identifier_text(&p.DeclaredIdentifier()).unwrap();
                    let alias_target = identifier_text(&twb.Expression().QualifiedName().expect(
                        "internal error: built-in aliases can only be declared within the type",
                    ))
                    .unwrap();
                    Some((alias_name, alias_target))
                } else {
                    None
                }
            })
            .collect();
        n.cpp_type = parse_annotation("cpp_type", &e).map(|x| x.unwrap());
        n.rust_type_constructor = parse_annotation("rust_type_constructor", &e).map(|x| x.unwrap());
        let global = if let Some(base) = e.QualifiedName() {
            let base = QualifiedTypeName::from_node(base).to_string();
            if base != "_" {
                n.parent = Some(natives.get(&base).unwrap().native_class.clone())
            };
            false
        } else {
            true
        };
        let mut builtin = BuiltinElement::new(Rc::new(n));
        builtin.is_global = global;
        let properties = &mut builtin.properties;
        for p in e.PropertyDeclaration() {
            if let (Some(name), Some(e)) =
                (identifier_text(&p.DeclaredIdentifier()), p.BindingExpression())
            {
                let info = properties.get_mut(&name).unwrap();
                if e.Expression()
                    .and_then(|e| e.QualifiedName())
                    .and_then(|q| q.child_text(SyntaxKind::Identifier))
                    .map(|s| s == "native_output")
                    == Some(true)
                {
                    info.is_native_output = true;
                } else {
                    let ty = info.ty.clone();
                    info.default_value = Some(compiled(e, register, ty));
                }
            }
        }
        builtin.disallow_global_types_as_child_elements =
            parse_annotation("disallow_global_types_as_child_elements", &e).is_some();
        builtin.is_non_item_type = parse_annotation("is_non_item_type", &e).is_some();
        builtin.is_internal = parse_annotation("is_internal", &e).is_some();
        builtin.accepts_focus = parse_annotation("accepts_focus", &e).is_some();
        builtin.default_size_binding = parse_annotation("default_size_binding", &e)
            .map(|size_type| match size_type.as_deref() {
                Some("expands_to_parent_geometry") => DefaultSizeBinding::ExpandsToParentGeometry,
                Some("implicit_size") => DefaultSizeBinding::ImplicitSize,
                other => panic!("invalid default size binding {:?}", other),
            })
            .unwrap_or(DefaultSizeBinding::None);
        builtin.additional_accepted_child_types = e
            .SubElement()
            .map(|s| {
                let a = identifier_text(&s.Element().QualifiedName().unwrap()).unwrap();
                let t = natives[&a].clone();
                (a, Type::Builtin(t))
            })
            .collect();
        if let Some(builtin_name) = exports.get(&id) {
            if !global {
                builtin.name = builtin_name.clone();
                register
                    .insert_type_with_name(Type::Builtin(Rc::new(builtin)), builtin_name.clone());
            } else {
                let glob = Rc::new(Component {
                    id: builtin_name.clone(),
                    root_element: Rc::new(RefCell::new(Element {
                        base_type: Type::Builtin(Rc::new(builtin)),
                        ..Default::default()
                    })),
                    ..Default::default()
                });
                glob.root_element.borrow_mut().enclosing_component = Rc::downgrade(&glob);
                register.insert_type(Type::Component(glob));
            }
        } else {
            // because they are not taken from if we inherit from it
            assert!(builtin.properties.iter().all(|(_, i)| i.default_value.is_none()));
            assert!(builtin.additional_accepted_child_types.is_empty());
            natives.insert(id, Rc::new(builtin));
        }
    }

    register.property_animation_type = Type::Builtin(natives.remove("PropertyAnimation").unwrap());

    if !diag.is_empty() {
        let vec = diag.to_string_vec();
        #[cfg(feature = "display-diagnostics")]
        diag.print();
        panic!("Error loading the builtin elements: {:?}", vec);
    }
}

/// Compile an expression, knowing that the expression is basic (does not have lookup to other things)
fn compiled(
    node: syntax_nodes::BindingExpression,
    type_register: &TypeRegister,
    ty: Type,
) -> Expression {
    let mut diag = crate::diagnostics::BuildDiagnostics::default();
    let e = Expression::from_binding_expression_node(
        node.clone().into(),
        &mut crate::lookup::LookupCtx::empty_context(type_register, &mut diag),
    )
    .maybe_convert_to(ty, &node, &mut diag);
    if diag.has_error() {
        let vec = diag.to_string_vec();
        #[cfg(feature = "display-diagnostics")]
        diag.print();
        panic!("Error parsing the builtin elements: {:?}", vec);
    }
    e
}

/// Find out if there are comments that starts with `//-key` and returns `None`
/// if no annotation with this key is found, or `Some(None)` if it is found without a value
/// or `Some(Some(value))` if there is a `//-key:value`  match
fn parse_annotation(key: &str, node: &SyntaxNode) -> Option<Option<String>> {
    for x in node.children_with_tokens() {
        if x.kind() == SyntaxKind::Comment {
            if let Some(comment) = x
                .as_token()
                .unwrap()
                .text()
                .strip_prefix("//-")
                .and_then(|x| x.trim_end().strip_prefix(key))
            {
                if comment.is_empty() {
                    return Some(None);
                }
                if let Some(comment) = comment.strip_prefix(":") {
                    return Some(Some(comment.to_owned()));
                }
            }
        }
    }
    None
}
