---
title: 宏参考
description: Init、init!、pin_init! 和 pin_init_local! 的语法与检查规则。
---

## `#[derive(Init)]`

将 derive 用于结构体：

```rust
#[derive(Init)]
struct Packet {
    id: u32,
    enabled: bool,
}
```

该 derive 会生成 `InitFields` 元数据、字段投影实现和 builder API。它支持具名字段、元组字段、单元结构体及泛型结构体。枚举和 `#[repr(packed)]` 结构体会被拒绝，因为生成代码通过地址投影字段。即使依赖项在 `Cargo.toml` 中被重命名，宏也能解析正确的运行时 crate 路径。

字段属性：

| 属性 | 作用 |
| --- | --- |
| `#[pin]` | 标记需要固定地址初始化的字段。该属性不接受参数。 |
| `#[init(default)]` | 使用 `InitDefault<Field, Mode>` 预先设置 builder 字段。 |
| `#[init(default = initializer)]` | 使用给定的不可失败字段初始化器预先设置 builder 字段。 |

每个字段最多只能有一个 `default` 项。未知 `init` 键会被拒绝。生成的 setter 名称是字段名；元组字段使用 `field_N`，而 `build` 是保留名称。

## `init!` 与 `pin_init!`

字段清单形式如下：

```rust
init!(TypePath {
    field <- initializer,
    another_field <- default,
})
```

元组结构体使用数字成员，例如 `0 <- initializer`。每个字段都必须且只能列出一次。条目顺序就是初始化顺序。使用 `init!` 时，每个表达式都必须在普通模式下实现 `Init<FieldType, Error>`。使用 `pin_init!` 时，`#[pin]` 字段要求实现 `Init<FieldType, Error, PinInitMode>`；未固定字段使用普通可移动初始化器。`default` 标记会在字段所需模式下调用该字段类型的 `InitDefault`。

整个值的默认形式只能包含一个条目：

```rust
init!(TypePath { .. })
```

目标必须实现 `InitDefault<T, Mode>`。`..` 不能和显式字段条目混用。

`init!` 选择 `InitMode`；`pin_init!` 选择 `PinInitMode`。要指定统一错误类型，可在闭合花括号后添加 `? ErrorType`：

```rust
init!(TypePath { field <- initializer } ? AppError)
```

每个字段初始化器都必须产生 `AppError`。对于其他错误类型的字段初始化器，通常调用 `.map_err(...)` 转换。若省略后缀，错误类型为 `Infallible`。

### `@target`

在固定模式宏中，`field <- @target` 会将父值的最终地址传给 `*const Target` 或 `*mut Target` 字段。它在 `init!` 中会被拒绝，也不是通用表达式形式。它不会延长目标生命周期，也不会固定目标存储。

### 编译期检查

字段清单会在常量求值期间与 derive 元数据核对。宏会拒绝重复或缺失的字段，以及通过 `init!` 传入的 `#[pin]` 字段。所有字段都使用 `<-` 语法；固定字段还必须使用固定宏模式。宏也会拒绝重复条目和末尾无法识别的 token。

## `pin_init_local!`

该宏会声明一个局部固定凭证，以及隐藏的局部 `MaybeUninit<T>` 存储：

```rust
pin_init_local! {
    let mut value = TypePath {
        field <- initializer,
    };
}
```

它也接受整个值的 `TypePath { .. }` 形式。可选的 `? ErrorType` 后缀写在初始化器花括号之后，并使用 `?` 传播；因此应在返回兼容 `Result` 的函数内使用。若省略错误后缀，宏会自行处理 `Infallible`。

## 如何选择入口

| 需求 | API |
| --- | --- |
| 在借用存储中构造可移动值 | `init_in(&mut MaybeUninit<T>, init!(...))` |
| 在局部存储中构造固定值 | `pin_init_local!` |
| 在调用方管理的存储中构造固定值 | `unsafe pin_init_in(Pin<&mut MaybeUninit<T>>, pin_init!(...))` |
| 创建可移动或固定的堆拥有者 | 启用 `alloc` 后使用 `.init_box()` / `.pin_box()` |
