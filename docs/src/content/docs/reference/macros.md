---
title: Macro reference
description: Syntax and validation rules for Init, init!, pin_init!, and pin_init_local!.
---

## `#[derive(Init)]`

Apply the derive to a struct:

```rust
#[derive(Init)]
struct Packet {
    id: u32,
    enabled: bool,
}
```

The derive emits `InitFields` metadata, field projection implementations, and builder APIs. It supports named, tuple, and unit structs, including generic structs. It rejects enums and `#[repr(packed)]` because the generated code projects fields by address. The runtime path is resolved even if the dependency is renamed in `Cargo.toml`.

Field attributes:

| Attribute | Effect |
| --- | --- |
| `#[pin]` | Marks a field as requiring pinned initialization. The attribute takes no arguments. |
| `#[init(default)]` | Prepopulates the builder with `InitDefault<Field, Mode>`. |
| `#[init(default = initializer)]` | Prepopulates the builder with this infallible field initializer. |

Only one `default` item is allowed per field. Unknown `init` keys are rejected. Generated setter names use the field name, or `field_N` for tuple fields; `build` is reserved.

## `init!` and `pin_init!`

The field-list form is:

```rust
init!(TypePath {
    field <- initializer,
    another_field <- default,
})
```

Use a numeric member for tuple structs, such as `0 <- initializer`. Each field must be listed exactly once. The order of entries is the initialization order. With `init!`, each expression must implement `Init<FieldType, Error>` in movable mode. With `pin_init!`, `#[pin]` fields require `Init<FieldType, Error, PinInitMode>`; unpinned fields use ordinary movable initializers. The `default` sentinel requests `InitDefault` for that field in its required mode.

The whole-value default form has exactly one entry:

```rust
init!(TypePath { .. })
```

The target must implement `InitDefault<T, Mode>`. `..` cannot be combined with explicit field entries.

`init!` selects `InitMode`. `pin_init!` selects `PinInitMode`. To choose a shared error type, append `? ErrorType` after the closing brace:

```rust
init!(TypePath { field <- initializer } ? AppError)
```

Every field initializer must produce `AppError`, usually by calling `.map_err(...)` on field initializers with a different error type. Without the suffix, the error type is `Infallible`.

### `@target`

In pinned macro forms, `field <- @target` supplies the final parent address to a `*const Target` or `*mut Target` field. It is rejected in `init!` and is not a general expression form. It does not extend the target's lifetime or pin its storage.

### Compile-time checks

The field list is validated against the derive metadata in a const context. The macro rejects duplicate or missing fields and a `#[pin]` field passed through `init!`. Every listed field uses `<-` syntax; pinned fields specifically require the pinned macro mode. It also rejects duplicate entries and trailing unrecognized tokens.

## `pin_init_local!`

This macro declares a local pinned receipt and its hidden local `MaybeUninit<T>` storage:

```rust
pin_init_local! {
    let mut value = TypePath {
        field <- initializer,
    };
}
```

It also accepts the whole-value `TypePath { .. }` form. The optional `? ErrorType` suffix follows the initializer braces and propagates with `?`, so use it inside a function that returns a compatible `Result`. Without an error suffix, the macro handles `Infallible` internally.

## Choosing an entry point

| Need | API |
| --- | --- |
| Movable value in borrowed storage | `init_in(&mut MaybeUninit<T>, init!(...))` |
| Pinned value in local storage | `pin_init_local!` |
| Pinned value in caller-managed storage | `unsafe pin_init_in(Pin<&mut MaybeUninit<T>>, pin_init!(...))` |
| Movable or pinned heap owner | `.init_box()` / `.pin_box()` with `alloc` |
