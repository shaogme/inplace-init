//! 生成字段级类型状态 builder 及其初始化实现。
//! Generates the field-level typestate builder and its initialization implementations.

use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Fields, Result, parse_quote};

use crate::{
    derive::model::{
        BuilderBuildInput, BuilderInput, BuilderSetterInput, DeriveField, FieldDefault,
    },
    support::MacroSupport,
};

/// builder 展开器；公开给父 derive 模块的入口保持为关联函数。
/// Builder expander; its entry into the parent derive module is an associated function.
pub(super) struct BuilderMacro;

impl BuilderMacro {
    pub(super) fn expand(input: BuilderInput<'_>) -> syn::Result<TokenStream2> {
        expand_builder(input)
    }
}

// 每个字段状态都是独立泛型参数；setter 消耗旧 builder 并返回只替换一个状态的新类型，
// 因而缺失字段会在类型检查阶段暴露，而不是运行时才失败。
// Each field state is an independent type parameter; a setter consumes the old builder and
// returns a type with exactly one state replaced, so missing fields fail during type checking.
fn expand_builder(input: BuilderInput<'_>) -> Result<TokenStream2> {
    let BuilderInput {
        name,
        visibility,
        generics,
        fields_definition,
        fields,
        root,
    } = input;
    let builder_name = format_ident!("InplaceInit{}Builder", name);
    let error_name = format_ident!("InplaceInit{}Error", name);
    let mut occupied_parameters = generics
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Lifetime(parameter) => parameter.lifetime.ident.to_string(),
            syn::GenericParam::Type(parameter) => parameter.ident.to_string(),
            syn::GenericParam::Const(parameter) => parameter.ident.to_string(),
        })
        .collect::<Vec<_>>();
    let mode_parameter = MacroSupport::unique_ident("InplaceMode", &mut occupied_parameters);
    let state_parameters = (0..fields.len())
        .map(|index| {
            MacroSupport::unique_ident(&format!("InplaceState{index}"), &mut occupied_parameters)
        })
        .collect::<Vec<_>>();
    let state_fields = (0..fields.len())
        .map(|index| format_ident!("__state_{index}"))
        .collect::<Vec<_>>();

    let mut builder_generics = MacroSupport::strip_generic_defaults(generics.clone());
    builder_generics.params.push(parse_quote!(#mode_parameter));
    for state in &state_parameters {
        builder_generics.params.push(parse_quote!(#state));
    }
    let original_args = MacroSupport::generic_arguments(generics);
    let (_, original_ty_generics, _) = generics.split_for_impl();
    let original_type = quote!(#name #original_ty_generics);
    let default_helpers =
        expand_default_helpers(visibility, generics, fields, &original_type, root);
    let builder_fields = state_fields
        .iter()
        .zip(&state_parameters)
        .map(|(field, state)| quote! { #field: #state });
    let initial_states = fields
        .iter()
        .map(|field| {
            if field.default.is_some() {
                default_state_type(field, root, &original_args)
            } else {
                quote!(#root::Unset)
            }
        })
        .collect::<Vec<_>>();
    let initial_state_values = fields
        .iter()
        .zip(&state_fields)
        .map(|(field, state)| initial_state_value(field, state, root, &original_args));
    let init_default_bounds = fields
        .iter()
        .filter(|field| {
            field
                .default
                .as_ref()
                .is_some_and(|default| matches!(default, FieldDefault::InitDefault))
        })
        .map(|field| {
            let field_type = &field.ty;
            quote!(#field_type: #root::InitDefault<#field_type, #root::InitMode>)
        })
        .collect::<Vec<_>>();
    let init_default_where_clause = if init_default_bounds.is_empty() {
        quote!()
    } else {
        quote!(where #(#init_default_bounds),*)
    };
    let pin_default_bounds = fields
        .iter()
        .filter(|field| {
            field
                .default
                .as_ref()
                .is_some_and(|default| matches!(default, FieldDefault::InitDefault))
        })
        .map(|field| {
            let field_type = &field.ty;
            let mode = if field.pinned {
                quote!(#root::PinInitMode)
            } else {
                quote!(#root::InitMode)
            };
            quote!(#field_type: #root::InitDefault<#field_type, #mode>)
        })
        .collect::<Vec<_>>();
    let pin_default_where_clause = if pin_default_bounds.is_empty() {
        quote!()
    } else {
        quote!(where #(#pin_default_bounds),*)
    };

    // 普通模式无法表达固定字段的地址不变式，因此只在结构体没有 `#[pin]` 字段时生成 `init`。
    // Movable mode cannot express pinned-field address invariants, so `init` exists only when no field is pinned.
    let init_method = if fields.iter().any(|field| field.pinned) {
        quote!()
    } else {
        let builder_type = MacroSupport::builder_type(
            &builder_name,
            &original_args,
            quote!(#root::InitMode),
            &initial_states,
        );
        let state_initializers = initial_state_values.clone();
        quote! {
            #visibility fn init() -> #builder_type #init_default_where_clause {
                #builder_name {
                    #(#state_initializers,)*
                    __mode: ::core::marker::PhantomData,
                    __type: ::core::marker::PhantomData,
                }
            }
        }
    };

    let pin_builder_type = MacroSupport::builder_type(
        &builder_name,
        &original_args,
        quote!(#root::PinInitMode),
        &initial_states,
    );
    let pin_state_initializers = initial_state_values;
    let pin_init_method = quote! {
        #visibility fn pin_init() -> #pin_builder_type #pin_default_where_clause {
            #builder_name {
                #(#pin_state_initializers,)*
                __mode: ::core::marker::PhantomData,
                __type: ::core::marker::PhantomData,
            }
        }
    };

    let mut setters = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        setters.push(expand_builder_setter(BuilderSetterInput {
            builder: &builder_name,
            generics,
            original_args: &original_args,
            fields,
            target_index: index,
            target: field,
            mode_parameter: &mode_parameter,
            state_parameters: &state_parameters,
            state_fields: &state_fields,
            root,
            pinned_mode: false,
        }));
        if field.pinned {
            setters.push(expand_builder_setter(BuilderSetterInput {
                builder: &builder_name,
                generics,
                original_args: &original_args,
                fields,
                target_index: index,
                target: field,
                mode_parameter: &mode_parameter,
                state_parameters: &state_parameters,
                state_fields: &state_fields,
                root,
                pinned_mode: true,
            }));
        }
    }

    let init_build = if fields.iter().any(|field| field.pinned) {
        quote!()
    } else {
        expand_builder_build(BuilderBuildInput {
            name,
            builder: &builder_name,
            error_name: &error_name,
            generics,
            fields_definition,
            fields,
            original_args: &original_args,
            state_parameters: &state_parameters,
            state_fields: &state_fields,
            root,
            pinned_mode: false,
        })
    };
    let pin_build = expand_builder_build(BuilderBuildInput {
        name,
        builder: &builder_name,
        error_name: &error_name,
        generics,
        fields_definition,
        fields,
        original_args: &original_args,
        state_parameters: &state_parameters,
        state_fields: &state_fields,
        root,
        pinned_mode: true,
    });

    // 每个字段错误保留原始错误类型，并通过字段序号区分来源。
    // Preserve each field's original error type and identify its source by field index.
    let error_enum = if fields.is_empty() {
        quote!()
    } else {
        let error_parameters = (0..fields.len())
            .map(|index| format_ident!("InplaceError{index}"))
            .collect::<Vec<_>>();
        let variants = error_parameters.iter().enumerate().map(|(index, error)| {
            let variant = format_ident!("Field{index}");
            quote! { #variant(#error) }
        });
        quote! {
            #[doc(hidden)]
            #[derive(Debug)]
            #visibility enum #error_name<#(#error_parameters),*> {
                #(#variants),*
            }
        }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        #[doc(hidden)]
        #visibility struct #builder_name #builder_generics {
            #(#builder_fields,)*
            __mode: ::core::marker::PhantomData<#mode_parameter>,
            __type: ::core::marker::PhantomData<fn() -> #original_type>,
        }

        #(#default_helpers)*

        #error_enum

        impl #impl_generics #name #ty_generics #where_clause {
            #init_method
            #pin_init_method
        }

        #(#setters)*

        #init_build
        #pin_build
    })
}

fn expand_default_helpers(
    visibility: &syn::Visibility,
    generics: &syn::Generics,
    fields: &[DeriveField],
    original_type: &TokenStream2,
    root: &TokenStream2,
) -> Vec<TokenStream2> {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    fields
        .iter()
        .filter_map(|field| {
            let FieldDefault::Initializer(expression) = field.default.as_ref()? else {
                return None;
            };
            let helper = field.default_helper.as_ref()?;
            let field_type = &field.ty;
            let mode = if field.pinned {
                quote!(#root::PinInitMode)
            } else {
                quote!(#root::InitMode)
            };
            Some(quote! {
                #[doc(hidden)]
                #visibility struct #helper #generics {
                    _target: ::core::marker::PhantomData<fn() -> #original_type>,
                }

                impl #impl_generics #helper #ty_generics #where_clause {
                    fn new() -> Self {
                        Self {
                            _target: ::core::marker::PhantomData,
                        }
                    }
                }

                #[doc = "安全性依据：此适配器只转发字段初始化表达式的凭证与结果，其 Init unsafe 契约负责失败清理。"]
                #[doc = "Safety: this adapter only forwards the field initializer's receipt or result, and that initializer's Init contract guarantees cleanup on failure."]
                unsafe impl #impl_generics #root::Init<#field_type, ::core::convert::Infallible, #mode>
                    for #helper #ty_generics #where_clause
                {
                    fn initialize<'slot>(
                        self,
                        slot: #root::InitSlot<'slot, #field_type, #mode>,
                    ) -> ::core::result::Result<
                        #root::InitReceipt<'slot, #field_type, #mode>,
                        ::core::convert::Infallible,
                    > {
                        <_ as #root::Init<#field_type, ::core::convert::Infallible, #mode>>::initialize(
                            #expression,
                            slot,
                        )
                    }
                }
            })
        })
        .collect()
}

fn default_state_type(
    field: &DeriveField,
    root: &TokenStream2,
    original_args: &[TokenStream2],
) -> TokenStream2 {
    let field_type = &field.ty;
    let initializer_type = match field.default.as_ref() {
        Some(FieldDefault::Initializer(_)) => {
            let helper = field
                .default_helper
                .as_ref()
                .expect("A default initializer must have a generated adapter type");
            if original_args.is_empty() {
                quote!(#helper)
            } else {
                quote!(#helper<#(#original_args),*>)
            }
        }
        _ => quote!(#root::InitDefaultInit<#field_type>),
    };
    quote! {
        #root::Set<#initializer_type, ::core::convert::Infallible>
    }
}

fn initial_state_value(
    field: &DeriveField,
    state: &Ident,
    root: &TokenStream2,
    original_args: &[TokenStream2],
) -> TokenStream2 {
    let Some(default) = &field.default else {
        return quote! { #state: #root::Unset };
    };
    let field_type = &field.ty;
    let initializer = match default {
        FieldDefault::InitDefault => quote!(#root::InitDefaultInit::<#field_type>::new()),
        FieldDefault::Initializer(_) => {
            let helper = field
                .default_helper
                .as_ref()
                .expect("A default initializer must have a generated adapter type");
            if original_args.is_empty() {
                quote!(#helper::new())
            } else {
                quote!(#helper::<#(#original_args),*>::new())
            }
        }
    };
    quote! {
        #state: #root::Set::new(#initializer)
    }
}

fn expand_builder_setter(input: BuilderSetterInput<'_>) -> TokenStream2 {
    let BuilderSetterInput {
        builder,
        generics,
        original_args,
        fields,
        target_index,
        target,
        mode_parameter,
        state_parameters,
        state_fields,
        root,
        pinned_mode,
    } = input;
    if target.pinned != pinned_mode {
        return quote!();
    }

    let mut setter_generics = MacroSupport::strip_generic_defaults(generics.clone());
    let mode_type = if pinned_mode {
        quote!(#root::PinInitMode)
    } else {
        setter_generics.params.push(parse_quote!(#mode_parameter));
        quote!(#mode_parameter)
    };
    let other_state_parameters = state_parameters
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != target_index)
        .map(|(_, state)| state.clone())
        .collect::<Vec<_>>();
    for state in &other_state_parameters {
        setter_generics.params.push(parse_quote!(#state));
    }
    let (setter_impl_generics, _, setter_where_clause) = setter_generics.split_for_impl();
    let mut method_parameters = generics
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Lifetime(parameter) => parameter.lifetime.ident.to_string(),
            syn::GenericParam::Type(parameter) => parameter.ident.to_string(),
            syn::GenericParam::Const(parameter) => parameter.ident.to_string(),
        })
        .collect::<Vec<_>>();
    method_parameters.extend(state_parameters.iter().map(ToString::to_string));
    let initializer_type =
        MacroSupport::unique_ident("InplaceFieldInitializer", &mut method_parameters);
    let initializer_error = MacroSupport::unique_ident("InplaceFieldError", &mut method_parameters);

    let mut input_states = Vec::new();
    for (index, _) in fields.iter().enumerate() {
        if index == target_index {
            input_states.push(if target.default.is_some() {
                default_state_type(target, root, original_args)
            } else {
                quote!(#root::Unset)
            });
        } else {
            let state = &state_parameters[index];
            input_states.push(quote!(#state));
        }
    }

    let self_type =
        MacroSupport::builder_type(builder, original_args, mode_type.clone(), &input_states);
    let initializer_return_states = fields
        .iter()
        .enumerate()
        .map(|(index, _field)| {
            if index == target_index {
                quote!(#root::Set<#initializer_type, #initializer_error>)
            } else {
                let state = &state_parameters[index];
                quote!(#state)
            }
        })
        .collect::<Vec<_>>();
    let initializer_return_type = MacroSupport::builder_type(
        builder,
        original_args,
        mode_type.clone(),
        &initializer_return_states,
    );
    let destructure_fields = state_fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            if index == target_index {
                quote! { #field: _, }
            } else {
                quote! { #field, }
            }
        })
        .collect::<Vec<_>>();
    let initializer_construct_fields = state_fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            if index == target_index {
                quote! { #field: #root::Set::new(initializer), }
            } else {
                quote! { #field, }
            }
        })
        .collect::<Vec<_>>();
    let method_name = &target.method;
    let init_trait = if target.pinned {
        let field_type = &target.ty;
        quote!(#root::Init<#field_type, #initializer_error, #root::PinInitMode>)
    } else {
        let field_type = &target.ty;
        quote!(#root::Init<#field_type, #initializer_error>)
    };
    quote! {
        impl #setter_impl_generics #self_type #setter_where_clause {
            pub fn #method_name<#initializer_type, #initializer_error>(
            self,
            initializer: #initializer_type,
        ) -> #initializer_return_type
        where
            #initializer_type: #init_trait,
        {
            let #builder {
                #(#destructure_fields)*
                __mode: _,
                __type: _,
            } = self;
            #builder {
                #(#initializer_construct_fields)*
                __mode: ::core::marker::PhantomData,
                __type: ::core::marker::PhantomData,
            }
        }
        }
    }
}

fn expand_builder_build(input: BuilderBuildInput<'_>) -> TokenStream2 {
    let BuilderBuildInput {
        name,
        builder,
        error_name,
        generics,
        fields_definition,
        fields,
        original_args,
        state_parameters,
        state_fields,
        root,
        pinned_mode,
    } = input;
    let mode = if pinned_mode {
        quote!(#root::PinInitMode)
    } else {
        quote!(#root::InitMode)
    };
    let mut build_generics = MacroSupport::strip_generic_defaults(generics.clone());
    for state in state_parameters {
        build_generics.params.push(parse_quote!(#state));
    }
    for (index, field) in fields.iter().enumerate() {
        let state = &state_parameters[index];
        let field_type = &field.ty;
        let field_pinned = field.pinned;
        build_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #state: #root::InitState<#field_type, #mode, #field_pinned>
            ));
    }
    let (build_impl_generics, _, build_where_clause) = build_generics.split_for_impl();
    let state_type_args = state_parameters
        .iter()
        .map(|state| quote!(#state))
        .collect::<Vec<_>>();
    let builder_type =
        MacroSupport::builder_type(builder, original_args, mode.clone(), &state_type_args);
    let type_generics = generics.split_for_impl().1;
    let type_name = quote!(#name #type_generics);
    let state_moves = state_fields
        .iter()
        .map(|field| quote!(let #field = self.#field;));
    let method_visibility = quote!(pub);

    if fields.is_empty() {
        let value = match fields_definition {
            Fields::Named(_) => quote!(#name {}),
            Fields::Unnamed(_) => quote!(#name()),
            Fields::Unit => quote!(#name),
        };
        return quote! {
            impl #build_impl_generics #builder_type #build_where_clause {
                #method_visibility fn build(self) -> Self {
                    self
                }
            }

            #[doc = "安全性依据：空结构体通过 slot.write 一次性构造完整值，并返回唯一析构凭证。"]
            #[doc = "Safety: slot.write constructs the complete empty struct at once and returns its sole destruction receipt."]
            unsafe impl #build_impl_generics #root::Init<#type_name, ::core::convert::Infallible, #mode>
                for #builder_type #build_where_clause
            {
                fn initialize<'slot>(
                    self,
                    slot: #root::InitSlot<'slot, #type_name, #mode>,
                ) -> ::core::result::Result<#root::InitReceipt<'slot, #type_name, #mode>, ::core::convert::Infallible> {
                    let _ = self;
                    Ok(slot.write(#value))
                }
            }
        };
    }

    let error_types = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let state = &state_parameters[index];
            let field_type = &field.ty;
            let field_pinned = field.pinned;
            quote!(<#state as #root::InitState<#field_type, #mode, #field_pinned>>::Error)
        })
        .collect::<Vec<_>>();
    let error_type = quote!(#error_name<#(#error_types),*>);
    let slot_ident = format_ident!("__inplace_parent_slot", span = Span::mixed_site());
    let target_pointer = format_ident!("__inplace_target_pointer", span = Span::mixed_site());
    let field_pointers = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let pointer =
                format_ident!("__inplace_field_pointer{index}", span = Span::mixed_site());
            let member = &field.member;
            let field_type = &field.ty;
            quote! {
                let #pointer: *mut #field_type = unsafe {
                    ::core::ptr::addr_of_mut!((*#target_pointer).#member)
                };
            }
        })
        .collect::<Vec<_>>();
    let field_slots = fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let pointer =
                format_ident!("__inplace_field_pointer{index}", span = Span::mixed_site());
            let field_slot =
                format_ident!("__inplace_field_slot{index}", span = Span::mixed_site());
            quote! {
                let #field_slot = unsafe { #slot_ident.__project(#pointer) };
            }
        })
        .collect::<Vec<_>>();
    let initialize_fields = fields
        .iter()
        .zip(state_fields)
        .enumerate()
        .map(|(index, (field, state))| {
            let field_slot = format_ident!("__inplace_field_slot{index}", span = Span::mixed_site());
            let receipt = format_ident!("__inplace_field_receipt{index}", span = Span::mixed_site());
            let field_type = &field.ty;
            let field_pinned = field.pinned;
            let state_parameter = &state_parameters[index];
            let variant = format_ident!("Field{index}");
            quote! {
                let #receipt = <#state_parameter as #root::InitState<#field_type, #mode, #field_pinned>>::initialize(
                    #state,
                    #field_slot,
                )
                .map_err(#error_name::#variant)?;
            }
        })
        .collect::<Vec<_>>();
    let receipts = fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let receipt =
                format_ident!("__inplace_field_receipt{index}", span = Span::mixed_site());
            quote! { ::core::mem::forget(#receipt); }
        })
        .collect::<Vec<_>>();

    quote! {
        impl #build_impl_generics #builder_type #build_where_clause {
            #method_visibility fn build(self) -> Self {
                self
            }
        }

        #[doc = "安全性依据：每个字段凭证保留到后续字段成功；错误或 panic 会回滚已完成字段，全部成功后才转交整体析构责任。"]
        #[doc = "Safety: each field receipt remains live until later fields succeed; errors or panics roll back completed fields, and whole-value destruction responsibility transfers only after all fields succeed."]
        unsafe impl #build_impl_generics #root::Init<#type_name, #error_type, #mode>
            for #builder_type #build_where_clause
        {
            fn initialize<'slot>(
                self,
                mut #slot_ident: #root::InitSlot<'slot, #type_name, #mode>,
            ) -> ::core::result::Result<#root::InitReceipt<'slot, #type_name, #mode>, #error_type> {
                #(#state_moves)*
                let #target_pointer = #slot_ident.__target_ptr();
                #(#field_pointers)*
                #(#field_slots)*
                #(#initialize_fields)*
                #(#receipts)*
                // 安全性：字段凭证已转交，所有字段完整后父对象可整体析构。 / Safety: transferred field receipts allow the complete parent to be dropped as one value.
                Ok(unsafe { #slot_ident.__assume_init() })
            }
        }
    }
}
