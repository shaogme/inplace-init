//! 初始化器适配器、闭包封装和编译期常量初始化器。
//! Initializer adapters, closure wrappers, and compile-time constant initializers.

use core::{convert::Infallible, marker::PhantomData};

use crate::init::{Init, InitDefault, InitMode, InitReceipt, InitSlot, PinInitMode};

/// 错误映射后的初始化器。
/// An initializer wrapper that converts the underlying initializer's error.
///
/// 只有初始化失败时才调用映射闭包；成功时保留原始析构凭证。
/// The mapping closure runs only on failure; on success, the original destruction receipt is preserved.
pub struct MapInitError<I, F, E, Mode = InitMode> {
    initializer: I,
    map: F,
    _error: PhantomData<fn(E)>,
    _mode: PhantomData<fn() -> Mode>,
}

impl<I, F, E, Mode> MapInitError<I, F, E, Mode> {
    /// 创建保留原错误类型和地址模式的映射适配器。
    /// Create a mapping adapter that retains the original error type and address mode.
    pub(crate) fn new(initializer: I, map: F) -> Self {
        Self {
            initializer,
            map,
            _error: PhantomData,
            _mode: PhantomData,
        }
    }
}

// 安全性依据：底层 Init 的 unsafe 契约保证错误时槽位未初始化；映射闭包只转换错误，成功时保留原凭证。
// Safety: the underlying Init contract guarantees an uninitialized slot on error; the mapper only converts errors and preserves the original receipt on success.
unsafe impl<T, E1, E2, I, F, Mode> Init<T, E2, Mode> for MapInitError<I, F, E1, Mode>
where
    I: Init<T, E1, Mode>,
    F: FnOnce(E1) -> E2,
{
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, T, Mode>,
    ) -> Result<InitReceipt<'slot, T, Mode>, E2> {
        let MapInitError {
            initializer, map, ..
        } = self;
        match initializer.initialize(slot) {
            Ok(receipt) => {
                drop(map);
                Ok(receipt)
            }
            Err(error) => Err(map(error)),
        }
    }
}

struct InitFnMarker<T, E, Mode> {
    _value: PhantomData<fn() -> T>,
    _error: PhantomData<fn() -> E>,
    _mode: PhantomData<fn() -> Mode>,
}

/// 将按值构造闭包适配为普通可移动模式的初始化器。
/// Adapts a by-value construction closure into a movable-mode initializer.
///
/// 闭包只需返回一个值或错误；值会在目标槽位中写入，错误或 panic 不会留下已初始化状态。固定地址初始化请使用 [`InitFnRaw`] 或其他支持固定模式的初始化器。
/// The closure only returns a value or error; the value is written into the target slot, and an error or panic cannot leave initialized state behind. Use [`InitFnRaw`] or another pinned-mode initializer for pinned initialization.
pub struct InitFn<T, E, F> {
    initialize: F,
    _marker: InitFnMarker<T, E, InitMode>,
}

impl<T, E, F> InitFn<T, E, F> {
    /// 封装一个无参数、返回值或错误的闭包。
    /// Wrap a no-argument closure that returns a value or an error.
    pub fn new(initialize: F) -> Self
    where
        F: FnOnce() -> Result<T, E>,
    {
        Self {
            initialize,
            _marker: InitFnMarker {
                _value: PhantomData,
                _error: PhantomData,
                _mode: PhantomData,
            },
        }
    }
}

// 安全性依据：闭包只按值返回 T；错误或 panic 时尚未写入槽位，成功时 slot.write 创建唯一凭证。
// Safety: the closure only returns T by value; errors and panics occur before the slot is written, and slot.write creates the sole receipt on success.
unsafe impl<T, E, F> Init<T, E, InitMode> for InitFn<T, E, F>
where
    F: FnOnce() -> Result<T, E>,
{
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, T, InitMode>,
    ) -> Result<InitReceipt<'slot, T, InitMode>, E> {
        let value = (self.initialize)()?;
        Ok(slot.write(value))
    }
}

/// 由闭包直接在目标槽位中构造值的原始初始化器包装类型。
/// Raw initializer wrapper for closures that construct a value directly in the target slot.
///
/// 闭包必须返回与槽位生命周期一致的凭证，因此不能把槽位引用或凭证延长到目标借用之外。
/// The closure must return a receipt tied to the slot lifetime, so neither the slot nor its receipt can outlive the target borrow.
pub struct InitFnRaw<T, E, F, Mode = InitMode> {
    initialize: F,
    _marker: InitFnMarker<T, E, Mode>,
}

impl<T, E, F, Mode> InitFnRaw<T, E, F, Mode> {
    /// 封装一个接收目标槽位并返回初始化结果的闭包。
    /// Wrap a closure that accepts a target slot and returns the initialization result.
    ///
    /// # 安全性要求
    /// 闭包必须满足 [`Init`] 的完整安全契约：成功时只留下一个完整目标并返回唯一凭证；
    /// 返回错误或 panic 时清理所有已构造状态；同时遵守地址模式约束。
    ///
    /// # Safety
    /// The closure must uphold the full [`Init`] safety contract: leave exactly one complete
    /// target and return its sole receipt on success; clean up every initialized part on error or
    /// panic; and uphold the address-mode requirements.
    pub unsafe fn new(initialize: F) -> Self
    where
        F: for<'slot> FnOnce(InitSlot<'slot, T, Mode>) -> Result<InitReceipt<'slot, T, Mode>, E>,
    {
        Self {
            initialize,
            _marker: InitFnMarker {
                _value: PhantomData,
                _error: PhantomData,
                _mode: PhantomData,
            },
        }
    }
}

// 安全性依据：InitFnRaw 只能通过 unsafe new 创建，调用者承诺闭包满足 Init 的完整契约。
// Safety: InitFnRaw can only be constructed through unsafe new, whose caller promises the closure upholds the full Init contract.
unsafe impl<T, E, F, Mode> Init<T, E, Mode> for InitFnRaw<T, E, F, Mode>
where
    F: for<'slot> FnOnce(InitSlot<'slot, T, Mode>) -> Result<InitReceipt<'slot, T, Mode>, E>,
{
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, T, Mode>,
    ) -> Result<InitReceipt<'slot, T, Mode>, E> {
        (self.initialize)(slot)
    }
}

macro_rules! const_init {
    ($name:ident, $value_type:ty, $default:expr) => {
        #[doc = concat!("使用编译期 `", stringify!($value_type), "` 常量构造值的初始化器。 / An initializer that constructs a value from a compile-time `", stringify!($value_type), "` constant.")]
        pub struct $name<const VALUE: $value_type>;

        unsafe impl<Mode> InitDefault<$value_type, Mode> for $value_type {
            fn initialize_default<'slot>(
                slot: InitSlot<'slot, $value_type, Mode>,
            ) -> InitReceipt<'slot, $value_type, Mode> {
                slot.write($default)
            }
        }

        unsafe impl<const VALUE: $value_type, Mode> InitDefault<$value_type, Mode>
            for $name<VALUE>
        {
            fn initialize_default<'slot>(
                slot: InitSlot<'slot, $value_type, Mode>,
            ) -> InitReceipt<'slot, $value_type, Mode> {
                slot.write(VALUE)
            }
        }

        // 安全性依据：slot.write 原子地写入一个完整标量并返回其唯一凭证。
        // Safety: slot.write constructs one complete scalar and returns its sole receipt.
        unsafe impl<const VALUE: $value_type> Init<$value_type> for $name<VALUE> {
            fn initialize<'slot>(
                self,
                slot: InitSlot<'slot, $value_type>,
            ) -> Result<InitReceipt<'slot, $value_type>, Infallible> {
                Ok(slot.write(VALUE))
            }
        }

        // 安全性依据：标量不依赖固定地址，slot.write 完整构造值并返回其唯一凭证。
        // Safety: scalars do not depend on a stable address, and slot.write constructs the complete value and returns its sole receipt.
        unsafe impl<const VALUE: $value_type> Init<$value_type, Infallible, PinInitMode> for $name<VALUE> {
            fn initialize<'slot>(
                self,
                slot: InitSlot<'slot, $value_type, PinInitMode>,
            ) -> Result<InitReceipt<'slot, $value_type, PinInitMode>, Infallible> {
                Ok(slot.write(VALUE))
            }
        }
    };
}

const_init!(InitTyBool, bool, false);
const_init!(InitTyChar, char, '\0');
const_init!(InitTyI8, i8, 0);
const_init!(InitTyI16, i16, 0);
const_init!(InitTyI32, i32, 0);
const_init!(InitTyI64, i64, 0);
const_init!(InitTyI128, i128, 0);
const_init!(InitTyIsize, isize, 0);
const_init!(InitTyU8, u8, 0);
const_init!(InitTyU16, u16, 0);
const_init!(InitTyU32, u32, 0);
const_init!(InitTyU64, u64, 0);
const_init!(InitTyU128, u128, 0);
const_init!(InitTyUsize, usize, 0);
