---
title: Derive and builder
description: Generate a type-state builder for named, tuple, and generic structs.
---

## Derive the initializer

`#[derive(Init)]` supports structs with named fields, tuple fields, and no fields. Enums and `#[repr(packed)]` structs are rejected because the generated initializer needs to project aligned field addresses.

For a named struct, `Type::init()` creates a movable builder. Each required field starts as `Unset`; each field with an `#[init(default...)]` attribute starts with its declared default initializer. Setter methods consume the current builder and return a new builder type with that field's state changed. This is a type-state API: attempting to initialize a builder with a required field still unset fails trait checking at compile time.

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

`build()` returns the initializer value. Completeness is enforced when an API requires `Init<RetryPolicy, ...>`; a builder with an `Unset` required field does not implement that trait. A defaulted field can still be overridden by calling its setter.

## Defaults on builder fields

`#[init(default)]` asks `InitDefault<FieldType, Mode>` to construct that field directly in its slot. `#[init(default = expression)]` stores an initializer expression for the field. That expression must implement `Init<FieldType, Infallible, Mode>`; it is not an ordinary field value.

For example, `InitTyU8::<3>` is an initializer that writes the scalar value `3`. A custom field initializer can be supplied with the setter in exactly the same way:

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

Tuple struct fields use setters named `field_0`, `field_1`, and so on. Generic structs are supported; the generated builder carries the source type's lifetime, type, and const generic arguments.

## Pinned builders

Every derived struct gets `Type::pin_init()`. If the struct has any `#[pin]` field, the movable `Type::init()` method is omitted. Pinned fields have setters that require `Init<FieldType, Error, PinInitMode>`. Unpinned fields can use ordinary initializers inside the pinned builder because their slots are safely adapted to movable mode.

The builder's `build()` result is an initializer; use `.pin_box()` with `alloc`, or pass it to a pinned initialization entry point. See [pinned initialization](/guides/pinning/) for storage and address requirements.

## Attribute and naming constraints

- `#[pin]` is a bare field attribute and does not accept arguments.
- `#[init(...)]` accepts one `default` item, optionally followed by `= initializer`.
- The generated setter name is the field name (or `field_N` for tuple fields). A field cannot generate a setter named `build`.
- Repeating `#[pin]` does not change its meaning; repeated defaults and unknown `init` keys are compile errors.
