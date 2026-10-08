---
title: Pinned initialization
description: Initialize address-sensitive values at their final location and keep them pinned.
---

## When pinning is needed

Most values should use `InitMode`; moving them after construction is fine. Pinning is for a type whose invariants depend on its address after initialization, for example a type that stores a pointer to one of its own fields. `PinInitMode` lets the initializer receive the final destination before it establishes that invariant.

Pinning does not make arbitrary self-references safe. The initializer must create any address-dependent fields using the final target address, and the storage must not move for as long as the pinned value is alive.

## Mark pinned fields

Use `#[pin]` on fields that participate in the type's pinning invariant. A struct with any pinned field has `Type::pin_init()` and does not get the movable `Type::init()` builder entry point. `pin_init!` and `pin_init_local!` require every field in the struct, and compile-time validation rejects an attempt to initialize a `#[pin]` field through ordinary `init!`.

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

`@target` initializes a raw `*const SelfRef` or `*mut SelfRef` field with the final address of the parent. It is supported only by `pin_init!` and `pin_init_local!`. The pointer itself does not keep the target alive; the receipt and storage must remain in scope together.

`pin_init_local!` creates a local `MaybeUninit<SelfRef>` and a pinned receipt. The generated declaration order ensures the receipt drops before its backing storage. The binding can be read through the receipt; use `value.as_pin_ref()` or `value.as_pin_mut()` when an API needs `Pin` explicitly.

## Choose a pinned owner

There are three common patterns:

1. **Local storage:** `pin_init_local!` is the simplest way to create a pinned value for a lexical scope without enabling `alloc`.
2. **Heap allocation:** with the `alloc` feature, create a `pin_init!(Type { ... })` initializer and call `.pin_box()`. The returned `Pin<Box<T>>` owns the stable allocation.
3. **Caller-managed storage:** use `pin_init!` with `pin_init_in(Pin<&mut MaybeUninit<T>>, initializer)`. This function is unsafe because you must guarantee that the backing storage remains pinned until the receipt is dropped. If the receipt is forgotten, do not move, reuse, or expose the storage for another initialization.

`pin_init!` creates an initializer; by itself it does not allocate or pin its target. Avoid `InitStorage<T>` and `InitValue<T>` for address-sensitive values because their ordinary API permits moving the value.

## Initializer requirements

For a `#[pin]` field, a setter or field initializer must implement `Init<Field, Error, PinInitMode>`. An unpinned field in the same pinned struct may use an ordinary `Init<Field, Error>`; the derive-generated adapter retags only that unpinned field's slot. A field-level `default` expression is checked in the field's appropriate mode.

For a pinned struct-level default, implement `InitDefault<T, PinInitMode>` so it writes the value directly at the supplied target. For types that require unsafe field writes, read [errors and safety](/guides/errors-and-safety/) before implementing it.
