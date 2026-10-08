---
title: 默认值
description: 了解 InitDefault、字段默认值和内置默认值提供者。
---

## 原位默认值需要显式提供

Rust 的 `Default` trait 按值返回对象。`InitDefault<T, Mode>` 则会在给定槽位中构造 `T` 并返回凭证。实现了 `Default` 的类型不会自动实现 `InitDefault`；分离这两种能力可以让依赖地址的默认值在最终位置构造。

如果派生 builder 字段的类型实现了 `InitDefault<Field, Mode>`，可在字段上使用 `#[init(default)]`：

```rust
use inplace_init::{Init, InitTyU8};

#[derive(Init)]
struct Settings {
    #[init(default)]
    enabled: bool,
    #[init(default = InitTyU8::<4>)]
    retries: u8,
}

fn main() {
    let initializer = Settings::init().build();
    let _ = initializer;
}
```

第一个字段使用类型的原位默认值提供者；第二个字段保存显式、不可失败的初始化器。两个字段在类型状态 builder 中都已设置，也都可以通过 setter 覆盖。

在宏层面，`init!(Settings { .. })` 会为整个值请求默认初始化器。花括号中的 `..` 必须是唯一项。如果想混合显式字段值和默认值，请列出每个字段，并为需要 `InitDefault` 的字段写成 `field <- default`。

## 内置提供者

运行时 crate 包含以下原位提供者：

| Feature | 类型 |
| --- | --- |
| 无 feature | `()`、`Option<T>`（构造为 `None`）、`PhantomData<T>`、`PhantomPinned`、元素类型 `T` 有原位默认值时的数组 `[T; N]`、两个元素都支持时的二元组 `(A, B)`，以及 bool/char/整数标量类型 |
| `alloc` | `String`、`Vec`、`VecDeque`、`BinaryHeap`、`LinkedList`、`BTreeMap`、`BTreeSet`，以及当 `T` 有原位默认值时的 `Rc<T>` / `Arc<T>` |
| `std` | `HashMap` 和 `HashSet` |

默认 feature 集为空。`std` 会启用 `alloc`。集合默认值为空集合。数组元素会逐个构造；若某个元素默认构造时 panic，先前已经构造的元素会被析构。

整数和布尔值的内置默认分别为零和 `false`。`InitTy...` const 泛型类型可初始化指定标量值，例如 `InitTyU32::<42>`。

## 适用时使用 `Default::default()`

`InitDefaultValue<T>` 会通过创建普通值并将其写入槽位，把 `T: Default` 适配为 `InitDefault<T, InitMode>`。若类型没有专门的原位提供者，但希望使用标准 `Default` 行为，可使用它。该适配器只支持普通模式。

```rust
use inplace_init::{InitDefaultInit, InitDefaultValue, InitStorage};

fn main() {
    let text = InitStorage::<String>::uninit()
        .initialize_default_with::<InitDefaultValue<String>>();
    assert!(text.is_empty());
}
```

此示例需要 `alloc` feature，因为 `String` 需要分配器。若用于 derive 字段，请将该提供者包装为通用初始化器：

```rust
use inplace_init::{Init, InitDefaultInit, InitDefaultValue};

#[derive(Init)]
struct TextSettings {
    #[init(default = InitDefaultInit::<String, InitDefaultValue<String>>::new())]
    text: String,
}

fn main() {
    let _initializer = TextSettings::init().build();
}
```

## 实现自定义原位默认值

当值必须在目标地址构造时，实现 `InitDefault<T, Mode>`。`InitDefault` 是 unsafe trait：实现必须在成功时留下一个完整的 `T` 并返回唯一凭证；发生 panic 时必须清理部分状态，并使目标恢复为未初始化状态。固定模式的默认实现还必须在最终地址构造依赖地址的字段。

实现契约和固定字段逐项写入示例请见[错误处理与安全](/zh-cn/guides/errors-and-safety/)。如果现有提供者或 `InitDefaultValue<T>` 的语义适合，应优先使用它们。
