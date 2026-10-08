---
title: 固定地址初始化
description: 在地址敏感值的最终位置构造它，并在之后保持地址稳定。
---

## 何时需要固定地址

大多数值使用 `InitMode` 即可；构造后移动它们没有问题。只有当类型的不变式依赖初始化后的地址时才需要固定，例如类型保存了指向自身字段的指针。`PinInitMode` 会让初始化器在建立此类不变式之前获知最终目标地址。

固定地址本身不会让任意自引用变得安全。初始化器必须根据最终目标地址构造依赖地址的字段，并且只要固定值仍存活，后备存储就不能移动。

## 标记固定字段

使用 `#[pin]` 标记参与类型固定不变式的字段。若结构体包含任何固定字段，它会生成 `Type::pin_init()`，但不会生成普通模式 builder 入口 `Type::init()`。`pin_init!` 和 `pin_init_local!` 要求提供结构体的每个字段；编译期检查会拒绝通过普通 `init!` 初始化 `#[pin]` 字段。

```rust
use core::{marker::PhantomPinned, ptr};
use inplace_init::{pin_init_local, Init};

#[derive(Init)]
struct SelfRef {
    #[pin]
    address: *const SelfRef,
    #[pin]
    _pinned: PhantomPinned,
}

fn main() {
    pin_init_local! {
        let mut value = SelfRef {
            address <- @target,
            _pinned <- default,
        };
    }

    assert_eq!(value.address, ptr::addr_of!(*value));
}
```

`@target` 会把父值的最终地址写入 `*const SelfRef` 或 `*mut SelfRef` 裸指针字段。它只受 `pin_init!` 和 `pin_init_local!` 支持。裸指针本身不会让目标保持存活；凭证与存储必须共同保持在作用域中。

`pin_init_local!` 会创建局部 `MaybeUninit<SelfRef>` 和固定凭证。生成的声明顺序保证凭证先于后备存储析构。可以通过凭证读取绑定；如果 API 明确要求 `Pin`，请调用 `value.as_pin_ref()` 或 `value.as_pin_mut()`。

## 选择固定值的拥有者

常见模式有三种：

1. **局部存储：** `pin_init_local!` 是在词法作用域内创建固定值的最简单方式，且无需启用 `alloc`。
2. **堆分配：** 启用 `alloc` 后，创建 `pin_init!(Type { ... })` 初始化器并调用 `.pin_box()`。返回的 `Pin<Box<T>>` 拥有稳定分配。
3. **调用方管理的存储：** 将 `pin_init!` 与 `pin_init_in(Pin<&mut MaybeUninit<T>>, initializer)` 一起使用。该函数是 unsafe，因为调用方必须保证凭证销毁前后备存储始终固定。如果凭证被遗忘，不得移动、复用该存储，也不得再次将它暴露为待初始化存储。

`pin_init!` 只创建初始化器；它本身不会分配或固定目标。地址敏感值不要使用 `InitStorage<T>` 和 `InitValue<T>`，因为它们的普通 API 允许移动值。

## 初始化器要求

`#[pin]` 字段的 setter 或字段初始化器必须实现 `Init<Field, Error, PinInitMode>`。同一固定结构体中的未固定字段可以使用普通 `Init<Field, Error>`；derive 生成的适配器只会重新标记该未固定字段的槽位。字段级 `default` 表达式也会根据字段模式进行检查。

固定结构体级默认值需要实现 `InitDefault<T, PinInitMode>`，并在给定目标位置直接写入值。若类型需要 unsafe 字段写入，请先阅读[错误处理与安全](/zh-cn/guides/errors-and-safety/)。
