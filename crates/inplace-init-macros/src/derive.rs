//! 展开 `Init` derive，并收集源结构体的字段元数据。
//! Expands `Init` derive and collects source-struct field metadata.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, Index, Member, Result, ext::IdentExt};

use crate::{
    derive::{
        builder::BuilderMacro,
        model::{BuilderInput, DeriveField, DeriveModel, FieldDefault},
    },
    support::MacroSupport,
};

mod builder;
mod model;

/// derive 宏的本地入口类型，避免跨模块公开独立辅助函数。
/// Local entry type for the derive macro; helper functions remain module-private.
pub(super) struct DeriveMacro;

impl DeriveMacro {
    pub(super) fn expand(input: DeriveInput) -> Result<TokenStream2> {
        expand_derive(input)
    }
}

fn expand_derive(input: DeriveInput) -> Result<TokenStream2> {
    let name = input.ident.clone();
    let visibility = input.vis.clone();
    let generics = input.generics.clone();
    let data = match input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(Error::new_spanned(
                name,
                "`Init` can only be derived for structs",
            ));
        }
    };

    if DeriveModel::has_packed_repr(&input.attrs)? {
        return Err(Error::new_spanned(
            name,
            "`Init` does not support `repr(packed)` structs",
        ));
    }

    let mut fields = Vec::new();
    match &data.fields {
        Fields::Named(named) => {
            for field in &named.named {
                let identifier = field
                    .ident
                    .as_ref()
                    .expect("Named fields must have an identifier");
                let field_name = identifier.unraw().to_string();
                let pinned = DeriveModel::has_pin_attr(&field.attrs)?;
                let default = DeriveModel::init_default_attr(&field.attrs)?;
                fields.push(DeriveField {
                    name: field_name,
                    member: Member::Named(identifier.clone()),
                    method: identifier.clone(),
                    ty: field.ty.clone(),
                    pinned,
                    default,
                    default_helper: None,
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for (index, field) in unnamed.unnamed.iter().enumerate() {
                let method = format_ident!("field_{index}");
                fields.push(DeriveField {
                    name: index.to_string(),
                    member: Member::Unnamed(Index::from(index)),
                    method,
                    ty: field.ty.clone(),
                    pinned: DeriveModel::has_pin_attr(&field.attrs)?,
                    default: DeriveModel::init_default_attr(&field.attrs)?,
                    default_helper: None,
                });
            }
        }
        Fields::Unit => {}
    }

    DeriveModel::validate_builder_method_names(&fields)?;
    for (index, field) in fields.iter_mut().enumerate() {
        if matches!(field.default, Some(FieldDefault::Initializer(_))) {
            field.default_helper = Some(format_ident!(
                "InplaceInit{}Default{index}",
                name,
                span = Span::call_site()
            ));
        }
    }
    let mut field_keys = Vec::new();
    for field in &fields {
        let key = MacroSupport::field_key(&field.name);
        if field_keys.contains(&key) {
            return Err(Error::new_spanned(
                &field.method,
                "Field-name mapping key collision; cannot generate a safe field projection",
            ));
        }
        field_keys.push(key);
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_count = fields.len();
    let field_names = fields.iter().map(|field| {
        let field_name = &field.name;
        let pinned = field.pinned;
        quote! { (#field_name, #pinned) }
    });
    let root = MacroSupport::crate_path();
    let field_projections = fields
        .iter()
        .map(|field| {
            let member = &field.member;
            let field_type = &field.ty;
            let key = MacroSupport::field_key(&field.name);
            let field_state = if field.pinned {
                quote!(#root::PinnedField)
            } else {
                quote!(#root::MovableField)
            };
            quote! {
                unsafe impl #impl_generics #root::InitField<#key> for #name #ty_generics #where_clause {
                    type Field = #field_type;
                    type State = #field_state;

                    unsafe fn __field_ptr(target: *mut Self) -> *mut Self::Field {
                        // 安全性：键由 derive 按目标字段名生成。 / Safety: derive generated the key from this target field's name.
                        unsafe { ::core::ptr::addr_of_mut!((*target).#member) }
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    let builder = BuilderMacro::expand(BuilderInput {
        name: &name,
        visibility: &visibility,
        generics: &generics,
        fields_definition: &data.fields,
        fields: &fields,
        root: &root,
    })?;

    Ok(quote! {
        unsafe impl #impl_generics #root::InitFields for #name #ty_generics #where_clause {
            const FIELD_COUNT: usize = #field_count;
            const FIELDS: &'static [(&'static str, bool)] = &[#(#field_names),*];
        }

        #(#field_projections)*

        #builder
    })
}
