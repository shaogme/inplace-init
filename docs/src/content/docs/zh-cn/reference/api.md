---
title: 运行时 API
description: 初始化器 trait、槽位、凭证、默认值提供者和存储拥有者参考。
---

## 核心 trait 与类型

| API | 用途 |
| --- | --- |
| `Init<T, E = Infallible, Mode = InitMode>` | unsafe trait。消费 `InitSlot` 并返回凭证或错误；实现者负责成功、回滚和地址模式保证。`map_err` 会包装初始化器，并仅在失败时映射错误。 |
| `InitSlot<'slot, T, Mode>` | 对独占且尚未初始化目标的一次性操作能力。`write(value)` 会写入完整值并返回凭证。 |
| `InitReceipt<'slot, T, Mode>` | 持续借用目标并拥有析构责任。销毁它会析构目标。 |
| `InitMode` | 普通可移动模式。此模式下可使用 `InitReceipt::into_inner`。 |
| `PinInitMode` | 固定地址模式。凭证提供 `as_pin_ref` 和 `as_pin_mut`，但不能移出值。 |
| `InitDefault<T, Mode>` | 在目标槽位中直接构造默认值的 unsafe 能力。 |
| `InitDefaultInit<T, Provider>` | 将 `InitDefault` 提供者适配为 `Init<T, Infallible, Mode>`。 |
| `InitDefaultValue<T>` | 在普通模式下按值调用 `T::default()`，作为默认值提供者。 |
| `InitFn<T, E, F>` | 在普通可移动模式下安全地适配无参数闭包 `FnOnce() -> Result<T, E>`；闭包返回的值会写入目标槽位。 |
| `InitFnRaw<T, E, F, Mode>` | 包装一个高阶生命周期闭包；闭包接收 `InitSlot` 并返回对应凭证或错误。`InitFnRaw::new` 是 unsafe 函数，因为闭包必须遵守 `Init` 契约。 |
| `InitRaw<T, E, Mode>` / `RawInit<I>` | 裸指针初始化的 unsafe 契约及其 `Init` 适配器。 |

两种模式下 `InitReceipt` 都实现 `Deref<Target = T>`。只有普通模式凭证实现 `DerefMut`；固定模式凭证提供固定借用。每个凭证的生命周期都绑定到创建其槽位时取得的独占借用。

## 入口函数

```rust
pub fn init_in<'slot, T, I, E>(
    slot: &'slot mut MaybeUninit<T>,
    initializer: I,
) -> Result<InitReceipt<'slot, T, InitMode>, E>
where
    I: Init<T, E>;
```

`init_in` 是安全函数，因为独占的 `&mut MaybeUninit<T>` 借用提供了目标存储保证，而 `Init` 的 unsafe 实现负责确保错误或 panic 时槽位保持未初始化。

```rust
pub unsafe fn pin_init_in<'slot, T, I, E>(
    slot: Pin<&'slot mut MaybeUninit<T>>,
    initializer: I,
) -> Result<InitReceipt<'slot, T, PinInitMode>, E>
where
    I: Init<T, E, PinInitMode>;
```

`pin_init_in` 是 unsafe。调用方必须在凭证销毁前保持目标地址稳定。如果凭证被遗忘，存储必须继续固定，且不得复用。

## 存储拥有者

`InitStorage<T>::uninit()` 创建拥有型未初始化存储。`.initialize(initializer)` 会消费它，并返回 `Result<InitValue<T>, E>`。`.initialize_default()` 使用 `T` 作为 `InitDefault` 提供者；`.initialize_default_with::<Provider>()` 可选择其他提供者。`InitValue<T>` 拥有已初始化的可移动值，并提供 `get`、`get_mut`、`into_inner` 和解引用访问。

启用 `alloc` feature 后：

- `InitBoxExt<T, E>::init_box()` 会初始化可移动的 `Box<T>`。
- `PinInitBoxExt<T, E>::pin_box()` 会在分配的最终地址构造 `Pin<Box<T>>`。

这两个扩展 trait 都会为满足模式约束的初始化器类型提供 blanket 实现。

## derive 生成的字段元数据

`InitFields`、`InitField<KEY>`、`InitFieldInit`、`InitState`、`Set`、`Unset`、`MovableField` 和 `PinnedField` 服务于宏与 builder 生成代码。它们的方法和字段属于隐藏实现细节。应用代码应使用 `#[derive(Init)]`、builder 方法和公开初始化入口，而不是手动实现这些元数据 trait。

`InitFieldInit` 和 `InitState` 是 unsafe trait，因为宏生成的 `Init` 实现依赖它们提供回滚保证。

标量初始化器类型包括 `InitTyBool`、`InitTyChar`、`InitTyI8` 至 `InitTyI128`、`InitTyIsize`、`InitTyU8` 至 `InitTyU128` 和 `InitTyUsize`，它们都是 const 泛型提供者。例如，`InitTyU32::<42>` 可在任一地址模式下将 `u32` 初始化为 `42`。

## Cargo feature

| Cargo feature | 增加的能力 |
| --- | --- |
| 默认（空） | 核心无分配 API 及核心类型默认值 |
| `alloc` | 依赖分配器的默认值提供者以及 `init_box` / `pin_box` |
| `std` | 启用 `alloc`，并为 `HashMap` / `HashSet` 提供默认值 |
