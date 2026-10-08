---
title: Getting started
description: Add inplace-init to a Rust project and initialize your first struct in its destination storage.
---

## Add the dependency

```toml
[dependencies]
inplace-init = "0.1"
```

The crate has no default features and supports `no_std`; enable `alloc` or `std` only if your application needs allocation-backed APIs.

## Initialize a struct in caller-owned storage

Derive `Init` for the struct, describe how to initialize each field with `init!`, then pass that initializer and a `MaybeUninit<T>` slot to `init_in`:

```rust
use core::mem::MaybeUninit;
use inplace_init::{init, init_in, Init, InitTyU16, InitTyU8};

#[derive(Init)]
struct Message {
    code: u16,
    flags: u8,
}

fn main() {
    let mut storage = MaybeUninit::<Message>::uninit();
    let message = init_in(
        &mut storage,
        init!(Message {
            code <- InitTyU16::<7>,
            flags <- InitTyU8::<1>,
        }),
    )
    .unwrap();

    assert_eq!(message.code, 7);
    assert_eq!(message.flags, 1);
}
```

The `<-` form names the field and supplies an initializer for it. The fields may appear in any order, but each field must appear exactly once. The macro projects each field slot from the destination; it does not first assemble a temporary `Message` and then move that struct into storage.

`init_in` returns an `InitReceipt`. It borrows the storage exclusively, dereferences to the initialized `Message`, and drops the value when the receipt is dropped. Keep the receipt alive while using the value. For movable values, `into_inner()` moves the value out and ends the receipt's destruction responsibility.

## Choose storage ownership

Use `init_in` when you already own a destination such as a stack `MaybeUninit<T>` and want the receipt to borrow that storage. For a simple owning wrapper, `InitStorage<T>` consumes its uninitialized storage and returns an `InitValue<T>`:

```rust
use inplace_init::{init, Init, InitStorage, InitTyU16, InitTyU8};

#[derive(Init)]
struct Message {
    code: u16,
    flags: u8,
}

fn main() {
    let message = InitStorage::<Message>::uninit()
        .initialize(init!(Message {
            code <- InitTyU16::<8>,
            flags <- InitTyU8::<0>,
        }))
        .unwrap();

    assert_eq!(message.code, 8);
}
```

`InitValue` owns and drops the value, and provides shared/mutable access plus `into_inner()`. It is a normal movable owner, not a pinned owner.

## Where to go next

- Read the [initialization model](/concepts/initialization-model/) to understand slots, receipts, and failure cleanup.
- Use the [derive builder](/guides/derive-builder/) when a type has defaults or you prefer chained field setters.
- Read [pinned initialization](/guides/pinning/) before constructing address-sensitive values.
- See [macro syntax](/reference/macros/) for tuple fields, default forms, and fallible initializers.
