---
title: 错误处理与安全
description: 处理可能失败的字段初始化，并理解 unsafe 初始化 API 的契约。
---

## 可能失败的字段初始化器

初始化器可以返回应用错误。如果未指定错误类型，`init!` 会使用 `Infallible`；在字段清单后添加 `? ErrorType`，可为生成的初始化器指定统一错误类型。之后每个字段初始化器都必须直接使用该错误类型，或通过 `map_err` 转换成该类型。

```rust
use core::mem::MaybeUninit;
use inplace_init::{init, init_in, Init, InitReceipt, InitSlot, InitTyU8};

#[derive(Debug)]
enum BuildError {
    Rejected,
}

struct Reject;

// 安全性依据：该初始化器在写入前返回错误，因此目标保持未初始化。
// Safety: this initializer returns before writing, so the target stays uninitialized on error.
unsafe impl Init<u16, BuildError> for Reject {
    fn initialize<'slot>(
        self,
        _slot: InitSlot<'slot, u16>,
    ) -> Result<InitReceipt<'slot, u16>, BuildError> {
        Err(BuildError::Rejected)
    }
}

#[derive(Init)]
struct Record {
    code: u16,
    flags: u8,
}

fn main() {
    let initializer = init!(Record {
        code <- Reject,
        flags <- InitTyU8::<1>.map_err(|never| -> BuildError { match never {} }),
    } ? BuildError);

    let mut storage = MaybeUninit::<Record>::uninit();
    let result = init_in(&mut storage, initializer);
    assert!(matches!(result, Err(BuildError::Rejected)));
}
```

后续字段失败时，先前已初始化字段的凭证会自动析构。父槽位不会转成初始化完成的凭证。若初始化器已经返回了字段凭证，之后发生 panic 时也会通过同一机制清理字段。

初始化器在类型层面不可失败时，可使用 `Infallible`，也可让宏自动选择它。对于可能失败的初始化器，建议使用具体的应用错误，并将各字段错误映射到该类型。成功时不会调用 `MapInitError` 的映射闭包。

## 自定义实现的契约

`Init<T, E, Mode>` 是 **unsafe trait**，也是实现字段初始化器和完整值初始化器的扩展点。实现必须：

- 成功时恰好构造一个有效的 `T`，并返回唯一负责析构它的凭证；
- 返回 `Err` 或 panic 时销毁所有已构造的值和部分字段，使槽位保持未初始化；
- 在后续字段全部成功前保留先前字段的凭证，之后才把所有权转交给完整值；
- 遵守 `Mode` 的地址约束。

`init_in` 是安全函数，因为这些义务由 `Init` 的 unsafe 实现承担。`InitFn::new` 是安全函数：闭包按值返回完整的 `T`，适配器只会在闭包成功后将其写入槽位，因此错误或 panic 不会触碰目标槽位。若要直接操作槽位，可使用 `InitFnRaw::new`；它是 unsafe 函数，调用方必须保证闭包在成功、错误、panic 和所选地址模式下都遵守 `Init` 契约。

`InitDefault<T, Mode>` 是一个 **unsafe trait**。实现必须：

- 成功时为且仅为一个有效 `T` 返回凭证；
- panic 时清理所有已构造部分，并使槽位恢复为未初始化状态；
- 不得在返回凭证前让部分初始化状态逃逸；
- 遵守 `Mode` 的地址保证，尤其是 `PinInitMode`。

`InitRaw<T, E, Mode>` 同样是一个 **unsafe trait**，成功与回滚保证和 `Init` 一致：成功时恰好构造一个有效目标，并由 `RawInit` 适配器接管唯一析构责任；返回错误或 panic 时清理所有已构造状态，使目标保持未初始化。`RawInit` 只会在 `InitRaw` 成功后创建整体凭证。如果不需要裸指针访问，应优先通过经过审查的 `Init` 实现和 `InitSlot::write` 完成初始化。

## 固定地址与 unsafe 入口

`pin_init_in` 是 unsafe，因为调用方负责后备存储的不变式：凭证销毁前，存储必须保持在同一地址。如果凭证被遗忘，存储永远不能移动、复用或再次暴露为未初始化状态。如果 `pin_init_local!` 或 `.pin_box()` 能更清楚地管理生命周期和存储，请优先使用它们。

不要把 `MaybeUninit::assume_init`、`InitSlot::__assume_init`、裸指针写入或 `mem::forget` 当作绕过这些契约的捷径。`InitReceipt` 是目标值析构责任的唯一拥有者；遗忘它也会遗失这项责任。
