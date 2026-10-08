---
title: Default values
description: Understand InitDefault, field defaults, and the built-in default providers.
---

## In-place defaults are explicit

Rust's `Default` trait returns a value by value. `InitDefault<T, Mode>` instead constructs `T` in the supplied slot and returns a receipt. A type implementing `Default` does not automatically implement `InitDefault`; this separation allows address-aware defaults to initialize at the final location.

Use `#[init(default)]` on a derived builder field when that field's type has an `InitDefault<Field, Mode>` implementation:

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

The first field uses the type's in-place default provider. The second stores an explicit, infallible initializer. Both fields begin set in the type-state builder, and either can be overridden with a setter.

At the macro level, `init!(Settings { .. })` requests a default initializer for the whole value. The `..` must be the only entry in the braces. To mix explicit field values and defaults, list every field and write `field <- default` for a field that should use `InitDefault`.

## Built-in providers

The runtime crate includes in-place providers for:

| Feature | Types |
| --- | --- |
| No features | `()`, `Option<T>` (`None`), `PhantomData<T>`, `PhantomPinned`, arrays `[T; N]` when `T` has an in-place default, pairs `(A, B)` when both elements do, and scalar bool/char/integer types |
| `alloc` | `String`, `Vec`, `VecDeque`, `BinaryHeap`, `LinkedList`, `BTreeMap`, `BTreeSet`, and `Rc<T>` / `Arc<T>` when `T` has an in-place default |
| `std` | `HashMap` and `HashSet` |

The default feature set is empty. `std` enables `alloc`. The collection defaults are empty collections. Array elements are constructed one at a time; if an element default panics, already initialized elements are dropped.

For integers and booleans, the built-in default is zero or `false`. The `InitTy...` const-generic types can initialize a chosen scalar value, for example `InitTyU32::<42>`.

## Use `Default::default()` when appropriate

`InitDefaultValue<T>` adapts `T: Default` to `InitDefault<T, InitMode>` by creating the ordinary value and writing it into the slot. It is available when you want the standard `Default` behavior for a type that has no specialized in-place provider. It is movable-mode only.

```rust
use inplace_init::{InitDefaultInit, InitDefaultValue, InitStorage};

fn main() {
    let text = InitStorage::<String>::uninit()
        .initialize_default_with::<InitDefaultValue<String>>();
    assert!(text.is_empty());
}
```

This example needs the `alloc` feature because `String` does. For a derive field, wrap the provider in the general initializer adapter:

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

## Write a custom in-place default

Implement `InitDefault<T, Mode>` when a value must be created at the destination address. `InitDefault` is an unsafe trait: the implementation must leave one complete `T` and return its sole receipt on success; on panic it must clean up partial state and leave the target uninitialized. A pinned default must also construct address-dependent fields at the final address.

See [errors and safety](/guides/errors-and-safety/) for the contract and a pinned field-by-field pattern. Prefer existing providers or `InitDefaultValue<T>` when their semantics fit.
