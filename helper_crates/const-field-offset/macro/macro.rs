/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This crate allow to get the offset of a field of a structure in a const or static context.

To be used re-exported from the `const_field_offset` crate

*/
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, DeriveInput};
#[cfg(feature = "field-offset-trait")]
use syn::{VisRestricted, Visibility};

/**

The macro FieldOffsets adds a `FIELD_OFFSETS` associated const to the struct. That
is an object which has fields with the same name as the fields of the original struct,
each field is of type `const_field_offset::FieldOffset`

```rust
use const_field_offset::FieldOffsets;
#[repr(C)]
#[derive(FieldOffsets)]
struct Foo {
    field_1 : u8,
    field_2 : u32,
}

const FOO : usize = Foo::FIELD_OFFSETS.field_2.get_byte_offset();
assert_eq!(FOO, 4);

// This would not work on stable rust at the moment (rust 1.43)
// const FOO : usize = memoffsets::offsetof!(Foo, field_2);
```

*/
#[cfg_attr(
    feature = "field-offset-trait",
    doc = "
In addition, the macro also create a module `{ClassName}_field_offsets` which contains
zero-sized type that implement the `const_field_offset::ConstFieldOffset` trait

```rust
use const_field_offset::{FieldOffsets, FieldOffset, ConstFieldOffset};
#[repr(C)]
#[derive(FieldOffsets)]
struct Foo {
    field_1 : u8,
    field_2 : u32,
}

const FOO : FieldOffset<Foo, u32> = Foo_field_offsets::field_2::OFFSET;
assert_eq!(FOO.get_byte_offset(), 4);
```
"
)]
/**

## Limitations

Only work with named #[repr(C)] structures.

## Attributes

### `pin`

Add a `AllowPin` to the FieldOffset.

In order for this to be safe, the macro will add code to prevent a
custom `Drop` or `Unpin` implementation.

```rust
use const_field_offset::*;
#[repr(C)]
#[derive(FieldOffsets)]
#[pin]
struct Foo {
    field_1 : u8,
    field_2 : u32,
}

const FIELD_2 : FieldOffset<Foo, u32, AllowPin> = Foo::FIELD_OFFSETS.field_2;
let pin_box = Box::pin(Foo{field_1: 1, field_2: 2});
assert_eq!(*FIELD_2.apply_pin(pin_box.as_ref()), 2);
```

### `pin_drop`

This attribute works like the `pin` attribute but it does not prevent a custom
Drop implementation. Instead it provides a Drop implementation that forwards to
the [PinnedDrop](../const_field_offset/trait.PinnedDrop.html) trait that you need to implement for our type.

```rust
use const_field_offset::*;
use core::pin::Pin;

struct TypeThatRequiresSpecialDropHandling(); // ...

#[repr(C)]
#[derive(FieldOffsets)]
#[pin_drop]
struct Foo {
    field : TypeThatRequiresSpecialDropHandling,
}

impl PinnedDrop for Foo {
    fn drop(self: Pin<&mut Self>) {
        // Do you safe drop handling here
    }
}
```

### `const-field-offset`

In case the `const-field-offset` crate is re-exported, it is possible to
specify the crate name using the `const_field_offset` attribute.

```rust
// suppose you re-export the const_field_offset create from a different module
mod xxx { pub use const_field_offset as cfo; }
#[repr(C)]
#[derive(xxx::cfo::FieldOffsets)]
#[const_field_offset(xxx::cfo)]
struct Foo {
    field_1 : u8,
    field_2 : u32,
}
```

*/
#[proc_macro_derive(FieldOffsets, attributes(const_field_offset, pin, pin_drop))]
pub fn const_field_offset(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let mut has_repr_c = false;
    let mut crate_ = quote!(const_field_offset);
    let mut pin = false;
    let mut drop = false;
    for a in &input.attrs {
        if let Some(i) = a.path.get_ident() {
            if i == "repr" {
                match a.tokens.to_string().as_str() {
                    "(C)" => has_repr_c = true,
                    "(packed)" => {
                        return TokenStream::from(quote!(
                            compile_error! {"FieldOffsets does not work on #[repr(packed)]"}
                        ))
                    }
                    _ => (),
                }
            } else if i == "const_field_offset" {
                let mut token_it = a.tokens.clone().into_iter();
                if let (Some(proc_macro2::TokenTree::Group(g)), None) =
                    (token_it.next(), token_it.next())
                {
                    if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
                        crate_ = g.stream();
                        continue;
                    }
                }
                return TokenStream::from(
                    quote_spanned! {a.span() => compile_error!{"const_field_offset attribute must be a crate name"}},
                );
            } else if i == "pin" {
                pin = true;
            } else if i == "pin_drop" {
                drop = true;
                pin = true;
            }
        }
    }
    if !has_repr_c {
        return TokenStream::from(
            quote! {compile_error!{"FieldOffsets only work for structures using repr(C)"}},
        );
    }

    let struct_name = input.ident;
    let field_struct_name = quote::format_ident!("{}FieldsOffsets", struct_name);

    let (fields, types, vis) = if let syn::Data::Struct(s) = &input.data {
        if let syn::Fields::Named(n) = &s.fields {
            let (f, tv): (Vec<_>, Vec<_>) =
                n.named.iter().map(|f| (&f.ident, (&f.ty, &f.vis))).unzip();
            let (t, v): (Vec<_>, Vec<_>) = tv.into_iter().unzip();
            (f, t, v)
        } else {
            return TokenStream::from(quote! {compile_error!{"Only work for named fields"}});
        }
    } else {
        return TokenStream::from(quote! {compile_error!("Only work for struct")});
    };

    let doc = format!(
        "Helper struct containing the offsets of the fields of the struct `{}`",
        struct_name
    );

    let (ensure_pin_safe, ensure_no_unpin, pin_flag, new_from_offset) = if !pin {
        (None, None, quote!(#crate_::NotPinned), quote!(new_from_offset))
    } else {
        (
            if drop {
                None
            } else {
                let drop_trait_ident = format_ident!("{}MustNotImplDrop", struct_name);
                Some(quote! {
                    /// Make sure that Drop is not implemented
                    #[allow(non_camel_case_types)]
                    trait #drop_trait_ident {}
                    impl<T: ::core::ops::Drop> #drop_trait_ident for T {}
                    impl #drop_trait_ident for #struct_name {}

                })
            },
            Some(quote! {
                    /// Make sure that Unpin is not implemented
                    pub struct __MustNotImplUnpin<'__dummy_lifetime> (
                        #(#types, )*
                        ::core::marker::PhantomData<&'__dummy_lifetime ()>
                    );
                    impl<'__dummy_lifetime> Unpin for #struct_name where __MustNotImplUnpin<'__dummy_lifetime> : Unpin {};
            }),
            quote!(#crate_::AllowPin),
            quote!(new_from_offset_pinned),
        )
    };

    let pinned_drop_impl = if drop {
        Some(quote!(
            impl Drop for #struct_name {
                fn drop(&mut self) {
                    use #crate_::PinnedDrop;
                    self.do_safe_pinned_drop();
                }
            }
        ))
    } else {
        None
    };

    // Build the output, possibly using quasi-quotation
    let expanded = quote! {
        #[doc = #doc]
        ///
        /// Generated from the derive macro `const-field-offset::FieldOffsets`
        #[allow(missing_docs, non_camel_case_types)]
        pub struct #field_struct_name {
            #(#vis #fields : #crate_::FieldOffset<#struct_name, #types, #pin_flag>,)*
        }

        #[allow(clippy::eval_order_dependence)] // The point of this code is to depend on the order!
        impl #struct_name {
            /// Return a struct containing the offset of for the fields of this struct
            pub const FIELD_OFFSETS : #field_struct_name = {
                #ensure_pin_safe;
                #ensure_no_unpin;
                let mut len = 0usize;
                #field_struct_name {
                    #( #fields : {
                        let align = ::core::mem::align_of::<#types>();
                        // from Layout::padding_needed_for which is not yet stable
                        let len_rounded_up  = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
                        len = len_rounded_up + ::core::mem::size_of::<#types>();
                        /// Safety: According to the rules of repr(C), this is the right offset
                        unsafe { #crate_::FieldOffset::<#struct_name, #types, _>::#new_from_offset(len_rounded_up) }
                    }, )*
                }
            };
        }

        #pinned_drop_impl
    };

    #[cfg(feature = "field-offset-trait")]
    let module_name = quote::format_ident!("{}_field_offsets", struct_name);

    #[cfg(feature = "field-offset-trait")]
    let in_mod_vis = vis.iter().map(|vis| match vis {
        Visibility::Public(_) => quote! {#vis},
        Visibility::Crate(_) => quote! {#vis},
        Visibility::Restricted(VisRestricted { pub_token, path, .. }) => {
            if quote!(#path).to_string().starts_with("super") {
                quote!(#pub_token(in super::#path))
            } else {
                quote!(#vis)
            }
        }
        Visibility::Inherited => quote!(pub(super)),
    });

    #[cfg(feature = "field-offset-trait")]
    let expanded = quote! { #expanded
        #[allow(non_camel_case_types)]
        #[allow(non_snake_case)]
        #[allow(missing_docs)]
        pub mod #module_name {
            #(
                #[derive(Clone, Copy, Default)]
                #in_mod_vis struct #fields;
            )*
        }
        #(
            impl #crate_::ConstFieldOffset for #module_name::#fields {
                type Container = #struct_name;
                type Field = #types;
                type PinFlag = #pin_flag;
                const OFFSET : #crate_::FieldOffset<#struct_name, #types, Self::PinFlag>
                    = #struct_name::FIELD_OFFSETS.#fields;
            }
            impl ::core::convert::Into<#crate_::FieldOffset<#struct_name, #types, #pin_flag>> for #module_name::#fields {
                fn into(self) -> #crate_::FieldOffset<#struct_name, #types, #pin_flag> {
                    #struct_name::FIELD_OFFSETS.#fields
                }
            }
            impl<Other> ::core::ops::Add<Other> for #module_name::#fields
                where Other : #crate_::ConstFieldOffset<Container = #types>
            {
                type Output = #crate_::ConstFieldOffsetSum<Self, Other>;
                fn add(self, other: Other) -> Self::Output {
                    #crate_::ConstFieldOffsetSum(self, other)
                }
            }
        )*
    };

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}

/**
```compile_fail
use const_field_offset::*;
#[derive(FieldOffsets)]
struct Foo {
    x: u32,
}
```
*/
#[cfg(doctest)]
const _NO_REPR_C: u32 = 0;

/**
```compile_fail
use const_field_offset::*;
#[derive(FieldOffsets)]
#[repr(C)]
#[repr(packed)]
struct Foo {
    x: u32,
}
```
*/
#[cfg(doctest)]
const _REPR_PACKED: u32 = 0;

/**
```compile_fail
use const_field_offset::*;
#[derive(FieldOffsets)]
#[repr(C)]
#[pin]
struct Foo {
    x: u32,
}

impl Drop for Foo {
    fn drop(&mut self) {}
}
```
*/
#[cfg(doctest)]
const _PIN_NO_DROP: u32 = 0;

/**
```compile_fail
use const_field_offset::*;
#[derive(FieldOffsets)]
#[repr(C)]
#[pin]
struct Foo {
    q: std::marker::PhantomPinned,
    x: u32,
}

impl Unpin for Foo {}
```
*/
#[cfg(doctest)]
const _PIN_NO_UNPIN: u32 = 0;
