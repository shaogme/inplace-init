//! 宏展开各模块共享的标识符、字段键与泛型 token 操作。
//! Shared identifier, field-key, and generic-token operations for macro expansion.

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Expr, Member, ext::IdentExt};

/// 为代码生成提供确定且统一的跨模块辅助操作。
/// Provides deterministic shared operations for code generation.
pub(super) struct MacroSupport;

impl MacroSupport {
    /// 在依赖改名后仍生成调用方实际使用的运行时 crate 路径。
    /// Resolves the runtime crate path even when the dependency is renamed.
    pub(super) fn crate_path() -> TokenStream2 {
        match crate_name("inplace-init") {
            Ok(FoundCrate::Itself) => quote!(::inplace_init),
            Ok(FoundCrate::Name(name)) => {
                let name = name.replace('-', "_");
                let identifier = Ident::new(&name, Span::call_site());
                quote!(::#identifier)
            }
            Err(_) => quote!(::inplace_init),
        }
    }

    /// 用稳定的 FNV-1a 键把字段名编码为宏与运行时共同使用的常量。
    /// Encodes a field name with the stable FNV-1a key shared by macro and runtime.
    pub(super) fn field_key(name: &str) -> u64 {
        name.as_bytes()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    /// 将具名和元组字段统一转换为元数据中的规范名称。
    /// Normalizes named and tuple fields to the canonical metadata name.
    pub(super) fn member_name(member: &Member) -> String {
        match member {
            Member::Named(ident) => ident.unraw().to_string(),
            Member::Unnamed(index) => index.index.to_string(),
        }
    }

    /// 识别字段级 `default` 哨兵表达式，后续改写为原位默认构造器。
    /// Recognizes the field-level `default` sentinel, later rewritten as an in-place default initializer.
    pub(super) fn is_default_expression(expression: &Expr) -> bool {
        matches!(
            expression,
            Expr::Path(path) if path.qself.is_none() && path.path.is_ident("default")
        )
    }

    /// builder 类型参数不得重复声明默认值，避免在生成类型别名位置非法使用默认参数。
    /// Builder type parameters omit defaults to remain valid at generated type-alias positions.
    pub(super) fn strip_generic_defaults(mut generics: syn::Generics) -> syn::Generics {
        for parameter in &mut generics.params {
            match parameter {
                syn::GenericParam::Type(parameter) => parameter.default = None,
                syn::GenericParam::Const(parameter) => parameter.default = None,
                syn::GenericParam::Lifetime(_) => {}
            }
        }
        generics
    }

    /// 为生成的状态参数选择无冲突名称，同时保留稳定的可读前缀。
    /// Chooses collision-free generated state names while keeping a stable readable prefix.
    pub(super) fn unique_ident(base: &str, occupied: &mut Vec<String>) -> Ident {
        let mut candidate = base.to_owned();
        let mut suffix = 0;
        while occupied.iter().any(|existing| existing == &candidate) {
            suffix += 1;
            candidate = format!("{base}{suffix}");
        }
        occupied.push(candidate.clone());
        Ident::new(&candidate, Span::call_site())
    }

    /// 保持原始 lifetime、类型和 const 泛型的声明顺序以重建目标类型。
    /// Preserves lifetime, type, and const parameter order when rebuilding the target type.
    pub(super) fn generic_arguments(generics: &syn::Generics) -> Vec<TokenStream2> {
        generics
            .params
            .iter()
            .map(|parameter| match parameter {
                syn::GenericParam::Lifetime(parameter) => {
                    let lifetime = &parameter.lifetime;
                    quote!(#lifetime)
                }
                syn::GenericParam::Type(parameter) => {
                    let identifier = &parameter.ident;
                    quote!(#identifier)
                }
                syn::GenericParam::Const(parameter) => {
                    let identifier = &parameter.ident;
                    quote!(#identifier)
                }
            })
            .collect()
    }

    /// 将原类型参数、初始化模式和逐字段状态拼成完整 builder 类型。
    /// Combines original type arguments, initialization mode, and per-field states into the builder type.
    pub(super) fn builder_type(
        builder: &Ident,
        original_args: &[TokenStream2],
        mode: TokenStream2,
        states: &[TokenStream2],
    ) -> TokenStream2 {
        let mut arguments = original_args.to_vec();
        arguments.push(mode);
        arguments.extend(states.iter().cloned());
        quote!(#builder<#(#arguments),*>)
    }
}
