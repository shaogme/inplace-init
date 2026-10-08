---
title: Errors and safety
description: Handle fallible field initialization and understand the contracts behind unsafe initialization APIs.
---

## Fallible field initializers

An initializer can return an application error. `init!` uses `Infallible` when no error type is written; add `? ErrorType` after the field list to select a shared error type for the generated initializer. Each field initializer must then use that error type, directly or through `map_err`.

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

If a later field fails, receipts for previously initialized fields are dropped automatically. The parent slot is not converted into a receipt. Panic unwinding follows the same cleanup path for fields whose initializers already returned receipts.

When an initializer is statically infallible, use `Infallible` or let the macro choose it. For a fallible initializer, prefer a concrete application error and map individual field errors into it. `MapInitError` does not run its closure on success.

## Contracts for custom implementations

`Init<T, E, Mode>` is an **unsafe trait** and the extension point for field and whole-value initializers. Implementations must:

- construct exactly one valid `T` on success and return the sole receipt responsible for dropping it;
- drop every initialized value and partial field on `Err` or panic, leaving the slot uninitialized;
- retain receipts for earlier fields until every later field succeeds, then transfer ownership to the complete value;
- uphold the address requirements of `Mode`.

`init_in` is safe because its `Init` implementation carries this unsafe obligation. `InitFn::new` is safe: its closure returns a complete `T` by value, and the adapter writes it only after the closure succeeds. An error or panic therefore leaves the target slot untouched. For direct slot access, `InitFnRaw::new` is unsafe; its caller must ensure the closure follows the `Init` rules on success, error, panic, and for the selected address mode.

`InitDefault<T, Mode>` is an **unsafe trait**. Its implementation must:

- return a receipt for exactly one valid `T` on success;
- clean up every initialized part and leave the slot uninitialized if it panics;
- avoid letting partially initialized state escape before returning the receipt;
- honor the address guarantee of `Mode`, especially for `PinInitMode`.

`InitRaw<T, E, Mode>` is also an **unsafe trait**. Its success and rollback guarantees match `Init`: construct exactly one valid target and transfer its sole destruction responsibility to the `RawInit` adapter on success; clean all initialized state and leave the target uninitialized on error or panic. `RawInit` creates the whole-value receipt only after `InitRaw` succeeds. Prefer `InitSlot::write` through a carefully audited `Init` implementation when raw pointer access is unnecessary.

## Pinning and unsafe entry points

`pin_init_in` is unsafe because its caller owns the backing-storage invariant: the storage must remain at the same address until the receipt is dropped. If the receipt is forgotten, the storage must never be moved, reused, or exposed as uninitialized again. Use `pin_init_local!` or `.pin_box()` when either can own the lifetime and storage more clearly.

Do not use `MaybeUninit::assume_init`, `InitSlot::__assume_init`, raw pointer writes, or `mem::forget` as a shortcut around these contracts. `InitReceipt` is the sole owner of the target's destruction responsibility; leaking it leaks that responsibility too.
