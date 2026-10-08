//! 解析 `init!`、`pin_init!` 与 `pin_init_local!` 的语法并生成初始化器。
//! Parses `init!`, `pin_init!`, and `pin_init_local!` syntax and generates initializers.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Error, Expr, Index, LitInt, Member, Result, Token, Type, braced,
    ext::IdentExt,
    parse,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
};

use crate::support::MacroSupport;

/// 初始化宏入口；普通、固定地址及局部固定存储共享同一语法树。
/// Initializer macro entry; movable, pinned, and local-pinned forms share one syntax tree.
pub(super) struct InitializerMacro;

impl InitializerMacro {
    pub(super) fn expand(input: TokenStream, pinned: bool) -> TokenStream {
        expand_initializer(input, pinned)
    }

    pub(super) fn expand_pin_local(input: TokenStream) -> TokenStream {
        let input = parse_macro_input!(input as PinLocalInput);
        expand_pin_local(input)
            .unwrap_or_else(Error::into_compile_error)
            .into()
    }
}

// `let mut name = ...` 由宏自行创建存储，绑定声明顺序确保 receipt 先于存储析构。
// The macro owns the storage; declaration order ensures the receipt drops before that storage.
struct PinLocalInput {
    name: Ident,
    initializer: InitializerInput,
}

impl Parse for PinLocalInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<Token![let]>()?;
        input.parse::<Token![mut]>()?;
        let name = input.call(Ident::parse_any)?;
        input.parse::<Token![=]>()?;
        let initializer = input.parse()?;
        Ok(Self { name, initializer })
    }
}

fn expand_pin_local(input: PinLocalInput) -> Result<TokenStream2> {
    let PinLocalInput { name, initializer } = input;
    let ty = initializer.ty.clone();
    let fallible = initializer.error.is_some();
    let initializer_tokens = expand_initializer_tokens(initializer, true)?;
    let root = MacroSupport::crate_path();
    let storage = format_ident!("__inplace_pin_storage", span = Span::mixed_site());

    // 显式 `?` 保留用户选择的错误传播；无 `?` 时穷尽 Infallible，保持展开结果无额外错误分支。
    // An explicit `?` keeps the caller's error propagation; otherwise exhaust `Infallible` with no runtime error branch.
    let binding = if fallible {
        quote! {
            let mut #name = unsafe {
                #root::pin_init_in(
                    ::core::pin::Pin::new_unchecked(&mut #storage),
                    #initializer_tokens,
                )
            }?;
        }
    } else {
        quote! {
            let mut #name = match unsafe {
                #root::pin_init_in(
                    ::core::pin::Pin::new_unchecked(&mut #storage),
                    #initializer_tokens,
                )
            } {
                Ok(receipt) => receipt,
                Err(never) => match never {},
            };
        }
    };

    Ok(quote! {
        let mut #storage: ::core::mem::MaybeUninit<#ty> = ::core::mem::MaybeUninit::uninit();
        #binding
    })
}
fn expand_initializer(input: TokenStream, pinned: bool) -> TokenStream {
    let parsed = match parse::<InitializerInput>(input) {
        Ok(parsed) => parsed,
        Err(error) => return error.into_compile_error().into(),
    };

    match expand_initializer_tokens(parsed, pinned) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

// 先完成纯语法与字段检查，再生成实现；字段清单在常量上下文中与 derive 元数据核对。
// Validate syntax and field selection first; generated code then checks the field inventory against derive metadata in const context.
fn expand_initializer_tokens(input: InitializerInput, pinned: bool) -> Result<TokenStream2> {
    let InitializerInput {
        ty,
        fields,
        default,
        error,
    } = input;
    let error_ty = error.unwrap_or_else(|| parse_quote!(::core::convert::Infallible));
    let root = MacroSupport::crate_path();
    let mode = if pinned {
        quote!(#root::PinInitMode)
    } else {
        quote!(#root::InitMode)
    };

    if default {
        return Ok(quote! {
            #root::InitDefaultInit::<#ty>::new().map_err(
                |never| -> #error_ty { match never {} },
            )
        });
    }

    if fields.is_empty() {
        return Err(Error::new_spanned(
            ty,
            "The initializer list cannot be empty; use `..` for default construction",
        ));
    }

    // 结构字段的身份由成员名决定；重复项会造成同一存储被初始化两次，因此在展开前拒绝。
    // Member names define field identity; duplicates would initialize one storage location twice, so reject them before expansion.
    let mut seen = Vec::new();
    for field in &fields {
        let key = MacroSupport::member_name(&field.member);
        if seen.iter().any(|existing| existing == &key) {
            return Err(Error::new_spanned(
                &field.member,
                "A field cannot be initialized more than once",
            ));
        }
        seen.push(key);
    }

    if !pinned
        && fields
            .iter()
            .any(|field| matches!(field.value, FieldInitializerValue::Target))
    {
        return Err(Error::new_spanned(
            ty,
            "`@target` is only supported by `pin_init!` or `pin_init_local!`",
        ));
    }

    let field_count = fields.len();
    let slot_ident = format_ident!("__inplace_parent_slot", span = Span::mixed_site());
    let target_pointer = format_ident!("__inplace_target_pointer", span = Span::mixed_site());
    let initializer_type = format_ident!("InplaceMacroInit", span = Span::mixed_site());
    let type_parameters = (0..field_count)
        .map(|index| format_ident!("InplaceMacroFieldInit{index}", span = Span::mixed_site()))
        .collect::<Vec<_>>();
    let local_generics = quote!(<InplaceMacroTarget, InplaceMacroError, #(#type_parameters),*>);
    let local_type_arguments =
        quote!(<InplaceMacroTarget, InplaceMacroError, #(#type_parameters),*>);
    let initializer_fields = fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let field = format_ident!("__initializer{index}", span = Span::mixed_site());
            let parameter = &type_parameters[index];
            quote! { #field: #parameter }
        })
        .collect::<Vec<_>>();
    let target_bounds = fields.iter().map(|field| {
        let key = MacroSupport::field_key(&MacroSupport::member_name(&field.member));
        quote!(InplaceMacroTarget: #root::InitField<#key>)
    });
    let initializer_bounds = fields.iter().enumerate().map(|(index, field)| {
        let parameter = &type_parameters[index];
        let key = MacroSupport::field_key(&MacroSupport::member_name(&field.member));
        match &field.value {
            FieldInitializerValue::Expression(_) => {
                let field_state = quote!(<InplaceMacroTarget as #root::InitField<#key>>::State);
                quote!(
                    #parameter: #root::InitFieldInit<
                        <InplaceMacroTarget as #root::InitField<#key>>::Field,
                        InplaceMacroError,
                        #mode,
                        #field_state,
                    >
                )
            }
            FieldInitializerValue::Target => quote!(
                <InplaceMacroTarget as #root::InitField<#key>>::Field:
                    #root::InitTargetPointer<InplaceMacroTarget>
            ),
        }
    });
    let initializer_values = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let name = format_ident!("__initializer{index}", span = Span::mixed_site());
            let key = MacroSupport::field_key(&MacroSupport::member_name(&field.member));
            let value = match &field.value {
                FieldInitializerValue::Expression(expression)
                    if !MacroSupport::is_default_expression(expression) =>
                {
                    quote!(#expression)
                }
                FieldInitializerValue::Expression(_) => quote! {
                    #root::InitDefaultInit::<
                        <#ty as #root::InitField<#key>>::Field,
                    >::new().map_err(|never| -> #error_ty { match never {} })
                },
                _ => quote!(()),
            };
            quote! { #name: #value }
        })
        .collect::<Vec<_>>();
    let inference_arguments = fields.iter().map(|_| quote!(_)).collect::<Vec<_>>();
    let provided_fields = fields.iter().map(|field| {
        let name = MacroSupport::member_name(&field.member);
        quote! { (#name, true) }
    });
    let initialize_fields = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_slot = format_ident!("__inplace_field_slot{index}", span = Span::mixed_site());
            let receipt = format_ident!("__inplace_field_receipt{index}", span = Span::mixed_site());
            let initializer = format_ident!("__initializer{index}", span = Span::mixed_site());
            let key = MacroSupport::field_key(&MacroSupport::member_name(&field.member));
            let initialize = match &field.value {
                FieldInitializerValue::Target => quote! {
                    Ok::<_, InplaceMacroError>(#field_slot.write(
                        <<InplaceMacroTarget as #root::InitField<#key>>::Field as
                            #root::InitTargetPointer<InplaceMacroTarget>>::from_target(#target_pointer)
                    ))
                },
                FieldInitializerValue::Expression(_) => quote! {
                    #root::InitFieldInit::initialize(self.#initializer, #field_slot)
                },
            };
            quote! {
                let __field_pointer = unsafe {
                    <InplaceMacroTarget as #root::InitField<#key>>::__field_ptr(#target_pointer)
                };
                // 安全性：derive 元数据保证投影到目标中的唯一字段。 / Safety: derive metadata guarantees this projection selects one unique target field.
                let #field_slot = unsafe { #slot_ident.__project(__field_pointer) };
                let #receipt = #initialize?;
            }
        })
        .collect::<Vec<_>>();
    let forget_receipts = fields.iter().enumerate().map(|(index, _)| {
        let receipt = format_ident!("__inplace_field_receipt{index}", span = Span::mixed_site());
        quote! { ::core::mem::forget(#receipt); }
    });

    Ok(quote! {{
        const {
            #root::__validate_init_fields::<#ty>(
                &[#(#provided_fields),*],
                #pinned,
            );
        };
        struct #initializer_type #local_generics {
            __target: ::core::marker::PhantomData<fn() -> InplaceMacroTarget>,
            __error: ::core::marker::PhantomData<fn() -> InplaceMacroError>,
            #(#initializer_fields,)*
        }
        #[doc = "安全性依据：逐字段保留已完成字段的凭证；失败或 panic 时凭证自动回滚，全部成功后才转交整体析构责任。"]
        #[doc = "Safety: receipts are retained for completed fields and roll them back on error or panic; whole-value destruction responsibility transfers only after every field succeeds."]
        unsafe impl #local_generics #root::Init<InplaceMacroTarget, InplaceMacroError, #mode>
            for #initializer_type #local_type_arguments
            where
                InplaceMacroTarget: #root::InitFields,
                #(#target_bounds,)*
                #(#initializer_bounds,)*
        {
            fn initialize<'slot>(
                self,
                mut #slot_ident: #root::InitSlot<'slot, InplaceMacroTarget, #mode>,
            ) -> ::core::result::Result<
                #root::InitReceipt<'slot, InplaceMacroTarget, #mode>,
                InplaceMacroError,
            > {
                let #target_pointer = #slot_ident.__target_ptr();
                #(#initialize_fields)*
                #(#forget_receipts)*
                // 安全性：字段凭证已转交，全部字段完成后父对象可整体析构。 / Safety: field receipts are transferred, so the fully initialized parent may be dropped as one value.
                Ok(unsafe { #slot_ident.__assume_init() })
            }
        }
        #initializer_type::<#ty, #error_ty, #(#inference_arguments),*> {
            __target: ::core::marker::PhantomData,
            __error: ::core::marker::PhantomData,
            #(#initializer_values,)*
        }
    }})
}
// 解析后的表示将目标类型、各字段初始化器、整体默认模式和错误类型分开保存，
// 这样语法约束只需在 parse 阶段执行一次。
// The parsed form keeps target type, field initializers, whole-value default mode, and error type separate,
// so syntax constraints are enforced once during parsing.
struct InitializerInput {
    ty: Type,
    fields: Punctuated<FieldInitializer, Token![,]>,
    default: bool,
    error: Option<Type>,
}

impl Parse for InitializerInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ty = input.parse()?;
        let content;
        braced!(content in input);
        let entries = content.parse_terminated(InitializerEntry::parse, Token![,])?;
        let default_count = entries
            .iter()
            .filter(|entry| matches!(entry, InitializerEntry::Default))
            .count();
        if default_count > 0 && entries.len() != 1 {
            return Err(Error::new_spanned(
                ty,
                "Whole-value default construction with `..` cannot be combined with per-field initializers",
            ));
        }
        let default = default_count == 1;
        let fields = entries
            .into_iter()
            .filter_map(|entry| match entry {
                InitializerEntry::Field(field) => Some(field),
                InitializerEntry::Default => None,
            })
            .collect();
        let error = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("Unrecognized syntax at the end of the macro invocation"));
        }
        Ok(Self {
            ty,
            fields,
            default,
            error,
        })
    }
}

enum InitializerEntry {
    Default,
    Field(FieldInitializer),
}

impl Parse for InitializerEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(Token![..]) {
            input.parse::<Token![..]>()?;
            Ok(Self::Default)
        } else {
            FieldInitializer::parse(input).map(Self::Field)
        }
    }
}

struct FieldInitializer {
    member: Member,
    value: FieldInitializerValue,
}

// `@target` 表示把稳定目标地址写入字段，而不是先构造一个临时值。
// `@target` stores the stable target address in the field instead of constructing a temporary value.
enum FieldInitializerValue {
    Expression(Expr),
    Target,
}

impl Parse for FieldInitializer {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let member = if input.peek(Ident::peek_any) {
            Member::Named(input.parse()?)
        } else if input.peek(LitInt) {
            Member::Unnamed(input.parse::<Index>()?)
        } else {
            return Err(
                input.error("A field must be specified by an identifier or tuple-field index")
            );
        };
        input
            .parse::<Token![<]>()
            .map_err(|_| input.error("A field initializer must be specified with `<-`"))?;
        input
            .parse::<Token![-]>()
            .map_err(|_| input.error("A field initializer must be specified with `<-`"))?;
        let value = if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            let target: Ident = input.parse()?;
            if target != "target" {
                return Err(Error::new_spanned(
                    target,
                    "Only `@target` address expressions are supported",
                ));
            }
            FieldInitializerValue::Target
        } else {
            FieldInitializerValue::Expression(input.parse()?)
        };
        Ok(Self { member, value })
    }
}
