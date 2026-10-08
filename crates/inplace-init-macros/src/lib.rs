use proc_macro::TokenStream;
use syn::{DeriveInput, Error, parse_macro_input};

mod derive;
mod initializer;
mod support;

/// 为结构体生成类型状态构造器；字段可用 `#[init(default)]` 或
/// `#[init(default = 初始化器)]` 声明默认初始化方式。
/// Generate a typestate builder for a struct; fields may declare defaults with `#[init(default)]`
/// or `#[init(default = initializer)]`.
#[proc_macro_derive(Init, attributes(init, pin))]
pub fn derive_init(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::DeriveMacro::expand(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// 为结构体创建原地初始化器；`Type { .. }` 会在目标槽位调用 `InitDefault`。
/// Create an in-place initializer for a struct; `Type { .. }` calls `InitDefault` in its target slot.
#[proc_macro]
pub fn init(input: TokenStream) -> TokenStream {
    initializer::InitializerMacro::expand(input, false)
}

/// 为结构体创建固定地址初始化器；字段可用 `default` 在字段槽位中构造默认值。
/// Create a pinned initializer for a struct; fields can use `default` to construct in their slots.
#[proc_macro]
pub fn pin_init(input: TokenStream) -> TokenStream {
    initializer::InitializerMacro::expand(input, true)
}

/// 在当前作用域创建固定存储；返回的凭证在存储之前析构。
/// Create pinned storage in the current scope; its receipt is dropped before the storage.
#[proc_macro]
pub fn pin_init_local(input: TokenStream) -> TokenStream {
    initializer::InitializerMacro::expand_pin_local(input)
}
