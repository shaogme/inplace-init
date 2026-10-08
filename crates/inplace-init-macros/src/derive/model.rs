//! 派生宏与 builder 生成器之间传递的字段和参数模型。
//! Field and parameter models passed between derive expansion and builder generation.

use proc_macro2::{Ident, Span};
use syn::{
    Attribute, Error, Expr, Fields, Member, Meta, MetaList, Result, Token, Type, ext::IdentExt,
    punctuated::Punctuated,
};

/// 描述一个 derive 字段在源类型与生成 API 中的对应关系。
/// Describes how a derive field maps to the source type and generated API.
///
/// 字段名用于元数据与稳定键，`member` 用于投影地址，`method` 则是 builder setter 名称；
/// `default_helper` 仅为表达式默认值生成类型适配器。
/// The name feeds metadata and stable keys, `member` projects the address, and `method` names the setter;
/// `default_helper` exists only to adapt expression-based defaults to a concrete initializer type.
pub(super) struct DeriveField {
    pub(super) name: String,
    pub(super) member: Member,
    pub(super) method: Ident,
    pub(super) ty: Type,
    pub(super) pinned: bool,
    pub(super) default: Option<FieldDefault>,
    pub(super) default_helper: Option<Ident>,
}

/// 表示字段由 trait 默认构造，或由用户表达式构造。
/// Distinguishes trait-based defaults from user-provided initializer expressions.
pub(super) enum FieldDefault {
    InitDefault,
    Initializer(Expr),
}

/// 将源结构体信息显式交给独立的 builder 展开器。
/// Explicitly passes source-struct information to the separate builder expander.
pub(super) struct BuilderInput<'a> {
    pub(super) name: &'a Ident,
    pub(super) visibility: &'a syn::Visibility,
    pub(super) generics: &'a syn::Generics,
    pub(super) fields_definition: &'a Fields,
    pub(super) fields: &'a [DeriveField],
    pub(super) root: &'a proc_macro2::TokenStream,
}

/// 描述一个 setter 的模式替换与目标字段。
/// Describes a setter's type-state substitution and target field.
pub(super) struct BuilderSetterInput<'a> {
    pub(super) builder: &'a Ident,
    pub(super) generics: &'a syn::Generics,
    pub(super) original_args: &'a [proc_macro2::TokenStream],
    pub(super) fields: &'a [DeriveField],
    pub(super) target_index: usize,
    pub(super) target: &'a DeriveField,
    pub(super) mode_parameter: &'a Ident,
    pub(super) state_parameters: &'a [Ident],
    pub(super) state_fields: &'a [Ident],
    pub(super) root: &'a proc_macro2::TokenStream,
    pub(super) pinned_mode: bool,
}

/// 描述 builder 最终初始化实现所需的类型与字段状态。
/// Describes the types and field states required by the builder's final initialization impl.
pub(super) struct BuilderBuildInput<'a> {
    pub(super) name: &'a Ident,
    pub(super) builder: &'a Ident,
    pub(super) error_name: &'a Ident,
    pub(super) generics: &'a syn::Generics,
    pub(super) fields_definition: &'a Fields,
    pub(super) fields: &'a [DeriveField],
    pub(super) original_args: &'a [proc_macro2::TokenStream],
    pub(super) state_parameters: &'a [Ident],
    pub(super) state_fields: &'a [Ident],
    pub(super) root: &'a proc_macro2::TokenStream,
    pub(super) pinned_mode: bool,
}

/// 集中执行字段属性语义检查，避免 derive 主流程混入语法细节。
/// Centralizes field-attribute validation so the derive flow stays separate from syntax details.
pub(super) struct DeriveModel;

impl DeriveModel {
    /// 判断 repr 属性是否包含 packed；packed 字段无法安全地按地址投影。
    /// Detects `repr(packed)`, whose fields cannot be safely projected by address.
    pub(super) fn has_packed_repr(attributes: &[Attribute]) -> Result<bool> {
        for attribute in attributes {
            if !attribute.path().is_ident("repr") {
                continue;
            }

            let reprs =
                attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            if reprs.iter().any(|repr| match repr {
                Meta::Path(path) | Meta::List(MetaList { path, .. }) => path.is_ident("packed"),
                Meta::NameValue(value) => value.path.is_ident("packed"),
            }) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// 验证 pin 标记是无参数属性；重复标记不会改变字段语义。
    /// Requires `pin` to be a bare attribute; repeating it does not change field semantics.
    pub(super) fn has_pin_attr(attributes: &[Attribute]) -> Result<bool> {
        let mut pinned = false;
        for attribute in attributes {
            if attribute.path().is_ident("pin") {
                if !matches!(&attribute.meta, Meta::Path(_)) {
                    return Err(Error::new_spanned(
                        attribute,
                        "`#[pin]` does not accept arguments",
                    ));
                }
                pinned = true;
            }
        }
        Ok(pinned)
    }

    /// 读取单个 init 默认声明，并拒绝未知键或重复默认项。
    /// Reads one `init` default declaration and rejects unknown or repeated entries.
    pub(super) fn init_default_attr(attributes: &[Attribute]) -> Result<Option<FieldDefault>> {
        let mut default = None;
        for attribute in attributes {
            if !attribute.path().is_ident("init") {
                continue;
            }

            let mut argument_count = 0;
            attribute.parse_nested_meta(|meta| {
                argument_count += 1;
                if !meta.path.is_ident("default") {
                    return Err(meta.error(
                        "The `init` attribute supports only `default` or `default = <initializer>`",
                    ));
                }
                if default.is_some() {
                    return Err(meta.error("`default` can only be specified once"));
                }
                default = Some(if meta.input.peek(Token![=]) {
                    FieldDefault::Initializer(meta.value()?.parse()?)
                } else {
                    FieldDefault::InitDefault
                });
                Ok(())
            })?;
            if argument_count == 0 {
                return Err(Error::new_spanned(
                    attribute,
                    "The `init` attribute requires `default` or `default = <initializer>`",
                ));
            }
        }
        Ok(default)
    }

    /// 生成的 build 方法占用固定名称，因此字段 setter 不得与之重名。
    /// The generated `build` method owns a fixed name, so no field setter may collide with it.
    pub(super) fn validate_builder_method_names(fields: &[DeriveField]) -> Result<()> {
        let mut methods = Vec::<(String, Span)>::new();
        methods.push(("build".to_owned(), Span::call_site()));
        for field in fields {
            let method = field.method.unraw().to_string();
            if methods.iter().any(|(existing, _)| existing == &method) {
                return Err(Error::new_spanned(
                    &field.method,
                    "Field name conflicts with a generated builder method",
                ));
            }
            methods.push((method, field.method.span()));
        }
        Ok(())
    }
}
