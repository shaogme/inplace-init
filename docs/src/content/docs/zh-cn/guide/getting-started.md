---
title: 快速入门
description: 将 inplace-init 添加到 Rust 项目，并在目标存储中初始化第一个结构体。
---

## 添加依赖

```toml
[dependencies]
inplace-init = "0.1"
```

该 crate 默认不启用任何 feature，并支持 `no_std`；只有需要分配器相关 API 时才启用 `alloc` 或 `std`。

## 在调用方存储中初始化结构体

为结构体派生 `Init`，使用 `init!` 描述各字段的初始化方式，再将初始化器和 `MaybeUninit<T>` 槽位交给 `init_in`：

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

`<-` 语法用于指定字段及其初始化器。字段可以按任意顺序出现，但每个字段必须且只能出现一次。宏会从目标位置投影出字段槽位；它不会先组装一个临时 `Message`，再把整个结构体移动到存储中。

`init_in` 返回 `InitReceipt`。凭证独占借用存储，可解引用为初始化后的 `Message`，并在自身销毁时析构该值。使用值期间应保留凭证。对于可移动值，可调用 `into_inner()` 将值移出，并结束凭证的析构责任。

## 选择存储所有权方式

如果已经有栈上的 `MaybeUninit<T>` 等目标，并希望凭证借用这块存储，请使用 `init_in`。若需要简单的拥有型包装器，`InitStorage<T>` 会消费未初始化存储，并返回 `InitValue<T>`：

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

`InitValue` 拥有并析构内部值，还提供共享/可变访问和 `into_inner()`。它是普通可移动的拥有者，不是固定地址拥有者。

## 后续阅读

- 阅读[初始化模型](/zh-cn/concepts/initialization-model/)，了解槽位、凭证与失败清理。
- 类型包含默认值或更适合链式 setter 时，使用 [derive builder](/zh-cn/guides/derive-builder/)。
- 构造依赖地址的值之前，请先阅读[固定地址初始化](/zh-cn/guides/pinning/)。
- 在[宏语法](/zh-cn/reference/macros/)中查看元组字段、默认形式和可失败初始化器。
