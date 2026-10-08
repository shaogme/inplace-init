#![no_std]
#![doc = concat!(
    include_str!("../../../README.md"),
    "\n\n",
    include_str!("../../../README_CN.md"),
)]

use core::{mem::MaybeUninit, pin::Pin};

#[doc(hidden)]
pub extern crate self as inplace_init;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub use inplace_init_macros::{Init, init, pin_init, pin_init_local};

mod init;
mod storage;

pub use init::{
    Init, InitDefault, InitDefaultInit, InitDefaultValue, InitField, InitFieldInit, InitFields,
    InitFn, InitFnRaw, InitMode, InitRaw, InitReceipt, InitSlot, InitState, InitTargetPointer,
    InitTyBool, InitTyChar, InitTyI8, InitTyI16, InitTyI32, InitTyI64, InitTyI128, InitTyIsize,
    InitTyU8, InitTyU16, InitTyU32, InitTyU64, InitTyU128, InitTyUsize, MapInitError, MovableField,
    PinInitMode, PinnedField, RawInit, Set, Unset,
};
pub use storage::{InitStorage, InitValue};

#[cfg(feature = "alloc")]
pub use storage::{InitBoxExt, PinInitBoxExt};

/// 在 `MaybeUninit` 存储中初始化可移动值，并返回负责析构该值的凭证。
/// Initialize a movable value in `MaybeUninit` storage and return the receipt that drops it.
///
/// 调用方独占借用槽位直到凭证被销毁或值被移出；初始化器返回的错误会原样传给调用方。
/// The slot remains exclusively borrowed until the receipt is dropped or the value is moved out; initializer errors are returned unchanged.
/// 安全性由 `Init` 的 unsafe 实现契约保证：错误或 panic 时槽位保持未初始化。
/// The `Init` implementation's unsafe contract guarantees that the slot remains uninitialized on error or panic.
pub fn init_in<'slot, T, I, E>(
    slot: &'slot mut MaybeUninit<T>,
    initializer: I,
) -> Result<InitReceipt<'slot, T, InitMode>, E>
where
    I: Init<T, E>,
{
    // 安全性：独占借用保证目标地址有效、对齐且当前未初始化。 Safety: the exclusive borrow guarantees that the target address is valid, aligned, and currently uninitialized.
    let slot = unsafe { InitSlot::__from_raw(slot.as_mut_ptr()) };
    initializer.initialize(slot)
}

/// 在固定地址的 `MaybeUninit` 存储中初始化值，并返回固定析构凭证。
/// Initialize a value in pinned `MaybeUninit` storage and return a pinned destruction receipt.
///
/// 该入口不会移动存储，但调用方必须保证传入存储及其地址满足下方的固定约束。
/// This function does not move the storage, but the caller must uphold the pinning requirements below.
///
/// # Safety
/// 调用方必须保证存储在凭证销毁前始终固定。如果凭证被遗忘，存储也不得被移动、
/// 重用或以其他方式暴露为可再次初始化的槽位。
/// The storage must remain pinned until the receipt is dropped. If the receipt is forgotten, the storage must not be moved, reused, or exposed as an initialization slot again.
pub unsafe fn pin_init_in<'slot, T, I, E>(
    slot: Pin<&'slot mut MaybeUninit<T>>,
    initializer: I,
) -> Result<InitReceipt<'slot, T, PinInitMode>, E>
where
    I: Init<T, E, PinInitMode>,
{
    // 安全性：调用方承诺凭证遗忘后仍不移动或复用存储。 Safety: the caller promises not to move or reuse the storage even if the receipt is forgotten.
    let storage = unsafe { Pin::get_unchecked_mut(slot) };
    // 安全性：storage 来自固定独占借用，地址在初始化过程中保持稳定。 Safety: storage comes from a pinned exclusive borrow, so its address stays stable during initialization.
    let slot = unsafe { InitSlot::__from_raw(storage.as_mut_ptr()) };
    initializer.initialize(slot)
}

/// 在编译期核对初始化宏调用与目标结构体的字段清单。
/// Validate at compile time that an initialization macro call matches its target struct's field inventory.
///
/// `provided` 中的布尔值表示字段是否在目标地址原位初始化；固定字段必须同时使用固定入口和原位语法。
/// Each boolean in `provided` records whether that field is initialized in place; pinned fields require both a pinned entry point and in-place syntax.
#[doc(hidden)]
pub const fn __validate_init_fields<T: InitFields>(provided: &[(&str, bool)], pinned: bool) {
    let expected = T::FIELDS;
    assert!(
        expected.len() == T::FIELD_COUNT,
        "`InitFields` field metadata has an incorrect length"
    );
    assert!(
        expected.len() == provided.len(),
        "The number of initialized fields does not match the target struct"
    );

    let mut expected_index = 0;
    while expected_index < expected.len() {
        let (expected_name, is_pinned) = expected[expected_index];
        let mut prior_index = 0;
        while prior_index < expected_index {
            let (prior_name, _) = expected[prior_index];
            assert!(
                !__str_eq(expected_name, prior_name),
                "`InitFields` field names must be unique"
            );
            prior_index += 1;
        }

        let mut matching_index = 0;
        let mut match_count = 0;
        let mut initialized_in_place = false;
        while matching_index < provided.len() {
            let (provided_name, provided_in_place) = provided[matching_index];
            if __str_eq(expected_name, provided_name) {
                match_count += 1;
                initialized_in_place = provided_in_place;
            }
            matching_index += 1;
        }

        assert!(
            match_count == 1,
            "Initialized fields do not match the target struct's fields"
        );
        assert!(
            !is_pinned || (pinned && initialized_in_place),
            "`#[pin]` fields must be initialized with the `<-` syntax in `pin_init!`"
        );
        expected_index += 1;
    }
}

/// 在常量求值期间逐字节比较字段名。
/// Compare field names byte by byte during constant evaluation.
const fn __str_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }

    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
