---
title: derive 与 builder
description: 为具名结构体、元组结构体和泛型结构体生成类型状态 builder。
---

## 派生初始化器

`#[derive(Init)]` 支持具名字段结构体、元组结构体和单元结构体。枚举及 `#[repr(packed)]` 结构体会被拒绝，因为生成的初始化器需要投影对齐的字段地址。

对于具名字段结构体，`Type::init()` 会创建普通模式 builder。必填字段初始状态为 `Unset`；使用 `#[init(default...)]` 声明默认值的字段则从对应默认初始化器开始。setter 会消费当前 builder，并返回一个类型状态不同的新 builder。尝试初始化仍有必填字段处于 `Unset` 状态的 builder 时，trait 检查会在编译期失败。

```rust
use core::mem::MaybeUninit;
use inplace_init::{init_in, Init, InitTyU8, InitTyU16};

#[derive(Init)]
struct RetryPolicy {
    #[init(default = InitTyU8::<3>)]
    retries: u8,
    timeout_ms: u16,
}

fn main() {
    let initializer = RetryPolicy::init()
        .timeout_ms(InitTyU16::<250>)
        .build();
    let mut storage = MaybeUninit::<RetryPolicy>::uninit();
    let policy = init_in(&mut storage, initializer).unwrap();

    assert_eq!(policy.retries, 3);
    assert_eq!(policy.timeout_ms, 250);
}
```

`build()` 返回初始化器值。只有在某个 API 要求 `Init<RetryPolicy, ...>` 时才会检查完整性；必填字段仍为 `Unset` 的 builder 不实现该 trait。默认字段仍可通过 setter 覆盖。

## Builder 字段默认值

`#[init(default)]` 会要求 `InitDefault<FieldType, Mode>` 在字段槽位中直接构造该字段。`#[init(default = expression)]` 则保存一个字段初始化器表达式。表达式必须实现 `Init<FieldType, Infallible, Mode>`；它不是普通字段值。

例如，`InitTyU8::<3>` 是一个向目标位置写入标量值 `3` 的初始化器。自定义字段初始化器也可以通过同样的 setter 传入：

```rust
use core::mem::MaybeUninit;
use inplace_init::{init_in, Init, InitTyU8, InitTyU16};

#[derive(Init)]
struct RetryPolicy {
    #[init(default = InitTyU8::<3>)]
    retries: u8,
    timeout_ms: u16,
}

fn main() {
    let initializer = RetryPolicy::init()
        .retries(InitTyU8::<5>)
        .timeout_ms(InitTyU16::<500>)
        .build();
    let mut storage = MaybeUninit::<RetryPolicy>::uninit();
    let policy = init_in(&mut storage, initializer).unwrap();
    assert_eq!(policy.retries, 5);
}
```

元组结构体字段的 setter 名称为 `field_0`、`field_1` 等。泛型结构体也受支持；生成的 builder 会携带源类型的生命周期、类型和 const 泛型参数。

## 固定地址 builder

每个派生结构体都有 `Type::pin_init()`。如果结构体包含任何 `#[pin]` 字段，则不会生成普通模式的 `Type::init()`。固定字段的 setter 要求 `Init<FieldType, Error, PinInitMode>`。固定 builder 中的未固定字段可以使用普通初始化器，因为它们的槽位会安全地适配为可移动模式。

Builder 的 `build()` 结果是初始化器；启用 `alloc` 时可调用 `.pin_box()`，也可将它交给固定地址初始化入口。存储与地址要求详见[固定地址初始化](/zh-cn/guides/pinning/)。

## 属性与命名约束

- `#[pin]` 是不带参数的字段属性。
- `#[init(...)]` 只接受一个 `default` 项，可选地写成 `= initializer`。
- 生成的 setter 名称与字段名相同（元组字段为 `field_N`）。字段不能生成名为 `build` 的 setter。
- 重复 `#[pin]` 不会改变语义；重复默认项和未知的 `init` 键会导致编译错误。
