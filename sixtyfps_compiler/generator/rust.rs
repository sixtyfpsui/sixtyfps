/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*! module for the Rust code generator

Some convention used in the generated code:
 - `_self` is of type `Pin<&ComponentType>`  where ComponentType is the type of the generated component,
    this is existing for any evaluation of a binding
 - `self_rc` is of type `VRc<ComponentVTable, ComponentType>` or `Rc<ComponentType>` for globals
    this is usually a local variable to the init code that shouldn't rbe relied upon by the binding code.
 - `self_mapped` is of type `VRcMapped<ComponentVTable, ComponentType>` or `Rc<ComponentType>` for globals,
    it is also used in the init code and in code that can initialize repeater's component

*/

use crate::diagnostics::{BuildDiagnostics, Spanned};
use crate::expression_tree::{
    BuiltinFunction, EasingCurve, Expression, NamedReference, OperatorClass, Path,
};
use crate::langtype::Type;
use crate::layout::{Layout, LayoutGeometry, LayoutRect, Orientation};
use crate::object_tree::{Component, Document, ElementRc};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use std::{collections::BTreeMap, rc::Rc};

impl quote::ToTokens for Orientation {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let tks = match self {
            Orientation::Horizontal => quote!(sixtyfps::re_exports::Orientation::Horizontal),
            Orientation::Vertical => quote!(sixtyfps::re_exports::Orientation::Vertical),
        };
        tokens.extend(tks);
    }
}

fn rust_type(ty: &Type) -> Option<proc_macro2::TokenStream> {
    match ty {
        Type::Int32 => Some(quote!(i32)),
        Type::Float32 => Some(quote!(f32)),
        Type::String => Some(quote!(sixtyfps::re_exports::SharedString)),
        Type::Color => Some(quote!(sixtyfps::re_exports::Color)),
        Type::Duration => Some(quote!(i64)),
        Type::Angle => Some(quote!(f32)),
        Type::PhysicalLength => Some(quote!(f32)),
        Type::LogicalLength => Some(quote!(f32)),
        Type::Percent => Some(quote!(f32)),
        Type::Bool => Some(quote!(bool)),
        Type::Image => Some(quote!(sixtyfps::re_exports::Image)),
        Type::Struct { fields, name: None, .. } => {
            let elem = fields.values().map(|v| rust_type(v)).collect::<Option<Vec<_>>>()?;
            // This will produce a tuple
            Some(quote!((#(#elem,)*)))
        }
        Type::Struct { name: Some(name), .. } => Some(struct_name_to_tokens(name)),
        Type::Array(o) => {
            let inner = rust_type(o)?;
            Some(quote!(sixtyfps::re_exports::ModelHandle<#inner>))
        }
        Type::Enumeration(e) => {
            let e = format_ident!("r#{}", e.name);
            Some(quote!(sixtyfps::re_exports::#e))
        }
        Type::Brush => Some(quote!(sixtyfps::Brush)),
        Type::LayoutCache => Some(quote!(SharedVector<f32>)),
        _ => None,
    }
}

fn get_rust_type(
    ty: &Type,
    type_node: &dyn crate::diagnostics::Spanned,
    diag: &mut BuildDiagnostics,
) -> proc_macro2::TokenStream {
    rust_type(ty).unwrap_or_else(|| {
        diag.push_error(format!("Cannot map property type {} to Rust", ty), type_node);
        quote!(_)
    })
}

/// Generate the rust code for the given component.
///
/// Fill the diagnostic in case of error.
pub fn generate(doc: &Document, diag: &mut BuildDiagnostics) -> Option<TokenStream> {
    let (structs_ids, structs): (Vec<_>, Vec<_>) = doc
        .root_component
        .used_types
        .borrow()
        .structs
        .iter()
        .filter_map(|ty| {
            if let Type::Struct { fields, name: Some(name), node: Some(_) } = ty {
                Some((format_ident!("r#{}", name), generate_struct(name, fields, diag)))
            } else {
                None
            }
        })
        .unzip();
    let compo = generate_component(&doc.root_component, diag)?;
    let compo_id = public_component_id(&doc.root_component);
    let compo_module = format_ident!("sixtyfps_generated_{}", compo_id);
    let version_check = format_ident!(
        "VersionCheck_{}_{}_{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    );
    let globals = doc
        .root_component
        .used_types
        .borrow()
        .globals
        .iter()
        .filter(|glob| !matches!(glob.root_element.borrow().base_type, Type::Builtin(_)))
        .filter_map(|glob| generate_component(glob, diag))
        .collect::<Vec<_>>();
    Some(quote! {
        #[allow(non_snake_case)]
        mod #compo_module {
            use sixtyfps::re_exports::*;
            #(#structs)*
            #(#globals)*
            #compo
            const _THE_SAME_VERSION_MUST_BE_USED_FOR_THE_COMPILER_AND_THE_RUNTIME : sixtyfps::#version_check = sixtyfps::#version_check;
        }
        pub use #compo_module::{#compo_id #(,#structs_ids)* };
        pub use sixtyfps::ComponentHandle;
    })
}

fn generate_struct(
    name: &str,
    fields: &BTreeMap<String, Type>,
    diag: &mut BuildDiagnostics,
) -> TokenStream {
    let component_id = struct_name_to_tokens(name);
    let (declared_property_vars, declared_property_types): (Vec<_>, Vec<_>) = fields
        .iter()
        .map(|(name, ty)| {
            (
                format_ident!("r#{}", name),
                get_rust_type(ty, &crate::diagnostics::SourceLocation::default(), diag),
            )
        })
        .unzip();

    quote! {
        #[derive(Default, PartialEq, Debug, Clone)]
        pub struct #component_id {
            #(pub #declared_property_vars : #declared_property_types),*
        }
    }
}

fn handle_property_binding(
    component: &Rc<Component>,
    item_rc: &ElementRc,
    prop_name: &str,
    binding_expression: &Expression,
    is_constant: bool,
    init: &mut Vec<TokenStream>,
) {
    let rust_property = access_member(item_rc, prop_name, component, quote!(_self), false);
    let prop_type = item_rc.borrow().lookup_property(prop_name).property_type;
    let (downgrade, upgrade) = if item_rc
        .borrow()
        .enclosing_component
        .upgrade()
        .unwrap()
        .is_global()
    {
        (
            quote!(let self_weak = sixtyfps::re_exports::PinWeak::downgrade(self_mapped.clone());),
            quote!(
                let self_mapped = self_weak.upgrade().unwrap();
                let _self = self_mapped.as_ref();
            ),
        )
    } else {
        (
            quote!(let self_weak = sixtyfps::re_exports::VRcMapped::downgrade(&self_mapped);),
            quote!(
                let self_mapped = self_weak.upgrade().unwrap();
                let _self = self_mapped.as_pin_ref();
            ),
        )
    };

    if matches!(prop_type, Type::Callback { .. }) {
        let tokens_for_expression = compile_expression(binding_expression, component);
        init.push(quote!(
            #rust_property.set_handler({
                #downgrade
                move |args| {
                    #upgrade
                    (#tokens_for_expression) as _
                }
            });
        ));
    } else if let Expression::TwoWayBinding(nr, next) = &binding_expression {
        let p2 = access_member(&nr.element(), nr.name(), component, quote!(_self), false);
        init.push(quote!(
            Property::link_two_way(#rust_property, #p2);
        ));
        if let Some(next) = next {
            handle_property_binding(
                component,
                item_rc,
                prop_name,
                next,
                is_constant || next.is_constant(),
                init,
            )
        }
    } else {
        let tokens_for_expression = compile_expression(binding_expression, component);
        init.push(if is_constant {
            let t = rust_type(&prop_type).unwrap_or(quote!(_));
            quote! { #rust_property.set((||-> #t { (#tokens_for_expression) as #t })()); }
        } else {
            let binding_tokens = quote!({
                #downgrade
                move || {
                    #upgrade
                    (#tokens_for_expression) as _
                }
            });

            let is_state_info = matches!(prop_type, Type::Struct { name: Some(name), .. } if name.ends_with("::StateInfo"));
            if is_state_info {
                quote! { sixtyfps::re_exports::set_state_binding(#rust_property, #binding_tokens); }
            } else {
                match item_rc.borrow().property_animations.get(prop_name) {
                    Some(crate::object_tree::PropertyAnimation::Static(anim)) => {
                        let anim = property_animation_tokens(component, anim);
                        quote! { #rust_property.set_animated_binding(#binding_tokens, #anim); }
                    }
                    Some(crate::object_tree::PropertyAnimation::Transition {
                        state_ref,
                        animations,
                    }) => {
                        let state_tokens = compile_expression(state_ref, component);
                        let anim_expr = animations.iter().map(|a| {
                            let cond = compile_expression(
                                &a.condition(Expression::ReadLocalVariable {
                                    name: "state".into(),
                                    ty: state_ref.ty(),
                                }),
                                component,
                            );
                            let a_tokens = property_animation_tokens(component, &a.animation);
                            quote!(if #cond { #a_tokens })
                        });
                        quote! {
                            #downgrade
                            #rust_property.set_animated_binding_for_transition(#binding_tokens, move || {
                                #upgrade
                                let state = #state_tokens;
                                ({ #(#anim_expr else)* { sixtyfps::re_exports::PropertyAnimation::default() }  }, state.change_time)
                            });
                        }
                    }
                    None => {
                        quote! { #rust_property.set_binding(#binding_tokens); }
                    }
                }
            }
        });
    }
}

/// Generate the rust code for the given component.
///
/// Fill the diagnostic in case of error.
fn generate_component(
    component: &Rc<Component>,
    diag: &mut BuildDiagnostics,
) -> Option<TokenStream> {
    let inner_component_id = inner_component_id(component);

    let mut extra_components = component
        .popup_windows
        .borrow()
        .iter()
        .filter_map(|c| generate_component(&c.component, diag))
        .collect::<Vec<_>>();
    let mut declared_property_vars = vec![];
    let mut declared_property_types = vec![];
    let mut declared_callbacks = vec![];
    let mut declared_callbacks_types = vec![];
    let mut declared_callbacks_ret = vec![];
    let mut property_and_callback_accessors: Vec<TokenStream> = vec![];
    for (prop_name, property_decl) in component.root_element.borrow().property_declarations.iter() {
        let prop_ident = format_ident!("r#{}", prop_name);
        let prop = if let Some(alias) = &property_decl.is_alias {
            access_named_reference(alias, component, quote!(_self))
        } else {
            let field = access_component_field_offset(&inner_component_id, &prop_ident);
            quote!(#field.apply_pin(_self))
        };

        if let Type::Callback { args, return_type } = &property_decl.property_type {
            let callback_args = args
                .iter()
                .map(|a| get_rust_type(a, &property_decl.type_node(), diag))
                .collect::<Vec<_>>();
            let return_type = return_type
                .as_ref()
                .map_or(quote!(()), |a| get_rust_type(a, &property_decl.type_node(), diag));

            if property_decl.expose_in_public_api {
                let args_name = (0..callback_args.len())
                    .map(|i| format_ident!("arg_{}", i))
                    .collect::<Vec<_>>();
                let caller_ident = format_ident!("invoke_{}", prop_name);
                property_and_callback_accessors.push(quote!(

                    #[allow(dead_code)]
                    pub fn #caller_ident(&self, #(#args_name : #callback_args,)*) -> #return_type {
                        let _self = vtable::VRc::as_pin_ref(&self.0);
                        #prop.call(&(#(#args_name,)*))
                    }
                ));

                let on_ident = format_ident!("on_{}", prop_name);
                let args_index =
                    (0..callback_args.len()).map(proc_macro2::Literal::usize_unsuffixed);
                property_and_callback_accessors.push(
                    quote!(
                        #[allow(dead_code)]
                        pub fn #on_ident(&self, f: impl Fn(#(#callback_args),*) -> #return_type + 'static) {
                            let _self = vtable::VRc::as_pin_ref(&self.0);
                            #[allow(unused)]
                            #prop.set_handler(
                                // FIXME: why do i need to clone here?
                                move |args| f(#(args.#args_index.clone()),*)
                            )
                        }
                    )
                    ,
                );
            }
            if property_decl.is_alias.is_none() {
                declared_callbacks.push(prop_ident.clone());
                declared_callbacks_types.push(callback_args);
                declared_callbacks_ret.push(return_type);
            }
        } else {
            let rust_property_type =
                get_rust_type(&property_decl.property_type, &property_decl.type_node(), diag);
            if property_decl.expose_in_public_api {
                let getter_ident = format_ident!("get_{}", prop_name);
                let setter_ident = format_ident!("set_{}", prop_name);

                property_and_callback_accessors.push(quote!(
                    #[allow(dead_code)]
                    pub fn #getter_ident(&self) -> #rust_property_type {
                        #[allow(unused_imports)]
                        use sixtyfps::re_exports::*;
                        let _self = vtable::VRc::as_pin_ref(&self.0);
                        #prop.get()
                    }
                ));

                let set_value = property_set_value_tokens(
                    component,
                    &component.root_element,
                    prop_name,
                    quote!(value),
                );
                property_and_callback_accessors.push(quote!(
                    #[allow(dead_code)]
                    pub fn #setter_ident(&self, value: #rust_property_type) {
                        #[allow(unused_imports)]
                        use sixtyfps::re_exports::*;
                        let _self = vtable::VRc::as_pin_ref(&self.0);
                        #prop.#set_value
                    }
                ));
            }

            if property_decl.is_alias.is_none() {
                declared_property_vars.push(prop_ident.clone());
                declared_property_types.push(rust_property_type.clone());
            }
        }
    }

    if diag.has_error() {
        return None;
    }

    let mut item_tree_array = Vec::new();
    let mut item_names = Vec::new();
    let mut item_types = Vec::new();
    let mut repeated_element_names = Vec::new();
    let mut repeated_element_layouts = Vec::new();
    let mut repeated_element_components = Vec::new();
    let mut repeated_visit_branch = Vec::new();
    let mut repeated_input_branch = Vec::new();
    let mut init = Vec::new();
    let mut binding_and_handler_setup_code = Vec::new();
    let mut window_field_init = None;
    let mut window_parent_param = None;
    super::build_array_helper(component, |item_rc, children_index, parent_index| {
        let parent_index = parent_index as u32;
        let item = item_rc.borrow();
        if item.base_type == Type::Void {
            assert!(component.is_global());
        } else if let Some(repeated) = &item.repeated {
            let base_component = item.base_type.as_component();
            let repeater_index = repeated_element_names.len();
            let repeater_id = format_ident!("repeater_{}", item.id);
            let rep_inner_component_id = self::inner_component_id(&*base_component);

            extra_components.push(generate_component(&*base_component, diag).unwrap_or_else(
                || {
                    assert!(diag.has_error());
                    Default::default()
                },
            ));

            let extra_fn = if repeated.is_listview.is_some() {
                let am = |prop| {
                    access_member(
                        &base_component.root_element,
                        prop,
                        base_component,
                        quote!(self),
                        false,
                    )
                };
                let p_y = am("y");
                let p_height = am("height");
                let p_width = am("width");
                quote! {
                    fn listview_layout(
                        self: core::pin::Pin<&Self>,
                        offset_y: &mut f32,
                        viewport_width: core::pin::Pin<&sixtyfps::re_exports::Property<f32>>,
                    ) {
                        use sixtyfps::re_exports::*;
                        let vp_w = viewport_width.get();
                        #p_y.set(*offset_y);
                        *offset_y += #p_height.get();
                        let w = #p_width.get();
                        if vp_w < w {
                            viewport_width.set(w);
                        }
                    }
                }
            } else {
                // TODO: we could generate this code only if we know that this component is in a box layout
                quote! {
                    fn box_layout_data(self: ::core::pin::Pin<&Self>, o: sixtyfps::re_exports::Orientation)
                        -> sixtyfps::re_exports::BoxLayoutCellData
                    {
                        use sixtyfps::re_exports::*;
                        BoxLayoutCellData { constraint: self.as_ref().layout_info(o) }
                    }
                }
            };

            extra_components.push(if repeated.is_conditional_element {
                quote! {
                    impl sixtyfps::re_exports::RepeatedComponent for #rep_inner_component_id {
                        type Data = ();
                        fn update(&self, _: usize, _: Self::Data) { }
                        #extra_fn
                    }
                }
            } else {
                let data_type = get_rust_type(
                    &Expression::RepeaterModelReference { element: Rc::downgrade(item_rc) }.ty(),
                    &item.node.as_ref().map(|x| x.to_source_location()),
                    diag,
                );

                quote! {
                    impl sixtyfps::re_exports::RepeatedComponent for #rep_inner_component_id {
                        type Data = #data_type;
                        fn update(&self, index: usize, data: Self::Data) {
                            self.index.set(index);
                            self.model_data.set(data);
                        }
                        #extra_fn
                    }
                }
            });

            let mut model = compile_expression(&repeated.model, component);
            if repeated.is_conditional_element {
                model =
                    quote!(sixtyfps::re_exports::ModelHandle::new(std::rc::Rc::<bool>::new(#model)))
            }

            // FIXME: there could be an optimization if `repeated.model.is_constant()`, we don't need a binding
            binding_and_handler_setup_code.push(quote! {
                _self.#repeater_id.set_model_binding({
                    let self_weak = sixtyfps::re_exports::VRcMapped::downgrade(&self_mapped);
                    move || {
                        let self_mapped = self_weak.upgrade().unwrap();
                        let _self = self_mapped.as_pin_ref();
                        (#model) as _
                    }
                });
            });

            if let Some(listview) = &repeated.is_listview {
                let vp_y = access_named_reference(&listview.viewport_y, component, quote!(_self));
                let vp_h =
                    access_named_reference(&listview.viewport_height, component, quote!(_self));
                let lv_h =
                    access_named_reference(&listview.listview_height, component, quote!(_self));
                let vp_w =
                    access_named_reference(&listview.viewport_width, component, quote!(_self));
                let lv_w =
                    access_named_reference(&listview.listview_width, component, quote!(_self));

                let ensure_updated = quote! {
                    #inner_component_id::FIELD_OFFSETS.#repeater_id.apply_pin(_self).ensure_updated_listview(
                        || { #rep_inner_component_id::new(_self.self_weak.get().unwrap().clone(), &_self.window).into() },
                        #vp_w, #vp_h, #vp_y, #lv_w.get(), #lv_h
                    );
                };

                repeated_visit_branch.push(quote!(
                    #repeater_index => {
                        #ensure_updated
                        _self.#repeater_id.visit(order, visitor)
                    }
                ));

                repeated_element_layouts.push(quote!(
                    #ensure_updated
                ));
            } else {
                let ensure_updated = quote! {
                    #inner_component_id::FIELD_OFFSETS.#repeater_id.apply_pin(_self).ensure_updated(
                        || { #rep_inner_component_id::new(_self.self_weak.get().unwrap().clone(), &_self.window).into() }
                    );
                };

                repeated_visit_branch.push(quote!(
                    #repeater_index => {
                        #ensure_updated
                        _self.#repeater_id.visit(order, visitor)
                    }
                ));

                repeated_element_layouts.push(quote!(
                    #ensure_updated
                    _self.#repeater_id.compute_layout();
                ));
            }

            repeated_input_branch.push(quote!(
                #repeater_index => self.#repeater_id.input_event(rep_index, event, window),
            ));

            item_tree_array.push(quote!(
                sixtyfps::re_exports::ItemTreeNode::DynamicTree {
                    index: #repeater_index,
                    parent_index: #parent_index,
                }
            ));

            repeated_element_names.push(repeater_id);
            repeated_element_components.push(rep_inner_component_id);
        } else if item.is_flickable_viewport {
            let field_name = format_ident!(
                "r#{}",
                crate::object_tree::find_parent_element(item_rc).unwrap().borrow().id
            );
            let children_count = item.children.len() as u32;
            let field = access_component_field_offset(&inner_component_id, &field_name);

            item_tree_array.push(quote!(
                sixtyfps::re_exports::ItemTreeNode::Item{
                    item: VOffset::new(#field + sixtyfps::re_exports::Flickable::FIELD_OFFSETS.viewport),
                    children_count: #children_count,
                    children_index: #children_index,
                    parent_index: #parent_index
                }
            ));
        } else {
            let field_name = format_ident!("r#{}", item.id);
            let children_count = item.children.len() as u32;
            let field = access_component_field_offset(&inner_component_id, &field_name);

            item_tree_array.push(quote!(
                sixtyfps::re_exports::ItemTreeNode::Item{
                    item: VOffset::new(#field),
                    children_count: #children_count,
                    children_index: #children_index,
                    parent_index: #parent_index,
                }
            ));
            item_names.push(field_name);
            item_types.push(format_ident!("r#{}", item.base_type.as_native().class_name));
        }
    });

    super::handle_property_bindings_init(component, |elem, prop, binding| {
        handle_property_binding(
            component,
            elem,
            prop,
            binding,
            binding.analysis.borrow().as_ref().map_or(false, |a| a.is_const),
            &mut init,
        )
    });

    let resource_symbols: Vec<proc_macro2::TokenStream> = component
        .embedded_file_resources
        .borrow()
        .iter()
        .map(|(path, id)| {
            let symbol = format_ident!("SFPS_EMBEDDED_RESOURCE_{}", id);
            quote!(const #symbol: &'static [u8] = ::core::include_bytes!(#path);)
        })
        .collect();

    let layouts = compute_layout(component);
    let mut visibility = None;
    let mut parent_component_type = None;
    let mut has_window_impl = None;
    let mut window_field = Some(quote!(window: sixtyfps::re_exports::ComponentWindow));
    if let Some(parent_element) = component.parent_element.upgrade() {
        if parent_element.borrow().repeated.as_ref().map_or(false, |r| !r.is_conditional_element) {
            declared_property_vars.push(format_ident!("index"));
            declared_property_types.push(quote!(usize));
            declared_property_vars.push(format_ident!("model_data"));
            declared_property_types.push(get_rust_type(
                &Expression::RepeaterModelReference { element: component.parent_element.clone() }
                    .ty(),
                &parent_element.borrow().node.as_ref().map(|x| x.to_source_location()),
                diag,
            ));
        }

        parent_component_type = Some(self::inner_component_id(
            &parent_element.borrow().enclosing_component.upgrade().unwrap(),
        ));
        window_field_init = Some(quote!(window: parent_window.clone()));
        window_parent_param = Some(quote!(, parent_window: &sixtyfps::re_exports::ComponentWindow))
    } else if !component.is_global() {
        // FIXME: This field is public for testing.
        window_field = Some(quote!(pub window: sixtyfps::re_exports::ComponentWindow));
        window_field_init = Some(quote!(window: sixtyfps::create_window()));

        visibility = Some(quote!(pub));

        init.push(quote!(_self.window.set_component(&VRc::into_dyn(_self.as_ref().self_weak.get().unwrap().upgrade().unwrap()));));

        has_window_impl = Some(quote!(
            impl sixtyfps::testing::HasWindow for #inner_component_id {
                fn component_window(&self) -> &sixtyfps::re_exports::ComponentWindow {
                    &self.window
                }
            }
        ))
    } else {
        window_field = None;
    };

    // Trick so we can use `#()` as a `if let Some` in `quote!`
    let parent_component_type = parent_component_type.iter().collect::<Vec<_>>();

    if diag.has_error() {
        return None;
    }

    let (drop_impl, pin) = if component.is_global() {
        (None, quote!(#[pin]))
    } else {
        let item_fields = item_names
            .iter()
            .map(|field_name| access_component_field_offset(&format_ident!("Self"), &field_name));
        (
            Some(quote!(impl sixtyfps::re_exports::PinnedDrop for #inner_component_id {
                fn drop(self: core::pin::Pin<&mut #inner_component_id>) {
                    use sixtyfps::re_exports::*;
                    let items = [
                        #(VRef::new_pin(#item_fields.apply_pin(self.as_ref())),)*
                    ];
                    self.window.free_graphics_resources(&Slice::from_slice(&items));
                }
            })),
            quote!(#[pin_drop]),
        )
    };

    for extra_init_code in component.setup_code.borrow().iter() {
        init.push(compile_expression(extra_init_code, component));
    }

    let (item_tree_impl, component_impl) = if component.is_global() {
        (None, None)
    } else {
        let item_tree_array_len = item_tree_array.len();
        let parent_item_index = component
            .parent_element
            .upgrade()
            .and_then(|e| e.borrow().item_index.get().map(|x| *x));
        let parent_item_index = parent_item_index.iter();
        init.insert(0, quote!(sixtyfps::re_exports::init_component_items(_self, Self::item_tree(), &_self.window);));
        (
            Some(quote! {
                fn item_tree() -> &'static [sixtyfps::re_exports::ItemTreeNode<Self>] {
                    use sixtyfps::re_exports::*;
                    ComponentVTable_static!(static VT for #inner_component_id);
                    // FIXME: ideally this should be a const
                    static ITEM_TREE : Lazy<[sixtyfps::re_exports::ItemTreeNode<#inner_component_id>; #item_tree_array_len]>  =
                        Lazy::new(|| [#(#item_tree_array),*]);
                    &*ITEM_TREE
                }
            }),
            Some(quote! {
                impl sixtyfps::re_exports::Component for #inner_component_id {
                    fn visit_children_item(self: ::core::pin::Pin<&Self>, index: isize, order: sixtyfps::re_exports::TraversalOrder, visitor: sixtyfps::re_exports::ItemVisitorRefMut)
                        -> sixtyfps::re_exports::VisitChildrenResult
                    {
                        use sixtyfps::re_exports::*;
                        return sixtyfps::re_exports::visit_item_tree(self, &VRc::into_dyn(self.as_ref().self_weak.get().unwrap().upgrade().unwrap()), Self::item_tree(), index, order, visitor, visit_dynamic);
                        #[allow(unused)]
                        fn visit_dynamic(_self: ::core::pin::Pin<&#inner_component_id>, order: sixtyfps::re_exports::TraversalOrder, visitor: ItemVisitorRefMut, dyn_index: usize) -> VisitChildrenResult  {
                            match dyn_index {
                                #(#repeated_visit_branch)*
                                _ => panic!("invalid dyn_index {}", dyn_index),
                            }
                        }
                    }


                    #layouts

                    fn get_item_ref(self: ::core::pin::Pin<&Self>, index: usize) -> ::core::pin::Pin<ItemRef> {
                        match &Self::item_tree()[index] {
                            ItemTreeNode::Item { item, .. } => item.apply_pin(self),
                            ItemTreeNode::DynamicTree { .. } => panic!("get_item_ref called on dynamic tree"),

                        }
                    }

                    fn parent_item(self: ::core::pin::Pin<&Self>, index: usize, result: &mut sixtyfps::re_exports::ItemWeak) {
                        if index == 0 {
                            #(
                                if let Some(parent) = self.parent.clone().into_dyn().upgrade() {
                                    *result = sixtyfps::re_exports::ItemRc::new(parent, #parent_item_index).parent_item();
                                }
                            )*
                            return;
                        }
                        let parent_index = match &Self::item_tree()[index] {
                            ItemTreeNode::Item { parent_index, .. } => *parent_index,
                            ItemTreeNode::DynamicTree { parent_index, .. } => *parent_index,
                        };
                        let self_rc = self.self_weak.get().unwrap().clone().into_dyn().upgrade().unwrap();
                        *result = ItemRc::new(self_rc, parent_index as _).downgrade()
                    }
                }
            }),
        )
    };

    let (global_name, global_type): (Vec<_>, Vec<_>) = component
        .used_types
        .borrow()
        .globals
        .iter()
        .map(|g| (format_ident!("global_{}", public_component_id(g)), self::inner_component_id(g)))
        .unzip();

    let (new_code, self_mapped_type, self_mapped_init_code) = if !component.is_global() {
        (
            quote! {
                let self_rc = VRc::new(self_);
                self_rc.self_weak.set(VRc::downgrade(&self_rc)).map_err(|_|())
                        .expect("Can only be pinned once");
                let _self = self_rc.as_pin_ref();
                let self_mapped = sixtyfps::re_exports::VRc::map(self_rc.clone(), |s| s);
            },
            quote!(&sixtyfps::re_exports::VRcMapped<sixtyfps::re_exports::ComponentVTable, Self>),
            quote!(self_mapped.as_pin_ref()),
        )
    } else {
        (
            quote! {
                let self_rc = ::std::rc::Rc::pin(self_);
                let _self = self_rc.as_ref();
                let self_mapped = self_rc.clone();
            },
            quote!(&::core::pin::Pin<::std::rc::Rc<Self>>),
            quote!(self_mapped.as_ref()),
        )
    };
    let self_weak = if !component.is_global() { Some(quote!(self_weak)) } else { None };
    let self_weak = self_weak.into_iter().collect::<Vec<_>>();
    let component_handle = if !component.is_global() {
        quote!(vtable::VRc<sixtyfps::re_exports::ComponentVTable, Self>)
    } else {
        quote!(::core::pin::Pin<::std::rc::Rc<Self>>)
    };

    let public_interface = if !component.is_global() && component.parent_element.upgrade().is_none()
    {
        let public_component_id = public_component_id(component);

        let parent_name =
            if !parent_component_type.is_empty() { Some(quote!(parent)) } else { None };
        let window_parent_name = window_parent_param.as_ref().map(|_| quote!(, parent_window));

        let component_handle_impl = if component.parent_element.upgrade().is_none() {
            Some(quote!(
                impl sixtyfps::ComponentHandle for #public_component_id {
                    type Inner = #inner_component_id;
                    fn as_weak(&self) -> sixtyfps::Weak<Self> {
                        sixtyfps::Weak::new(&self.0)
                    }

                    fn clone_strong(&self) -> Self {
                        Self(self.0.clone())
                    }

                    fn from_inner(inner: vtable::VRc<sixtyfps::re_exports::ComponentVTable, #inner_component_id>) -> Self {
                        Self(inner)
                    }

                    fn run(&self) {
                        self.show();
                        sixtyfps::run_event_loop();
                        self.hide();
                    }

                    fn show(&self) {
                        vtable::VRc::as_pin_ref(&self.0).window.show();
                    }

                    fn hide(&self) {
                        vtable::VRc::as_pin_ref(&self.0).window.hide();
                    }
                }
            ))
        } else {
            None
        };

        Some(quote!(
            #visibility struct #public_component_id(vtable::VRc<sixtyfps::re_exports::ComponentVTable, #inner_component_id>);

            impl #public_component_id {
                pub fn new(#(parent: sixtyfps::re_exports::VWeak::<sixtyfps::re_exports::ComponentVTable, #parent_component_type>)* #window_parent_param) -> Self {
                    Self(#inner_component_id::new(#parent_name #window_parent_name))
                }
                #(#property_and_callback_accessors)*
            }

            #component_handle_impl

            impl From<#public_component_id> for vtable::VRc<sixtyfps::re_exports::ComponentVTable, #inner_component_id> {
                fn from(value: #public_component_id) -> Self {
                    value.0
                }
            }
        ))
    } else {
        None
    };

    Some(quote!(
        #(#resource_symbols)*

        #[derive(sixtyfps::re_exports::FieldOffsets)]
        #[const_field_offset(sixtyfps::re_exports::const_field_offset)]
        #[repr(C)]
        #pin
        #visibility struct #inner_component_id {
            #(#item_names : sixtyfps::re_exports::#item_types,)*
            #(#declared_property_vars : sixtyfps::re_exports::Property<#declared_property_types>,)*
            #(#declared_callbacks : sixtyfps::re_exports::Callback<(#(#declared_callbacks_types,)*), #declared_callbacks_ret>,)*
            #(#repeated_element_names : sixtyfps::re_exports::Repeater<#repeated_element_components>,)*
            #(#self_weak : sixtyfps::re_exports::OnceCell<sixtyfps::re_exports::VWeak<sixtyfps::re_exports::ComponentVTable, #inner_component_id>>,)*
            #(parent : sixtyfps::re_exports::VWeak<sixtyfps::re_exports::ComponentVTable, #parent_component_type>,)*
            #(#global_name : ::core::pin::Pin<::std::rc::Rc<#global_type>>,)*
            #window_field
        }

        #component_impl

        impl #inner_component_id{
            fn init(self_mapped: #self_mapped_type)
            {
                #![allow(unused)]
                let _self = #self_mapped_init_code;
                #(#binding_and_handler_setup_code)*
            }

            pub fn new(#(parent: sixtyfps::re_exports::VWeak::<sixtyfps::re_exports::ComponentVTable, #parent_component_type>)* #window_parent_param)
                -> #component_handle
            {
                #![allow(unused)]
                use sixtyfps::re_exports::*;
                let mut self_ = Self {
                    #(#item_names : ::core::default::Default::default(),)*
                    #(#declared_property_vars : ::core::default::Default::default(),)*
                    #(#declared_callbacks : ::core::default::Default::default(),)*
                    #(#repeated_element_names : ::core::default::Default::default(),)*
                    #(#self_weak : ::core::default::Default::default(),)*
                    #(parent : parent as sixtyfps::re_exports::VWeak::<sixtyfps::re_exports::ComponentVTable, #parent_component_type>,)*
                    #(#global_name : #global_type::new(),)*
                    #window_field_init
                };
                #new_code
                #(#init)*
                Self::init(&self_mapped);
                self_rc.into()
            }
            #item_tree_impl

        }

        #public_interface

        #drop_impl

        #has_window_impl

        #(#extra_components)*
    ))
}

/// Return an identifier suitable for this component for internal use
fn inner_component_id(component: &Component) -> proc_macro2::Ident {
    if component.is_global()
        && matches!(&component.root_element.borrow().base_type, Type::Builtin(_))
    {
        public_component_id(component)
    } else {
        format_ident!("Inner{}", public_component_id(component))
    }
}

/// Return an identifier suitable for this component for the developer facing API
fn public_component_id(component: &Component) -> proc_macro2::Ident {
    if component.is_global() {
        format_ident!("r#{}", component.root_element.borrow().id)
    } else if component.id.is_empty() {
        let s = &component.root_element.borrow().id;
        // Capitalize first letter:
        let mut it = s.chars();
        let id =
            it.next().map(|c| c.to_ascii_uppercase()).into_iter().chain(it).collect::<String>();
        format_ident!("r#{}", id)
    } else {
        format_ident!("r#{}", component.id)
    }
}

fn property_animation_tokens(
    component: &Rc<Component>,
    animation: &ElementRc,
) -> Option<TokenStream> {
    let animation = animation.borrow();
    let bindings = animation.bindings.iter().map(|(prop, initializer)| {
        let prop_ident = format_ident!("r#{}", prop);
        let initializer = compile_expression(initializer, component);
        quote!(#prop_ident: #initializer as _)
    });

    Some(quote!(sixtyfps::re_exports::PropertyAnimation{
        #(#bindings, )*
        ..::core::default::Default::default()
    }))
}

fn property_set_value_tokens(
    component: &Rc<Component>,
    element: &ElementRc,
    property_name: &str,
    value_tokens: TokenStream,
) -> TokenStream {
    match element.borrow().property_animations.get(property_name) {
        Some(crate::object_tree::PropertyAnimation::Static(animation)) => {
            let animation_tokens = property_animation_tokens(component, animation);
            quote!(set_animated_value(#value_tokens, #animation_tokens))
        }
        _ => quote!(set(#value_tokens)),
    }
}

/// Returns the code that can access the given property or callback (but without the set or get)
///
/// to be used like:
/// ```ignore
/// let access = access_member(...)
/// quote!(#access.get())
/// ```
fn access_member(
    element: &ElementRc,
    name: &str,
    component: &Rc<Component>,
    component_rust: TokenStream,
    is_special: bool,
) -> TokenStream {
    let e = element.borrow();

    let enclosing_component = e.enclosing_component.upgrade().unwrap();
    if Rc::ptr_eq(component, &enclosing_component) {
        let inner_component_id = inner_component_id(&enclosing_component);
        let name_ident = format_ident!("r#{}", name);
        if e.property_declarations.contains_key(name) || is_special || component.is_global() {
            let field = access_component_field_offset(&inner_component_id, &name_ident);
            quote!(#field.apply_pin(#component_rust))
        } else if e.is_flickable_viewport {
            let elem_ident = format_ident!(
                "r#{}",
                crate::object_tree::find_parent_element(element).unwrap().borrow().id
            );
            let element_field = access_component_field_offset(&inner_component_id, &elem_ident);

            quote!((#element_field
                + sixtyfps::re_exports::Flickable::FIELD_OFFSETS.viewport
                + sixtyfps::re_exports::Rectangle::FIELD_OFFSETS.#name_ident)
                    .apply_pin(#component_rust)
            )
        } else {
            let elem_ident = format_ident!("r#{}", e.id);
            let elem_ty = format_ident!("r#{}", e.base_type.as_native().class_name);
            let element_field = access_component_field_offset(&inner_component_id, &elem_ident);

            quote!((#element_field + #elem_ty::FIELD_OFFSETS.#name_ident)
                .apply_pin(#component_rust)
            )
        }
    } else if enclosing_component.is_global() {
        let mut root_component = component.clone();
        let mut component_rust = component_rust;
        while let Some(p) = root_component.parent_element.upgrade() {
            root_component = p.borrow().enclosing_component.upgrade().unwrap();
            component_rust = quote!(#component_rust.parent.upgrade().unwrap().as_pin_ref());
        }
        let global_id = format_ident!("global_{}", public_component_id(&enclosing_component));
        let global_comp = quote!(#component_rust.#global_id.as_ref());
        access_member(element, name, &enclosing_component, global_comp, is_special)
    } else {
        access_member(
            element,
            name,
            &component
                .parent_element
                .upgrade()
                .unwrap()
                .borrow()
                .enclosing_component
                .upgrade()
                .unwrap(),
            quote!(#component_rust.parent.upgrade().unwrap().as_pin_ref()),
            is_special,
        )
    }
}

/// Call access_member  for a NamedReference
fn access_named_reference(
    nr: &NamedReference,
    component: &Rc<Component>,
    component_rust: TokenStream,
) -> TokenStream {
    access_member(&nr.element(), nr.name(), component, component_rust, false)
}

fn compile_expression(expr: &Expression, component: &Rc<Component>) -> TokenStream {
    match expr {
        Expression::StringLiteral(s) => quote!(sixtyfps::re_exports::SharedString::from(#s)),
        Expression::NumberLiteral(n, unit) => {
            let n = unit.normalize(*n);
            quote!(#n)
        }
        Expression::BoolLiteral(b) => quote!(#b),
        Expression::Cast { from, to } => {
            let f = compile_expression(&*from, component);
            match (from.ty(), to) {
                (Type::Float32, Type::String) | (Type::Int32, Type::String) => {
                    quote!(sixtyfps::re_exports::SharedString::from(format!("{}", #f).as_str()))
                }
                (Type::Float32, Type::Model) | (Type::Int32, Type::Model) => {
                    quote!(sixtyfps::re_exports::ModelHandle::new(std::rc::Rc::<usize>::new(#f as usize)))
                }
                (Type::Float32, Type::Color) => {
                    quote!(sixtyfps::re_exports::Color::from_argb_encoded(#f as u32))
                }
                (Type::Color, Type::Brush) => {
                    quote!(sixtyfps::Brush::SolidColor(#f))
                }
                (Type::Brush, Type::Color) => {
                    quote!(#f.color())
                }
                (Type::Struct { ref fields, .. }, Type::Component(c)) => {
                    let fields = fields.iter().enumerate().map(|(index, (name, _))| {
                        let index = proc_macro2::Literal::usize_unsuffixed(index);
                        let name = format_ident!("r#{}", name);
                        quote!(#name: obj.#index as _)
                    });
                    let id : TokenStream = c.id.parse().unwrap();
                    quote!({ let obj = #f; #id { #(#fields),*} })
                }
                (Type::Struct { ref fields, .. }, Type::Struct{  name: Some(n), .. }) => {
                    let fields = fields.iter().enumerate().map(|(index, (name, _))| {
                        let index = proc_macro2::Literal::usize_unsuffixed(index);
                        let name = format_ident!("r#{}", name);
                        quote!(#name: obj.#index as _)
                    });
                    let id = struct_name_to_tokens(n);
                    quote!({ let obj = #f; #id { #(#fields),*} })
                }
                _ => f,
            }
        }
        Expression::PropertyReference(nr) => {
            let access = access_named_reference(nr, component, quote!(_self));
            quote!(#access.get())
        }
        Expression::BuiltinFunctionReference(funcref, _) => match funcref {
            BuiltinFunction::GetWindowScaleFactor => {
                quote!(_self.window.scale_factor)
            }
            BuiltinFunction::Debug => quote!((|x| println!("{:?}", x))),
            BuiltinFunction::Mod => quote!((|a1, a2| (a1 as i32) % (a2 as i32))),
            BuiltinFunction::Round => quote!((|a| (a as f64).round())),
            BuiltinFunction::Ceil => quote!((|a| (a as f64).ceil())),
            BuiltinFunction::Floor => quote!((|a| (a as f64).floor())),
            BuiltinFunction::Sqrt => quote!((|a| (a as f64).sqrt())),
            BuiltinFunction::Abs => quote!((|a| (a as f64).abs())),
            BuiltinFunction::Sin => quote!((|a| (a as f64).to_radians().sin())),
            BuiltinFunction::Cos => quote!((|a| (a as f64).to_radians().cos())),
            BuiltinFunction::Tan => quote!((|a| (a as f64).to_radians().tan())),
            BuiltinFunction::ASin => quote!((|a| (a as f64).asin().to_degrees())),
            BuiltinFunction::ACos => quote!((|a| (a as f64).acos().to_degrees())),
            BuiltinFunction::ATan => quote!((|a| (a as f64).atan().to_degrees())),
            BuiltinFunction::SetFocusItem | BuiltinFunction::ShowPopupWindow | BuiltinFunction::ImplicitLayoutInfo(_) => {
                panic!("internal error: should be handled directly in CallFunction")
            }
            BuiltinFunction::StringToFloat => {
                quote!((|x: SharedString| -> f64 { ::core::str::FromStr::from_str(x.as_str()).unwrap_or_default() } ))
            }
            BuiltinFunction::StringIsFloat => {
                quote!((|x: SharedString| { <f64 as ::core::str::FromStr>::from_str(x.as_str()).is_ok() } ))
            }
            BuiltinFunction::ColorBrighter => {
                quote!((|x: Color, factor| -> Color { x.brighter(factor as f32) }))
            }
            BuiltinFunction::ColorDarker => {
                quote!((|x: Color, factor| -> Color { x.darker(factor as f32) }))
            }
            BuiltinFunction::ImageSize => {
                quote!((|x: Image| -> Size { x.size() }))
            }

            BuiltinFunction::Rgb => {
                quote!((|r: i32, g: i32, b: i32, a: f32| {
                    let r: u8 = r.max(0).min(255) as u8;
                    let g: u8 = g.max(0).min(255) as u8;
                    let b: u8 = b.max(0).min(255) as u8;
                    let a: u8 = (255. * a).max(0.).min(255.) as u8;
                    sixtyfps::re_exports::Color::from_argb_u8(a, r, g, b)
                }))
            }
            BuiltinFunction::RegisterCustomFontByPath => {
                panic!("internal error: BuiltinFunction::RegisterCustomFontByPath can only be compiled as part of a FunctionCall expression")
            }
            BuiltinFunction::RegisterCustomFontByMemory => {
                panic!("internal error: BuiltinFunction::RegisterCustomFontByMemory can only be compiled as part of a FunctionCall expression")
            }
        },
        Expression::ElementReference(_) => todo!("Element references are only supported in the context of built-in function calls at the moment"),
        Expression::MemberFunction{ .. } => panic!("member function expressions must not appear in the code generator anymore"),
        Expression::BuiltinMacroReference { .. } => panic!("macro expressions must not appear in the code generator anymore"),
        Expression::RepeaterIndexReference { element } => {
            let access = access_member(
                &element.upgrade().unwrap().borrow().base_type.as_component().root_element,
                "index",
                component,
                quote!(_self),
                true,
            );
            quote!(#access.get())
        }
        Expression::RepeaterModelReference { element } => {
            let access = access_member(
                &element.upgrade().unwrap().borrow().base_type.as_component().root_element,
                "model_data",
                component,
                quote!(_self),
                true,
            );
            quote!(#access.get())
        }
        Expression::FunctionParameterReference { index, .. } => {
            let i = proc_macro2::Literal::usize_unsuffixed(*index);
            quote! {args.#i.clone()}
        }
        Expression::StructFieldAccess { base, name } => match base.ty() {
            Type::Struct { fields, name: None, .. } => {
                let index = fields
                    .keys()
                    .position(|k| k == name)
                    .expect("Expression::ObjectAccess: Cannot find a key in an object");
                let index = proc_macro2::Literal::usize_unsuffixed(index);
                let base_e = compile_expression(base, component);
                quote!((#base_e).#index )
            }
            Type::Struct { .. } => {
                let name = format_ident!("r#{}", name);
                let base_e = compile_expression(base, component);
                quote!((#base_e).#name)
            }
            _ => panic!("Expression::ObjectAccess's base expression is not an Object type"),
        },
        Expression::CodeBlock(sub) => {
            let map = sub.iter().map(|e| compile_expression(e, component));
            quote!({ #(#map);* })
        }
        Expression::CallbackReference(nr) => access_named_reference(
            nr,
            component,
            quote!(_self),
        ),
        Expression::FunctionCall { function, arguments,  source_location: _ } => {
            match &**function {
                Expression::BuiltinFunctionReference(BuiltinFunction::SetFocusItem, _) => {
                    if arguments.len() != 1 {
                        panic!("internal error: incorrect argument count to SetFocusItem call");
                    }
                    if let Expression::ElementReference(focus_item) = &arguments[0] {
                        let focus_item = focus_item.upgrade().unwrap();
                        let focus_item = focus_item.borrow();
                        let item_index = focus_item.item_index.get().unwrap();
                        quote!(
                            _self.window.set_focus_item(&ItemRc::new(VRc::into_dyn(_self.self_weak.get().unwrap().upgrade().unwrap()), #item_index));
                        )
                    } else {
                        panic!("internal error: argument to SetFocusItem must be an element")
                    }
                }
                Expression::BuiltinFunctionReference(BuiltinFunction::ShowPopupWindow, _) => {
                    if arguments.len() != 1 {
                        panic!("internal error: incorrect argument count to ShowPopupWindow call");
                    }
                    if let Expression::ElementReference(popup_window) = &arguments[0] {
                        let popup_window = popup_window.upgrade().unwrap();
                        let pop_comp = popup_window.borrow().enclosing_component.upgrade().unwrap();
                        let popup_window_id = inner_component_id(&pop_comp);
                        let parent_component = pop_comp.parent_element.upgrade().unwrap().borrow().enclosing_component.upgrade().unwrap();
                        let popup_list = parent_component.popup_windows.borrow();
                        let popup = popup_list.iter().find(|p| Rc::ptr_eq(&p.component, &pop_comp)).unwrap();
                        let x = access_named_reference(&popup.x, component, quote!(_self));
                        let y = access_named_reference(&popup.y, component, quote!(_self));
                        quote!(
                            _self.window.show_popup(
                                &VRc::into_dyn(#popup_window_id::new(_self.self_weak.get().unwrap().clone(), &_self.window).into()),
                                Point::new(#x.get(), #y.get())
                            );
                        )
                    } else {
                        panic!("internal error: argument to SetFocusItem must be an element")
                    }
                }
                Expression::BuiltinFunctionReference(BuiltinFunction::ImplicitLayoutInfo(orient), _) => {
                    if arguments.len() != 1 {
                        panic!("internal error: incorrect argument count to ImplicitLayoutInfo call");
                    }
                    if let Expression::ElementReference(item) = &arguments[0] {
                        let item = item.upgrade().unwrap();
                        let item = item.borrow();
                        let item_id = format_ident!("r#{}", item.id);
                        let item_field = access_component_field_offset(&format_ident!("Self"), &item_id);
                        quote!(
                            #item_field.apply_pin(_self).layouting_info(#orient, &_self.window)
                        )
                    } else {
                        panic!("internal error: argument to ImplicitLayoutInfo must be an element")
                    }
                }
                Expression::BuiltinFunctionReference(BuiltinFunction::RegisterCustomFontByPath, _) => {
                    if arguments.len() != 1 {
                        panic!("internal error: incorrect argument count to RegisterCustomFontByPath call");
                    }
                    if let Expression::StringLiteral(path) = &arguments[0] {
                        quote!(sixtyfps::register_font_from_path(&std::path::PathBuf::from(#path));)
                    } else {
                        panic!("internal error: argument to RegisterCustomFontByPath must be a string literal")
                    }
                }
                Expression::BuiltinFunctionReference(BuiltinFunction::RegisterCustomFontByMemory, _) => {
                    if arguments.len() != 1 {
                        panic!("internal error: incorrect argument count to RegisterCustomFontByMemory call");
                    }
                    if let Expression::NumberLiteral(resource_id, _) = &arguments[0] {
                        let resource_id: usize = *resource_id as _;
                        let symbol = format_ident!("SFPS_EMBEDDED_RESOURCE_{}", resource_id);
                        quote!(sixtyfps::register_font_from_memory(#symbol.into());)
                    } else {
                        panic!("internal error: argument to RegisterCustomFontByMemory must be a number")
                    }
                }
                _ => {
                    let f = compile_expression(function, component);
                    let a = arguments.iter().map(|a| compile_expression(a, component));
                    let function_type = function.ty();
                    match function_type {
                         Type::Callback { args, .. } => {
                            let cast = args.iter().map(|ty| match ty {
                                Type::Bool => quote!(as bool),
                                Type::Int32 => quote!(as i32),
                                Type::Float32 => quote!(as f32),
                                _ => quote!(.clone()),
                            });
                            quote! { #f.call(&(#((#a)#cast,)*).into())}
                        }
                        Type::Function {args, .. } => {
                            let cast = args.iter().map(|ty| match ty {
                                Type::Bool => quote!(as bool),
                                Type::Int32 => quote!(as i32),
                                Type::Float32 => quote!(as f32),
                                _ => quote!(.clone()),
                            });
                            quote! { #f(#((#a) #cast),*)}
                        }
                        _ => panic!("not calling a function")
                    }
                }
            }

        }
        Expression::SelfAssignment { lhs, rhs, op } => {
            let rhs = compile_expression(&*rhs, component);
            compile_assignment(lhs, *op, rhs, component)
        }
        Expression::BinaryExpression { lhs, rhs, op } => {
            let (conv1, conv2) = match crate::expression_tree::operator_class(*op) {
                OperatorClass::ArithmeticOp => match lhs.ty() {
                    Type::String => (None, Some(quote!(.as_str()))),
                    Type::Struct{..} => (None, None),
                    _ => (Some(quote!(as f64)), Some(quote!(as f64))),
                },
                OperatorClass::ComparisonOp
                    if matches!(
                        lhs.ty(),
                        Type::Int32
                            | Type::Float32
                            | Type::Duration
                            | Type::PhysicalLength
                            | Type::LogicalLength
                            | Type::Angle
                    ) =>
                {
                    (Some(quote!(as f64)), Some(quote!(as f64)))
                }
                _ => (None, None),
            };
            let lhs = compile_expression(&*lhs, component);
            let rhs = compile_expression(&*rhs, component);

            let op = match op {
                '=' => quote!(==),
                '!' => quote!(!=),
                '≤' => quote!(<=),
                '≥' => quote!(>=),
                '&' => quote!(&&),
                '|' => quote!(||),
                _ => proc_macro2::TokenTree::Punct(proc_macro2::Punct::new(
                    *op,
                    proc_macro2::Spacing::Alone,
                ))
                .into(),
            };
            quote!( ((#lhs #conv1 ) #op (#rhs #conv2)) )
        }
        Expression::UnaryOp { sub, op } => {
            let sub = compile_expression(&*sub, component);
            if *op == '+' {
                // there is no unary '+' in rust
                return sub;
            }
            let op = proc_macro2::Punct::new(*op, proc_macro2::Spacing::Alone);
            quote!( #op #sub )
        }
        Expression::ImageReference(resource_ref) => {
            match resource_ref {
                crate::expression_tree::ImageReference::None => {
                    quote!(sixtyfps::re_exports::Image::default())
                }
                crate::expression_tree::ImageReference::AbsolutePath(path) => {
                     quote!(sixtyfps::re_exports::Image::load_from_path(sixtyfps::re_exports::SharedString::from(#path)))
                },
                crate::expression_tree::ImageReference::EmbeddedData(resource_id) => {
                    let symbol = format_ident!("SFPS_EMBEDDED_RESOURCE_{}", resource_id);
                    quote!(
                        sixtyfps::re_exports::Image::from(
                            sixtyfps::re_exports::ImageInner::EmbeddedData(#symbol.into())
                        )
                    )
                }
            }
        }
        Expression::Condition { condition, true_expr, false_expr } => {
            let condition_code = compile_expression(&*condition, component);
            let true_code = compile_expression(&*true_expr, component);
            let false_code = compile_expression(&*false_expr, component);
            quote!(
                if #condition_code {
                    #true_code
                } else {
                    (#false_code) as _
                }
            )
        }
        Expression::Invalid | Expression::Uncompiled(_) | Expression::TwoWayBinding(..) => {
            let error = format!("unsupported expression {:?}", expr);
            quote!(compile_error! {#error})
        }
        Expression::Array { values, element_ty } => {
            let rust_element_ty = rust_type(element_ty).unwrap();
            let val = values.iter().map(|e| compile_expression(e, component));
            quote!(sixtyfps::re_exports::ModelHandle::new(
                std::rc::Rc::new(sixtyfps::re_exports::VecModel::<#rust_element_ty>::from(vec![#(#val as _),*]))
            ))
        }
        Expression::Struct { ty, values } => {
            if let Type::Struct { fields, name, .. } = ty {
                let elem = fields.iter().map(|(k, t)| {
                    values.get(k).map(|e| {
                        let ce = compile_expression(e, component);
                        let t = rust_type(t).unwrap_or_default();
                        quote!(#ce as #t)
                    })
                });
                if let Some(name) = name {
                    let name : TokenStream = struct_name_to_tokens(name.as_str());
                    let keys = fields.keys().map(|k| k.parse::<TokenStream>().unwrap());
                    quote!(#name { #(#keys: #elem,)* })
                } else {
                    // This will produce a tuple
                    quote!((#(#elem,)*))
                }
            } else {
                panic!("Expression::Object is not a Type::Object")
            }
        }
        Expression::PathElements { elements } => compile_path(elements, component),
        Expression::StoreLocalVariable { name, value } => {
            let value = compile_expression(value, component);
            let name = format_ident!("r#{}", name);
            quote!(let #name = #value;)
        }
        Expression::ReadLocalVariable { name, .. } => {
            let name = format_ident!("r#{}", name);
            quote!(#name)
        }
        Expression::EasingCurve(EasingCurve::Linear) => {
            quote!(sixtyfps::re_exports::EasingCurve::Linear)
        }
        Expression::EasingCurve(EasingCurve::CubicBezier(a, b, c, d)) => {
            quote!(sixtyfps::re_exports::EasingCurve::CubicBezier([#a, #b, #c, #d]))
        }
        Expression::LinearGradient{angle, stops} => {
            let angle = compile_expression(angle, component);
            let stops = stops.iter().map(|(color, stop)| {
                let color = compile_expression(color, component);
                let position = compile_expression(stop, component);
                quote!(sixtyfps::re_exports::GradientStop{ color: #color, position: #position as _ })
            });
            quote!(sixtyfps::Brush::LinearGradient(
                sixtyfps::re_exports::LinearGradientBrush::new(#angle as _, [#(#stops),*].iter().cloned())
            ))
        }
        Expression::EnumerationValue(value) => {
            let base_ident = format_ident!("r#{}", value.enumeration.name);
            let value_ident = format_ident!("r#{}", value.to_string());
            quote!(sixtyfps::re_exports::#base_ident::#value_ident)
        }
        Expression::ReturnStatement(expr) => {
            let return_expr = expr.as_ref().map(|expr| compile_expression(expr, component));
            quote!(return (#return_expr) as _;)
        },
        Expression::LayoutCacheAccess { layout_cache_prop, index, repeater_index } => {
            let cache = access_named_reference(layout_cache_prop, component, quote!(_self));
            if let Some(ri) = repeater_index {
                let offset = compile_expression(ri, component);
                quote!({
                    let cache = #cache.get();
                    cache[(cache[#index] as usize) + #offset as usize * 2]
                })
            } else {
                quote!(#cache.get()[#index])
            }
        }
        Expression::ComputeLayoutInfo(Layout::GridLayout(layout), o) => {
            let (padding, spacing) = generate_layout_padding_and_spacing(&layout.geometry, *o, component);
            let cells = grid_layout_cell_data(layout, *o, component);
            quote!(grid_layout_info(Slice::from_slice(&#cells), #spacing, #padding))
        }
        Expression::ComputeLayoutInfo(Layout::BoxLayout(layout), o) => {
            let (padding, spacing) = generate_layout_padding_and_spacing(&layout.geometry,*o, component);
            let (cells, alignment) = box_layout_data(layout, *o, component, None);
            if *o == layout.orientation {
                quote!(box_layout_info(Slice::from_slice(&#cells), #spacing, #padding, #alignment))
            } else {
                quote!(box_layout_info_ortho(Slice::from_slice(&#cells), #padding))
            }
        }
        Expression::ComputeLayoutInfo(Layout::PathLayout(_), _) => unimplemented!(),
        Expression::SolveLayout(Layout::GridLayout(layout), o) => {
            let (padding, spacing) = generate_layout_padding_and_spacing(&layout.geometry, *o, component);
            let cells = grid_layout_cell_data(layout, *o, component);
            let size = layout_geometry_size(&layout.geometry.rect, *o, component);
            quote!(solve_grid_layout(&GridLayoutData{
                size: #size,
                spacing: #spacing,
                padding: #padding,
                cells: Slice::from_slice(&#cells),
            }))
        }
        Expression::SolveLayout(Layout::BoxLayout(layout), o) => {
            let (padding, spacing) = generate_layout_padding_and_spacing(&layout.geometry, *o, component);
            let mut repeated_indices = Default::default();
            let mut repeated_indices_init = Default::default();
            let (cells, alignment) = box_layout_data(layout, *o, component, Some((&mut repeated_indices, &mut repeated_indices_init)));
            let size = layout_geometry_size(&layout.geometry.rect, *o, component);
            quote!({
                #repeated_indices_init
                solve_box_layout(
                    &BoxLayoutData {
                        size: #size,
                        spacing: #spacing,
                        padding: #padding,
                        alignment: #alignment,
                        cells: Slice::from_slice(&#cells),
                    },
                    Slice::from_slice(&#repeated_indices),
                )
            })
        }
        Expression::SolveLayout(Layout::PathLayout(layout), _) => {
            let width = layout_geometry_size(&layout.rect,  Orientation::Horizontal, component);
            let height = layout_geometry_size(&layout.rect,  Orientation::Vertical, component);
            let elements = compile_path(&layout.path, component);
            let get_prop = |nr: &Option<NamedReference>| {
                nr.as_ref().map_or_else(
                    || quote!(::core::default::Default::default()),
                    |nr| {
                        let p = access_named_reference(nr, component, quote!(_self));
                        quote!(#p.get())
                    },
                )
            };
            let offset = get_prop(&layout.offset_reference);
            let count = layout.elements.len(); // FIXME! repeater
            quote!(
                solve_path_layout(
                    &PathLayoutData {
                        width: #width,
                        height: #height,
                        x: 0.,
                        y: 0.,
                        elements: &#elements,
                        offset: #offset,
                        item_count: #count as _,
                    },
                    Slice::from_slice(&[]),
                )
            )
        }
    }
}

/// Return a TokenStream for a name (as in [`Type::Struct::name`])
fn struct_name_to_tokens(name: &str) -> TokenStream {
    // the name match the C++ signature so we need to change that to the rust namespace
    let mut name = name.replace("::private_api::", "::re_exports::");
    if !name.contains("::") {
        name.insert_str(0, "r#")
    }
    name.parse().unwrap()
}

fn compile_assignment(
    lhs: &Expression,
    op: char,
    rhs: TokenStream,
    component: &Rc<Component>,
) -> TokenStream {
    match lhs {
        Expression::PropertyReference(nr) => {
            let lhs_ = access_named_reference(nr, component, quote!(_self));
            if op == '=' {
                quote!( #lhs_.set((#rhs) as _) )
            } else {
                let op = proc_macro2::Punct::new(op, proc_macro2::Spacing::Alone);
                if lhs.ty() == Type::String {
                    quote!( #lhs_.set(#lhs_.get() #op #rhs.as_str()) )
                } else {
                    quote!( #lhs_.set(((#lhs_.get() as f64) #op (#rhs as f64)) as _) )
                }
            }
        }
        Expression::StructFieldAccess { base, name } => {
            let tmpobj = quote!(tmpobj);
            let get_obj = compile_expression(base, component);
            let ty = base.ty();
            let (member, member_ty) = match &ty {
                Type::Struct { fields, name: None, .. } => {
                    let index = fields
                        .keys()
                        .position(|k| k == name)
                        .expect("Expression::ObjectAccess: Cannot find a key in an object");
                    let index = proc_macro2::Literal::usize_unsuffixed(index);
                    (quote!(#index), fields[name].clone())
                }
                Type::Struct { fields, name: Some(_), .. } => {
                    let n = format_ident!("r#{}", name);
                    (quote!(#n), fields[name].clone())
                }
                _ => panic!("Expression::ObjectAccess's base expression is not an Object type"),
            };

            let conv = if member_ty == Type::String {
                if op == '=' {
                    quote!()
                } else {
                    quote!(.as_str())
                }
            } else {
                let member_ty = rust_type(&member_ty).unwrap_or_default();
                quote!(as #member_ty)
            };

            let op = match op {
                '+' => quote!(+=),
                '*' => quote!(*=),
                '-' => quote!(-=),
                '/' => quote!(/=),
                '=' => quote!(=),
                _ => panic!("Unknown assignment op {:?}", op),
            };

            let new_value = quote!({
               let mut #tmpobj = #get_obj;
               #tmpobj.#member #op (#rhs #conv);
               #tmpobj
            });
            compile_assignment(base, '=', new_value, component)
        }
        Expression::RepeaterModelReference { element } => {
            let element = element.upgrade().unwrap();
            let parent_component = element.borrow().base_type.as_component().clone();
            let repeater_access = access_member(
                &parent_component
                    .parent_element
                    .upgrade()
                    .unwrap()
                    .borrow()
                    .enclosing_component
                    .upgrade()
                    .unwrap()
                    .root_element,
                &format!("repeater_{}", element.borrow().id),
                component,
                quote!(_self),
                true,
            );
            let index_access = access_member(
                &parent_component.root_element,
                "index",
                component,
                quote!(_self),
                true,
            );
            if op == '=' {
                quote!(#repeater_access.model_set_row_data(#index_access.get(), #rhs as _))
            } else {
                let op = proc_macro2::Punct::new(op, proc_macro2::Spacing::Alone);
                let old_data = compile_expression(lhs, component);
                if lhs.ty() == Type::String {
                    quote!(#repeater_access.model_set_row_data(#index_access.get(), #old_data #op &#rhs))
                } else {
                    quote!(#repeater_access.model_set_row_data(#index_access.get(), ((#old_data as f64) #op (#rhs as f64)) as _))
                }
            }
        }
        _ => panic!("typechecking should make sure this was a PropertyReference"),
    }
}

fn grid_layout_cell_data(
    layout: &crate::layout::GridLayout,
    orientation: Orientation,
    component: &Rc<Component>,
) -> TokenStream {
    let cells = layout.elems.iter().map(|c| {
        let (col_or_row, span) = c.col_or_row_and_span(orientation);
        let layout_info =
            get_layout_info(&c.item.element, component, &c.item.constraints, orientation);
        quote!(GridLayoutCellData {
            col_or_row: #col_or_row,
            span: #span,
            constraint: #layout_info,
        })
    });
    quote!([ #(#cells),* ])
}

/// Returns `(cells, alignment)`.
/// The repeated_indices initialize the repeated_indices (var, init_code)
fn box_layout_data(
    layout: &crate::layout::BoxLayout,
    orientation: Orientation,
    component: &Rc<Component>,
    mut repeated_indices: Option<(&mut TokenStream, &mut TokenStream)>,
) -> (TokenStream, TokenStream) {
    let alignment = if let Some(expr) = &layout.geometry.alignment {
        let p = access_named_reference(expr, component, quote!(_self));
        quote!(#p.get())
    } else {
        quote!(::core::default::Default::default())
    };

    let repeater_count =
        layout.elems.iter().filter(|i| i.element.borrow().repeated.is_some()).count();

    if repeater_count == 0 {
        let cells = layout.elems.iter().map(|li| {
            let layout_info = get_layout_info(&li.element, component, &li.constraints, orientation);
            quote!(BoxLayoutCellData { constraint: #layout_info })
        });
        if let Some((ri, _)) = &mut repeated_indices {
            **ri = quote!([]);
        }
        (quote!([ #(#cells),* ]), alignment)
    } else {
        let mut fixed_count = 0usize;
        let mut repeated_count = quote!();
        let mut push_code = quote!();
        let inner_component_id = inner_component_id(component);
        if let Some((ri, init)) = &mut repeated_indices {
            **ri = quote!(repeater_indices);
            **init = quote!( let mut #ri = [ 0u32; #repeater_count * 2]; );
        }
        let mut repeater_idx = 0usize;
        for item in &layout.elems {
            if item.element.borrow().repeated.is_some() {
                let repeater_id = format_ident!("repeater_{}", item.element.borrow().id);
                let rep_inner_component_id =
                    self::inner_component_id(item.element.borrow().base_type.as_component());
                repeated_count = quote!(#repeated_count + _self.#repeater_id.len());
                let ri = repeated_indices.as_ref().map(|(ri, _)| {
                    quote!(
                        #ri[#repeater_idx * 2] = items_vec.len() as u32;
                        #ri[#repeater_idx * 2 + 1] = internal_vec.len() as u32;
                    )
                });
                repeater_idx += 1;
                push_code = quote! {
                    #push_code
                    #inner_component_id::FIELD_OFFSETS.#repeater_id.apply_pin(_self).ensure_updated(
                        || { #rep_inner_component_id::new(_self.self_weak.get().unwrap().clone(), &_self.window).into() }
                    );
                    let internal_vec = _self.#repeater_id.components_vec();
                    #ri
                    for sub_comp in &internal_vec {
                        items_vec.push(sub_comp.as_pin_ref().box_layout_data(#orientation))
                    }
                }
            } else {
                let layout_info =
                    get_layout_info(&item.element, component, &item.constraints, orientation);
                fixed_count += 1;
                push_code = quote! {
                    #push_code
                    items_vec.push(BoxLayoutCellData { constraint: #layout_info });
                }
            }
        }
        (
            quote! { {
                let mut items_vec = Vec::with_capacity(#fixed_count #repeated_count);
                #push_code
                items_vec
            } },
            alignment,
        )
    }
}

fn generate_layout_padding_and_spacing(
    layout_geometry: &LayoutGeometry,
    orientation: Orientation,
    component: &Rc<Component>,
) -> (TokenStream, TokenStream) {
    let padding_prop = |expr| {
        if let Some(expr) = expr {
            let p = access_named_reference(expr, component, quote!(_self));
            quote!(#p.get())
        } else {
            quote!(0.)
        }
    };
    let spacing = padding_prop(layout_geometry.spacing.as_ref());
    let (begin, end) = layout_geometry.padding.begin_end(orientation);
    let (begin, end) = (padding_prop(begin), padding_prop(end));
    let padding = quote!(&sixtyfps::re_exports::Padding { begin: #begin, end: #end });

    (padding, spacing)
}

fn layout_geometry_size(
    rect: &LayoutRect,
    orientation: Orientation,
    component: &Rc<Component>,
) -> TokenStream {
    let nr = rect.size_reference(orientation);
    nr.map_or_else(
        || quote!(::core::default::Default::default()),
        |nr| {
            let p = access_named_reference(nr, component, quote!(_self));
            quote!(#p.get())
        },
    )
}

fn compute_layout(component: &Rc<Component>) -> TokenStream {
    let elem = &component.root_element;
    let constraints = component.root_constraints.borrow();
    let layout_info_h = get_layout_info(elem, component, &constraints, Orientation::Horizontal);
    let layout_info_v = get_layout_info(elem, component, &constraints, Orientation::Vertical);
    quote! {
        fn layout_info(self: ::core::pin::Pin<&Self>, orientation: sixtyfps::re_exports::Orientation) -> sixtyfps::re_exports::LayoutInfo {
            #![allow(unused)]
            use sixtyfps::re_exports::*;
            let _self = self;
            match orientation {
                sixtyfps::re_exports::Orientation::Horizontal => #layout_info_h,
                sixtyfps::re_exports::Orientation::Vertical => #layout_info_v,
            }
        }
    }
}

fn get_layout_info(
    elem: &ElementRc,
    component: &Rc<Component>,
    constraints: &crate::layout::LayoutConstraints,
    orientation: Orientation,
) -> TokenStream {
    let layout_info = if let Some(layout_info_prop) = &elem.borrow().layout_info_prop(orientation) {
        let li = access_named_reference(layout_info_prop, component, quote!(_self));
        quote! {#li.get()}
    } else {
        let elem_id = format_ident!("r#{}", elem.borrow().id);
        let inner_component_id = inner_component_id(component);
        quote!(#inner_component_id::FIELD_OFFSETS.#elem_id.apply_pin(_self).layouting_info(#orientation, &_self.window))
    };

    if constraints.has_explicit_restrictions() {
        let (name, expr): (Vec<_>, Vec<_>) = constraints
            .for_each_restrictions(orientation)
            .map(|(e, s)| {
                (format_ident!("r#{}", s), access_named_reference(e, component, quote!(_self)))
            })
            .unzip();
        quote!({
            let mut layout_info = #layout_info;
                #(layout_info.#name = #expr.get();)*
            layout_info
        })
    } else {
        layout_info
    }
}

fn compile_path_events(events: &crate::expression_tree::PathEvents) -> TokenStream {
    use lyon_path::Event;

    let mut coordinates = Vec::new();

    let converted_events: Vec<proc_macro2::TokenStream> = events
        .iter()
        .map(|event| match event {
            Event::Begin { at } => {
                coordinates.push(at);
                quote!(sixtyfps::re_exports::PathEvent::Begin)
            }
            Event::Line { from, to } => {
                coordinates.push(from);
                coordinates.push(to);
                quote!(sixtyfps::re_exports::PathEvent::Line)
            }
            Event::Quadratic { from, ctrl, to } => {
                coordinates.push(from);
                coordinates.push(ctrl);
                coordinates.push(to);
                quote!(sixtyfps::re_exports::PathEvent::Quadratic)
            }
            Event::Cubic { from, ctrl1, ctrl2, to } => {
                coordinates.push(from);
                coordinates.push(ctrl1);
                coordinates.push(ctrl2);
                coordinates.push(to);
                quote!(sixtyfps::re_exports::PathEvent::Cubic)
            }
            Event::End { close, .. } => {
                if *close {
                    quote!(sixtyfps::re_exports::PathEvent::EndClosed)
                } else {
                    quote!(sixtyfps::re_exports::PathEvent::EndOpen)
                }
            }
        })
        .collect();

    let coordinates: Vec<TokenStream> = coordinates
        .into_iter()
        .map(|pt| {
            let x = pt.x;
            let y = pt.y;
            quote!(sixtyfps::re_exports::Point::new(#x, #y))
        })
        .collect();

    quote!(sixtyfps::re_exports::SharedVector::<sixtyfps::re_exports::PathEvent>::from_slice(&[#(#converted_events),*]),
           sixtyfps::re_exports::SharedVector::<sixtyfps::re_exports::Point>::from_slice(&[#(#coordinates),*]))
}

fn compile_path(path: &Path, component: &Rc<Component>) -> TokenStream {
    match path {
        Path::Elements(elements) => {
            let converted_elements: Vec<TokenStream> = elements
                .iter()
                .map(|element| {
                    let mut bindings = element
                        .bindings
                        .iter()
                        .map(|(property, expr)| {
                            let prop_ident = format_ident!("r#{}", property);
                            let binding_expr = compile_expression(expr, component);

                            quote!(#prop_ident: #binding_expr as _).to_string()
                        })
                        .collect::<Vec<String>>();

                    if bindings.len() < element.element_type.properties.len() {
                        bindings.push("..Default::default()".into())
                    }

                    let bindings = bindings.join(",");

                    let ctor_format_string = element
                        .element_type
                        .native_class.rust_type_constructor
                        .as_ref()
                        .expect(
                        "Unexpected error in type registry: path element is lacking rust type name",
                    );

                    ctor_format_string
                        .replace("{}", &bindings)
                        .parse()
                        .expect("Error parsing rust path element constructor")
                })
                .collect();

            quote!(sixtyfps::re_exports::PathData::Elements(
                sixtyfps::re_exports::SharedVector::<sixtyfps::re_exports::PathElement>::from_slice(&[#(#converted_elements),*])
            ))
        }
        Path::Events(events) => {
            let events = compile_path_events(events);
            quote!(sixtyfps::re_exports::PathData::Events(#events))
        }
    }
}

// In Rust debug builds, accessing the member of the FIELD_OFFSETS ends up copying the
// entire FIELD_OFFSETS into a new stack allocation, which with large property
// binding initialization functions isn't re-used and with large generated inner
// components ends up large amounts of stack space (see issue #133)
fn access_component_field_offset(component_id: &Ident, field: &Ident) -> TokenStream {
    quote!({ *&#component_id::FIELD_OFFSETS.#field })
}
