# Inplace-Init

**Inplace-Init** constructs Rust values directly in caller-provided storage. It supports ordinary initialization, initialization at a stable pinned address, and field-by-field construction with rollback when an initializer fails or panics.

[Documentation](https://shaogme.github.io/inplace-init/) · [简体中文 README](README_CN.md)

## What it provides

- `InitSlot` gives an initializer one exclusive, uninitialized destination.
- `InitReceipt` tracks the initialized value and owns the responsibility to drop it. Dropping the receipt drops the value; movable receipts can also return the value with `into_inner`.
- `Init` and `InitDefault` describe custom initialization and in-place default construction.
- `#[derive(Init)]` generates a typestate builder and the field metadata used by the initializer macros.
- `init!`, `pin_init!`, and `pin_init_local!` provide field-oriented initialization syntax.
- The runtime crate is `no_std` by default. `alloc` and `std` enable optional collection and heap-allocation support.

The workspace uses Rust 2024 edition and is released under either the Apache-2.0 or MIT license.

## Installation

Add the runtime crate to your `Cargo.toml`:

```toml
[dependencies]
inplace-init = "0.1"
```

The default feature set is empty. Enable `alloc` for heap helpers and `alloc`-backed defaults, or enable `std` for those features plus standard-library hash collections:

```toml
[dependencies]
inplace-init = { version = "0.1", features = ["alloc"] }
```

## Quick start: initialize fields in a supplied slot

Derive `Init` on a struct to expose its fields to the field initializer macros. The `<-` operator takes an initializer for that field. The following example constructs `Packet` directly in `MaybeUninit` storage:

```rust
use core::mem::MaybeUninit;
use inplace_init::{init, init_in, Init, InitMode, InitReceipt, InitTyU32, InitTyU8};

#[derive(Init)]
struct Packet {
    id: u32,
    retries: u8,
}

let mut storage: MaybeUninit<Packet> = MaybeUninit::uninit();
let receipt: InitReceipt<'_, Packet, InitMode> = init_in(
    &mut storage,
    init!(Packet {
        id <- InitTyU32::<42>,
        retries <- InitTyU8::<3>,
    }),
)
.unwrap();

assert_eq!(receipt.id, 42);
assert_eq!(receipt.retries, 3);
```

`init_in` returns `Result<InitReceipt<...>, E>`. The receipt borrows the destination exclusively and drops the value when it is dropped. In movable mode, call `receipt.into_inner()` to move the value out, or use `InitStorage` when an owning container is more convenient.

## Typestate builder

The derive generates a `Type::pin_init()` builder entry point, and also generates `Type::init()` when the struct has no `#[pin]` fields. Each setter consumes the builder and marks one field as initialized in the type system. A field without a default must be set before `build()` is available; a field with a declared default may be overridden. Named-field setters use the field names; tuple-struct setters are named `field_0`, `field_1`, and so on.

```rust
use inplace_init::{Init, InitStorage, InitTyU32, InitTyU8, InitValue};

#[derive(Init)]
struct Request {
    #[init(default = InitTyU32::<30>)]
    timeout_ms: u32,
    attempts: u8,
}

let storage: InitStorage<Request> = InitStorage::<Request>::uninit();
let request: InitValue<Request> = storage
    .initialize(Request::init().attempts(InitTyU8::<2>).build())
    .unwrap();

assert_eq!(request.timeout_ms, 30);
assert_eq!(request.attempts, 2);
```

`#[init(default)]` uses the field type's `InitDefault` implementation. `#[init(default = initializer)]` stores the supplied initializer as the initial builder state; that expression must be an infallible initializer for the field's address mode. Use the setter for a fallible initializer. Both forms can be replaced by the field setter. For a pinned builder, `Type::pin_init()` selects pinned initialization for `#[pin]` fields and movable initialization for other fields.

`build()` completes the type-state description and returns an initializer; it does not construct the value by itself. Pass that initializer to `InitStorage::initialize`, `init_in`, or a heap extension method.

Builder fields may use different error types. The generated initializer preserves them in a generated, documentation-hidden error enum with one `FieldN` variant per field. The macro form's optional `? ErrorType` instead selects one shared error type for its field initializers.

## Pinned and self-referential values

Mark fields whose invariants require a stable address with `#[pin]`. `pin_init_local!` creates local pinned storage and a receipt whose destructor runs before the backing storage leaves scope. The `@target` expression writes the final address into a raw-pointer field:

```rust
use core::{marker::PhantomPinned, pin::Pin};
use inplace_init::{Init, pin_init_local};

#[derive(Init)]
struct SelfRef {
    #[pin]
    self_ptr: *const SelfRef,
    #[pin]
    _pin: PhantomPinned,
}

pin_init_local! {
    let mut value = SelfRef {
        self_ptr <- @target,
        _pin <- default,
    };
}

let target: Pin<&SelfRef> = value.as_pin_ref();
let target: &SelfRef = target.get_ref();
assert_eq!(target.self_ptr, target as *const SelfRef);
```

`@target` is available only in `pin_init!` and `pin_init_local!`, and only for raw `*const Target` or `*mut Target` fields. The target struct must derive `Init`. Pinned fields must be initialized with `<-` syntax through a pinned entry point.

With the `alloc` feature, `PinInitBoxExt::pin_box()` constructs a pinned initializer in its final heap allocation and returns `Pin<Box<T>>`:

```rust
#[cfg(feature = "alloc")]
fn pinned_box_example() {
    use core::{marker::PhantomPinned, pin::Pin};
    use inplace_init::{Init, PinInitBoxExt, pin_init};
    use std::boxed::Box;

    #[derive(Init)]
    struct SelfRef {
        #[pin]
        self_ptr: *const SelfRef,
        #[pin]
        _pin: PhantomPinned,
    }

    let value: Pin<Box<SelfRef>> = pin_init!(SelfRef {
        self_ptr <- @target,
        _pin <- default,
    })
    .pin_box()
    .unwrap();
    let target: &SelfRef = value.as_ref().get_ref();
    assert_eq!(target.self_ptr, target as *const SelfRef);
}

#[cfg(feature = "alloc")]
pinned_box_example();
```

The code block defines its own `SelfRef` type and runs when the `alloc` feature is enabled. For lower-level control, `pin_init_in` accepts `Pin<&mut MaybeUninit<T>>` and is unsafe: the caller must keep that storage pinned until the receipt is dropped. If the receipt is forgotten, the storage must not be moved, reused, or exposed for another initialization.

## Initializer macro syntax

| Form | Purpose |
| --- | --- |
| `init!(Type { field <- initializer, ... })` | Movable field-by-field initializer. |
| `init!(Type { field <- initializer, ... } ? ErrorType)` | Same, with the chosen error type for the initializer. |
| `init!(Type { .. })` | Calls `InitDefault` for the whole target in its destination slot. |
| `init!(Type { field <- default, ... })` | Calls `InitDefault` for an individual field in its destination slot. |
| `pin_init!(Type { ... })` | Produces a pinned field-by-field initializer. |
| `pin_init_local! { let mut name = Type { ... }; }` | Creates local pinned storage and binds its receipt. |

Field selectors are identifiers for named fields and numeric indices for tuple-struct fields. Explicit field initialization must name each field exactly once; duplicate, missing, or unknown fields fail at compile time. The whole-value `..` form cannot be combined with per-field entries. Field-oriented initialization needs `#[derive(Init)]`; whole-value default initialization only needs an appropriate `InitDefault` implementation.

Without `? ErrorType`, initializer expressions use `core::convert::Infallible`. With an explicit error type, each field initializer must use that error type; map different errors with `Init::map_err` or the initializer's `.map_err(...)` method. The same error suffix is supported by `pin_init!`. In `pin_init_local!`, the `?` form propagates the selected error from the enclosing function.

`init!` produces a movable initializer and rejects `@target`. `pin_init!` produces an initializer for use with `pin_init_in` or `PinInitBoxExt::pin_box`. `pin_init_local!` handles the local storage and pinning entry point for you.

## Defaults

The built-in `InitDefault` implementations are designed to construct values in their destination slots:

- In every address mode: `()`, `Option<T>` (as `None`), `PhantomData<T>`, and `PhantomPinned`.
- Arrays `[T; N]` and two-element tuples `(A, B)`, when their elements implement `InitDefault` for the same mode. Array initialization cleans up completed elements if a later element panics.
- `bool`, `char`, and the signed and unsigned integer types, with `false`, `'\0'`, or zero as their defaults. `InitTyBool`, `InitTyChar`, `InitTyI8` through `InitTyI128` / `InitTyIsize`, and `InitTyU8` through `InitTyU128` / `InitTyUsize` are const-generic initializers for explicit scalar values.
- With `alloc`: `String`, `Vec`, `VecDeque`, `BinaryHeap`, `LinkedList`, `BTreeMap`, `BTreeSet`, `Rc<T>`, and `Arc<T>`.
- With `std`: `HashMap` and `HashSet`.

`InitDefaultValue<T>` adapts `T: Default` by calling `Default::default()` and writing the resulting value. It is available for movable initialization. Use a direct `InitDefault` implementation when construction must happen at the destination address, including for pinned values.

## Storage and initializer APIs

| API | Description |
| --- | --- |
| `Init<T, E, Mode>` | Unsafe trait for an initializer. Implementations must return one complete value and clean up all partial state on errors or panic. |
| `InitDefault<T, Mode>` | Unsafe trait for constructing a default value directly in a slot. |
| `InitSlot<'slot, T, Mode>` | One-use access to uninitialized storage; `write` constructs a complete value and returns its receipt. |
| `InitReceipt<'slot, T, Mode>` | Owns the initialized value's drop responsibility. Movable mode supports `DerefMut` and `into_inner`; pinned mode exposes `as_pin_ref` and `as_pin_mut`. |
| `init_in` | Safe entry point for movable initialization in `&mut MaybeUninit<T>`. |
| `pin_init_in` | Unsafe entry point for pinned initialization in `Pin<&mut MaybeUninit<T>>`. |
| `InitStorage<T>` / `InitValue<T>` | Owned movable storage and its initialized value wrapper. `InitValue` dereferences to `T` and can return it with `into_inner`. |
| `InitFn<T, E, F>` | Safe adapter for a no-argument closure `FnOnce() -> Result<T, E>` in movable mode; a successful value is written into the target slot. |
| `InitFnRaw<T, E, F, Mode>` | Unsafe closure adapter for advanced custom initialization directly in a slot. |
| `InitRaw` / `RawInit` | Adapter for initializers that need direct raw-pointer access. Implementing `InitRaw` is unsafe. |

An initializer must retain receipts for completed fields until every later field succeeds. This is what lets the generated macro and builder clean up earlier fields if a later initializer returns an error or panics. Implement `Init` or `InitDefault` only when you can uphold their full unsafe contracts.

## Feature flags

| Feature | Effect |
| --- | --- |
| *(default)* | `no_std` runtime with no allocation-backed defaults. |
| `alloc` | Enables `alloc` collection defaults, `Rc` / `Arc` defaults, `InitBoxExt::init_box()`, and `PinInitBoxExt::pin_box()`. |
| `std` | Enables `alloc` and the `HashMap` / `HashSet` default implementations. |

## Constraints

- `#[derive(Init)]` supports structs, including named-field, tuple, and unit structs; it does not support enums or unions.
- `#[derive(Init)]` rejects `#[repr(packed)]`, because field projection requires correctly aligned fields.
- Field macro invocations must initialize every target field exactly once. Use `..` for whole-value defaults or `default` for individual field defaults.
- A pinned field requires `pin_init!`, `pin_init_local!`, or the generated pinned builder and an initializer compatible with the pinned address mode.
- `pin_init_in` is unsafe because storage stability is a caller responsibility. The convenience `pin_init_local!` and `pin_box()` APIs manage their backing storage.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option.
