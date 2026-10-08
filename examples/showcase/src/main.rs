use core::{marker::PhantomPinned, mem::MaybeUninit};
use inplace_init::{
    Init, InitMode, InitReceipt, InitStorage, InitTyU8, InitTyU32, InitValue, PinInitBoxExt, init,
    init_in, pin_init, pin_init_local,
};
use std::{boxed::Box, pin::Pin};

#[derive(Init)]
struct Packet {
    id: u32,
    retries: u8,
}

#[derive(Init)]
struct Request {
    #[init(default = InitTyU32::<30>)]
    timeout_ms: u32,
    attempts: u8,
}

#[derive(Init)]
struct SelfRef {
    #[pin]
    self_ptr: *const SelfRef,
    #[pin]
    _pin: PhantomPinned,
}

fn main() {
    show_slot_initialization();
    show_builder_initialization();
    show_pinned_initialization();
}

fn show_slot_initialization() {
    let mut storage: MaybeUninit<Packet> = MaybeUninit::uninit();
    let receipt: InitReceipt<'_, Packet, InitMode> = init_in(
        &mut storage,
        init!(Packet {
            id <- InitTyU32::<42>,
            retries <- InitTyU8::<3>,
        }),
    )
    .expect("常量初始化器不可失败");

    println!("槽位初始化：id={}, retries={}", receipt.id, receipt.retries);
}

fn show_builder_initialization() {
    let storage: InitStorage<Request> = InitStorage::<Request>::uninit();
    let request: InitValue<Request> = storage
        .initialize(Request::init().attempts(InitTyU8::<2>).build())
        .expect("常量初始化器不可失败");

    println!(
        "Builder 初始化：timeout_ms={}, attempts={}",
        request.timeout_ms, request.attempts
    );
}

fn show_pinned_initialization() {
    pin_init_local! {
        let mut local = SelfRef {
            self_ptr <- @target,
            _pin <- default,
        };
    }
    let local_target: Pin<&SelfRef> = local.as_pin_ref();
    let local_target: &SelfRef = local_target.get_ref();
    assert_eq!(local_target.self_ptr, local_target as *const SelfRef);

    let heap: Pin<Box<SelfRef>> = pin_init!(SelfRef {
        self_ptr <- @target,
        _pin <- default,
    })
    .pin_box()
    .expect("默认固定地址初始化不可失败");
    let heap_target: &SelfRef = heap.as_ref().get_ref();
    assert_eq!(heap_target.self_ptr, heap_target as *const SelfRef);

    println!("固定地址初始化：局部值和 Pin<Box<_>> 中的自引用均指向自身");
}
