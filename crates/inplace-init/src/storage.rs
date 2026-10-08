//! 可消费的未初始化存储容器，以及将初始化器接入堆分配的扩展 trait。
//! Consumable uninitialized-storage containers and extension traits for heap allocation.

use core::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use crate::{
    init::{Init, InitDefault, InitDefaultInit, InitMode},
    init_in,
};

#[cfg(feature = "alloc")]
use crate::{init::PinInitMode, pin_init_in};
#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use core::{mem::forget, pin::Pin};

/// 可消费的普通初始化存储。
/// Consumable storage for movable initialization.
///
/// 初始化成功后会交付拥有型 [`InitValue`]；失败时错误返回给调用方，尚未初始化的存储随容器释放。
/// Success yields an owning [`InitValue`]; on failure, the error is returned and the uninitialized storage is released with the container.
pub struct InitStorage<T> {
    storage: MaybeUninit<T>,
}

impl<T> InitStorage<T> {
    /// 创建尚未初始化的拥有型存储。
    /// Create owned storage that does not yet contain a `T`.
    pub const fn uninit() -> Self {
        Self {
            storage: MaybeUninit::uninit(),
        }
    }

    /// 消耗未初始化存储，并尝试返回拥有初始化值的对象。
    /// Consume the uninitialized storage and try to return an object that owns the initialized value.
    ///
    /// 错误不会产生 `InitValue`；若初始化器失败，调用方只会收到其错误值。
    /// No `InitValue` is produced on error; if the initializer fails, the caller receives its error value.
    /// 该错误路径依赖 `Init` 的 unsafe 契约：返回错误或 panic 时，存储必须保持未初始化。
    /// The error path relies on `Init`'s unsafe contract that storage remains uninitialized on error or panic.
    pub fn initialize<I, E>(mut self, initializer: I) -> Result<InitValue<T>, E>
    where
        I: Init<T, E>,
    {
        let receipt = init_in(&mut self.storage, initializer)?;
        Ok(InitValue {
            value: receipt.into_inner(),
        })
    }

    /// 在存储槽位中构造默认值并返回拥有该值的对象。
    /// Construct the default value in the storage slot and return an object that owns it.
    pub fn initialize_default(self) -> InitValue<T>
    where
        T: InitDefault<T, InitMode>,
    {
        self.initialize_default_with::<T>()
    }

    /// 使用指定的默认值提供者在存储槽位中构造目标。
    /// Construct the target in the storage slot using the specified default-value provider.
    ///
    /// `I` 可以与 `T` 不同，只要它实现了针对 `T` 和普通地址模式的 [`InitDefault`]。
    /// `I` may differ from `T` as long as it implements [`InitDefault`] for `T` in movable mode.
    pub fn initialize_default_with<I>(self) -> InitValue<T>
    where
        I: InitDefault<T, InitMode>,
    {
        match self.initialize(InitDefaultInit::<T, I>::new()) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
}

/// 由普通初始化存储返回、拥有完整值的容器。
/// An owning container returned by ordinary initialization storage.
///
/// 它负责析构内部值，并提供借用、可变借用或移出值的入口。
/// It drops the contained value and provides shared borrowing, mutable borrowing, and ownership extraction.
pub struct InitValue<T> {
    value: T,
}

impl<T> InitValue<T> {
    /// 借用内部值。
    /// Borrow the contained value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// 独占借用内部值。
    /// Mutably borrow the contained value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// 移出内部值并转移所有权。
    /// Move the contained value out and transfer ownership to the caller.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for InitValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for InitValue<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(feature = "alloc")]
/// 为普通初始化器提供堆分配入口。
/// Adds a heap-allocation entry point for movable initializers.
pub trait InitBoxExt<T, E>: Init<T, E> {
    /// 在堆存储中构造一个允许移动的值。
    /// Construct a movable value in heap storage and return its owning `Box`.
    fn init_box(self) -> Result<Box<T>, E>
    where
        Self: Sized,
    {
        let mut storage = Box::new(MaybeUninit::<T>::uninit());
        let receipt = init_in(&mut storage, self)?;
        forget(receipt);
        // 安全性：凭证已将析构责任转交给同一分配中的 Box<T>。 Safety: the receipt's drop responsibility is transferred to `Box<T>` for the same allocation.
        Ok(unsafe { storage.assume_init() })
    }
}

#[cfg(feature = "alloc")]
impl<T, E, I> InitBoxExt<T, E> for I where I: Init<T, E> {}

#[cfg(feature = "alloc")]
/// 为固定地址初始化器提供堆分配入口。
/// Adds a heap-allocation entry point for pinned initializers.
pub trait PinInitBoxExt<T, E>: Init<T, E, PinInitMode> {
    /// 在堆分配的最终地址中构造并固定一个值。
    /// Construct and pin a value at its final address in a heap allocation.
    fn pin_box(self) -> Result<Pin<Box<T>>, E>
    where
        Self: Sized,
    {
        let mut storage = Box::new(MaybeUninit::<T>::uninit());
        // 安全性：Box 提供对齐且足够大的稳定存储，分配在转换前不会移动。 Safety: `Box` provides aligned storage of sufficient size, and the allocation does not move during conversion.
        let pinned_storage = unsafe { Pin::new_unchecked(&mut *storage) };
        // 安全性：本方法独占分配，并在返回后把该分配转换为 Pin<Box<T>>。 Safety: this method exclusively owns the allocation and converts it to `Pin<Box<T>>` before returning.
        let receipt = unsafe { pin_init_in(pinned_storage, self)? };
        forget(receipt);
        // 安全性：凭证已将固定值的析构责任转交给同一稳定分配。 Safety: the receipt's drop responsibility for the pinned value is transferred to the same stable allocation.
        let value = unsafe { storage.assume_init() };
        Ok(Box::into_pin(value))
    }
}

#[cfg(feature = "alloc")]
impl<T, E, I> PinInitBoxExt<T, E> for I where I: Init<T, E, PinInitMode> {}
