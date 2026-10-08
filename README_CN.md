# Inplace-Init

**Inplace-Init** 可在调用方提供的存储中直接构造 Rust 值，支持普通初始化、在稳定固定地址上的初始化，以及逐字段构造并在初始化器失败或 panic 时回滚。

[在线文档](https://shaogme.github.io/inplace-init/zh-cn/) · [English documentation](https://shaogme.github.io/inplace-init/) · [English README](README.md)

## 功能概览

- `InitSlot` 为初始化器提供对一个独占、尚未初始化目标槽位的一次性访问。
- `InitReceipt` 跟踪已初始化的值并独占其析构责任。凭证析构时会析构该值；普通模式下也可以通过 `into_inner` 移出值。
- `Init` 与 `InitDefault` 分别描述自定义初始化和原位默认构造。
- `#[derive(Init)]` 会生成类型状态 builder，以及字段初始化宏所需的字段元数据。
- `init!`、`pin_init!` 和 `pin_init_local!` 提供字段初始化语法。
- 运行时 crate 默认是 `no_std`。`alloc` 和 `std` 可启用可选的集合与堆分配支持。

工作区使用 Rust 2024 edition，采用 Apache-2.0 或 MIT 双许可证。

## 安装

在 `Cargo.toml` 中添加运行时 crate：

```toml
[dependencies]
inplace-init = "0.1"
```

默认不启用任何 feature。启用 `alloc` 可使用堆分配辅助 API 和依赖 `alloc` 的默认实现；启用 `std` 则会同时启用这些能力以及标准库哈希集合的默认实现：

```toml
[dependencies]
inplace-init = { version = "0.1", features = ["alloc"] }
```

## 快速开始：在指定槽位中初始化字段

在结构体上派生 `Init`，即可让字段初始化宏访问其字段。`<-` 后面是该字段的初始化器。下面的例子直接在 `MaybeUninit` 存储中构造 `Packet`：

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

`init_in` 返回 `Result<InitReceipt<...>, E>`。凭证会独占借用目标存储，并在析构时析构值。普通模式下可以调用 `receipt.into_inner()` 移出值；也可以使用 `InitStorage` 管理拥有型存储。

## 类型状态 builder

derive 会生成 `Type::pin_init()` builder 入口；只有结构体没有 `#[pin]` 字段时才会同时生成 `Type::init()`。每个 setter 都会消耗当前 builder，并在类型系统中标记一个字段已完成初始化。没有默认值的字段必须先设置，`build()` 才可用；声明了默认值的字段可以通过 setter 覆盖。具名字段的 setter 与字段同名；tuple struct 的 setter 名为 `field_0`、`field_1` 等。

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

`#[init(default)]` 使用字段类型对应的 `InitDefault` 实现。`#[init(default = initializer)]` 将给定初始化器设为 builder 的初始字段状态；表达式必须是适用于该字段地址模式的不可失败初始化器。可失败的初始化器应通过 setter 设置。两种默认都可以通过字段 setter 替换。对于固定地址 builder，`Type::pin_init()` 会为 `#[pin]` 字段选择固定模式，为其他字段选择普通模式。

`build()` 只完成类型状态描述并返回一个初始化器，本身不会构造目标值。将其传给 `InitStorage::initialize`、`init_in` 或堆分配扩展方法即可执行初始化。

Builder 的各字段可以使用不同错误类型。生成的初始化器会通过一个隐藏文档的错误枚举保留这些错误，每个字段对应一个 `FieldN` 变体。宏形式的可选 `? ErrorType` 则为各字段初始化器指定同一个错误类型。

## 固定地址与自引用值

需要稳定地址的字段应标记 `#[pin]`。`pin_init_local!` 会创建局部固定存储，并返回一个凭证；该凭证先于底层存储析构。`@target` 会将目标最终地址写入裸指针字段：

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

`@target` 仅能用于 `pin_init!` 和 `pin_init_local!`，字段类型必须是 `*const Target` 或 `*mut Target`。目标结构体需要派生 `Init`。带 `#[pin]` 的字段必须通过固定地址入口使用 `<-` 语法初始化。

启用 `alloc` 后，`PinInitBoxExt::pin_box()` 会在最终堆地址上构造固定地址初始化器，并返回 `Pin<Box<T>>`：

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

代码块内定义了独立的 `SelfRef` 类型，并会在启用 `alloc` feature 时运行。若需要更底层的控制，`pin_init_in` 接受 `Pin<&mut MaybeUninit<T>>`，但它是 unsafe API：调用方必须确保凭证析构前存储始终固定。如果凭证被遗忘，存储不得移动、复用，也不得再次暴露为初始化槽位。

## 初始化宏语法

| 形式 | 用途 |
| --- | --- |
| `init!(Type { field <- initializer, ... })` | 普通地址模式的逐字段初始化器。 |
| `init!(Type { field <- initializer, ... } ? ErrorType)` | 同上，并指定初始化器的错误类型。 |
| `init!(Type { .. })` | 在目标槽位中对整个目标调用 `InitDefault`。 |
| `init!(Type { field <- default, ... })` | 在字段自己的目标槽位中调用 `InitDefault`。 |
| `pin_init!(Type { ... })` | 生成固定地址的逐字段初始化器。 |
| `pin_init_local! { let mut name = Type { ... }; }` | 创建局部固定存储并绑定其凭证。 |

具名字段使用标识符选择，tuple struct 字段使用数字索引。显式逐字段初始化必须恰好列出每个字段一次；重复、遗漏或未知字段都会在编译期报错。整个对象的 `..` 不能与逐字段条目混用。逐字段初始化需要 `#[derive(Init)]`；整个对象的默认初始化只需要合适的 `InitDefault` 实现。

省略 `? ErrorType` 时，初始化器错误类型为 `core::convert::Infallible`。指定错误类型后，各字段初始化器必须使用相同错误类型；若错误类型不同，可通过 `Init::map_err` 或初始化器的 `.map_err(...)` 统一。`pin_init!` 也支持相同的错误类型后缀。在 `pin_init_local!` 中，带 `?` 的形式会把所选错误传播到外层函数。

`init!` 生成普通模式初始化器，不接受 `@target`。`pin_init!` 生成的初始化器可交给 `pin_init_in` 或 `PinInitBoxExt::pin_box`。`pin_init_local!` 会代为管理局部存储和固定初始化入口。

## 默认初始化

内置 `InitDefault` 实现会尝试在目标槽位中直接构造值：

- 所有地址模式均支持：`()`, `Option<T>`（默认值为 `None`）、`PhantomData<T>` 和 `PhantomPinned`。
- 当元素在相同地址模式下实现 `InitDefault` 时，支持数组 `[T; N]` 和二元 tuple `(A, B)`。若数组后续元素初始化时 panic，已完成的元素会被清理。
- 支持 `bool`、`char` 以及有符号和无符号整数类型，默认值分别为 `false`、`'\0'` 或零。`InitTyBool`、`InitTyChar`、`InitTyI8` 至 `InitTyI128` / `InitTyIsize`，以及 `InitTyU8` 至 `InitTyU128` / `InitTyUsize` 是对应类型的 const 泛型显式值初始化器。
- 启用 `alloc`：支持 `String`、`Vec`、`VecDeque`、`BinaryHeap`、`LinkedList`、`BTreeMap`、`BTreeSet`、`Rc<T>` 和 `Arc<T>`。
- 启用 `std`：支持 `HashMap` 和 `HashSet`。

`InitDefaultValue<T>` 会调用 `Default::default()`，再将得到的值写入目标；它要求 `T: Default`，并提供普通地址模式的适配。如果值必须在目标地址构造（包括固定地址值），应使用直接的 `InitDefault` 实现。

## 存储与初始化器 API

| API | 说明 |
| --- | --- |
| `Init<T, E, Mode>` | unsafe 初始化器 trait。实现必须产生一个完整值，并在错误或 panic 时清理所有部分状态。 |
| `InitDefault<T, Mode>` | unsafe trait，用于在槽位中直接构造默认值。 |
| `InitSlot<'slot, T, Mode>` | 对未初始化存储的一次性访问；`write` 构造完整值并返回其凭证。 |
| `InitReceipt<'slot, T, Mode>` | 独占已初始化值的析构责任。普通模式支持 `DerefMut` 和 `into_inner`；固定模式提供 `as_pin_ref` 和 `as_pin_mut`。 |
| `init_in` | 普通初始化的安全入口，接受 `&mut MaybeUninit<T>`。 |
| `pin_init_in` | 固定地址初始化的 unsafe 入口，接受 `Pin<&mut MaybeUninit<T>>`。 |
| `InitStorage<T>` / `InitValue<T>` | 拥有型普通存储及其初始化后返回的值容器。`InitValue` 可解引用为 `T`，也可通过 `into_inner` 取出值。 |
| `InitFn<T, E, F>` | 在普通可移动模式下安全地适配无参数闭包 `FnOnce() -> Result<T, E>`；成功返回的值会写入目标槽位。 |
| `InitFnRaw<T, E, F, Mode>` | 将闭包包装为直接写入槽位的高级初始化器；创建时需要 unsafe。 |
| `InitRaw` / `RawInit` | 为需要直接操作裸指针的初始化器提供适配。实现 `InitRaw` 是 unsafe 操作。 |

初始化多个字段时，必须保留已完成字段的凭证，直到后续字段也成功。这样生成的宏和 builder 才能在后续初始化返回错误或 panic 时清理已完成的字段。只有能够履行完整 unsafe 契约时，才应实现 `Init` 或 `InitDefault`。

## 功能开关

| Feature | 效果 |
| --- | --- |
| *(默认)* | `no_std` 运行时，不启用依赖动态分配的默认实现。 |
| `alloc` | 启用 `alloc` 集合默认实现、`Rc` / `Arc` 默认实现、`InitBoxExt::init_box()` 和 `PinInitBoxExt::pin_box()`。 |
| `std` | 启用 `alloc`，并提供 `HashMap` / `HashSet` 默认实现。 |

## 限制

- `#[derive(Init)]` 支持具名字段结构体、tuple struct 和 unit struct；不支持 enum 或 union。
- `#[derive(Init)]` 拒绝 `#[repr(packed)]`，因为字段投影要求字段地址满足对齐要求。
- 字段初始化宏必须恰好初始化目标的每个字段。整个目标使用 `..`，单个字段使用 `default`。
- 固定字段必须通过 `pin_init!`、`pin_init_local!` 或生成的固定模式 builder 初始化，并使用符合固定地址模式的初始化器。
- `pin_init_in` 是 unsafe API，因为存储是否稳定由调用方负责。`pin_init_local!` 和 `pin_box()` 会管理其底层存储。

## 许可证

本项目可由你选择使用 [Apache License 2.0](LICENSE-APACHE) 或 [MIT License](LICENSE-MIT)。
