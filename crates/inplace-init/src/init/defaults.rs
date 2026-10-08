//! 默认初始化能力及其针对常见类型的实现。
//! Default-initialization capabilities and their implementations for common types.

use core::{
    convert::Infallible,
    marker::{PhantomData, PhantomPinned},
    mem::forget,
    ptr,
};

use crate::init::{Init, InitMode, InitReceipt, InitSlot};

#[cfg(feature = "alloc")]
use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use alloc::{
    collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque},
    rc::Rc,
    string::String,
    sync::Arc,
    vec::Vec,
};

#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

/// 在目标槽位中直接构造默认值。
/// Constructs a default value directly in the target slot.
///
/// # Safety
/// 正常返回时，目标槽位必须包含恰好一个有效的 `T`，返回凭证独占该值的析构责任。
/// 发生 panic 时，实现必须销毁所有已构造的部分状态，并使目标槽位保持未初始化状态。
/// 实现不得在创建凭证前让部分初始化状态逃逸。
/// 同时必须满足 `Mode` 对目标地址的要求。
/// On return, the slot must contain exactly one valid `T`, and the returned receipt must exclusively own its destruction responsibility.
/// If a panic occurs, the implementation must drop every initialized part and leave the slot uninitialized.
/// Partially initialized state must not escape before a receipt is created, and the implementation must uphold `Mode`'s address requirements.
pub unsafe trait InitDefault<T = Self, Mode = InitMode>: Sized {
    /// 在目标槽位中初始化默认值并返回其析构凭证。
    /// Initialize the default value in the target slot and return its destruction receipt.
    fn initialize_default<'slot>(slot: InitSlot<'slot, T, Mode>) -> InitReceipt<'slot, T, Mode>;
}

/// 将默认值提供者适配为普通初始化器。
/// Adapts a default-value provider to the general initializer interface.
#[doc(hidden)]
pub struct InitDefaultInit<T, Provider = T> {
    _types: PhantomData<fn() -> (T, Provider)>,
}

impl<T, Provider> InitDefaultInit<T, Provider> {
    /// 创建默认值初始化器。
    /// Create a default-value initializer.
    pub const fn new() -> Self {
        Self {
            _types: PhantomData,
        }
    }
}

impl<T, Provider> Default for InitDefaultInit<T, Provider> {
    fn default() -> Self {
        Self::new()
    }
}

// 安全性依据：InitDefault 的 unsafe 契约保证完整初始化且返回唯一凭证；本适配器直接转交该凭证。
// Safety: InitDefault's unsafe contract guarantees complete initialization and the sole receipt; this adapter forwards that receipt directly.
unsafe impl<T, Provider, Mode> Init<T, Infallible, Mode> for InitDefaultInit<T, Provider>
where
    Provider: InitDefault<T, Mode>,
{
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, T, Mode>,
    ) -> Result<InitReceipt<'slot, T, Mode>, Infallible> {
        Ok(Provider::initialize_default(slot))
    }
}

/// 使用 `Default::default()` 按值构造的默认初始化器。
/// A default initializer that constructs a value by calling `Default::default()` by value.
pub struct InitDefaultValue<T> {
    _type: PhantomData<fn() -> T>,
}

impl<T> InitDefaultValue<T> {
    /// 创建按值默认初始化器。
    /// Create a by-value default initializer.
    pub const fn new() -> Self {
        Self { _type: PhantomData }
    }
}

impl<T> Default for InitDefaultValue<T> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<T> InitDefault<T, InitMode> for InitDefaultValue<T>
where
    T: Default,
{
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, T, InitMode>,
    ) -> InitReceipt<'slot, T, InitMode> {
        slot.write(T::default())
    }
}

unsafe impl<Mode> InitDefault<(), Mode> for () {
    fn initialize_default<'slot>(slot: InitSlot<'slot, (), Mode>) -> InitReceipt<'slot, (), Mode> {
        slot.write(())
    }
}

unsafe impl<T, Mode> InitDefault<Option<T>, Mode> for Option<T> {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, Option<T>, Mode>,
    ) -> InitReceipt<'slot, Option<T>, Mode> {
        slot.write(None)
    }
}

unsafe impl<T, Mode> InitDefault<PhantomData<T>, Mode> for PhantomData<T> {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, PhantomData<T>, Mode>,
    ) -> InitReceipt<'slot, PhantomData<T>, Mode> {
        slot.write(PhantomData)
    }
}

unsafe impl<Mode> InitDefault<PhantomPinned, Mode> for PhantomPinned {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, PhantomPinned, Mode>,
    ) -> InitReceipt<'slot, PhantomPinned, Mode> {
        slot.write(PhantomPinned)
    }
}

struct ArrayInitGuard<T> {
    pointer: *mut T,
    initialized: usize,
}

impl<T> Drop for ArrayInitGuard<T> {
    fn drop(&mut self) {
        while self.initialized > 0 {
            self.initialized -= 1;
            // 安全性：计数只在对应元素完整初始化后增加，数组元素互不重叠。 Safety: the count advances only after an element is fully initialized, and array elements do not overlap.
            unsafe { ptr::drop_in_place(self.pointer.add(self.initialized)) };
        }
    }
}

unsafe impl<T, Mode, const N: usize> InitDefault<[T; N], Mode> for [T; N]
where
    T: InitDefault<T, Mode>,
{
    fn initialize_default<'slot>(
        mut slot: InitSlot<'slot, [T; N], Mode>,
    ) -> InitReceipt<'slot, [T; N], Mode> {
        let pointer = slot.__target_ptr().cast::<T>();
        let mut guard = ArrayInitGuard {
            pointer,
            initialized: 0,
        };

        for index in 0..N {
            // 安全性：索引小于数组长度，元素地址位于目标数组的独占存储中。 Safety: the index is within the array length, and the element address lies in the exclusively borrowed array storage.
            let element_pointer = unsafe { pointer.add(index) };
            // 安全性：数组元素是目标中的唯一、对齐且尚未初始化的字段槽位。 Safety: this array element is the unique, aligned, uninitialized field slot being initialized.
            let element_slot = unsafe { slot.__project(element_pointer) };
            let receipt = T::initialize_default(element_slot);
            forget(receipt);
            guard.initialized += 1;
        }

        guard.initialized = 0;
        // 安全性：所有数组元素均已初始化，且元素凭证的析构责任已转交给数组。 Safety: every array element is initialized, and the array now owns the elements' destruction responsibility.
        unsafe { slot.__assume_init() }
    }
}

unsafe impl<A, B, Mode> InitDefault<(A, B), Mode> for (A, B)
where
    A: InitDefault<A, Mode>,
    B: InitDefault<B, Mode>,
{
    fn initialize_default<'slot>(
        mut slot: InitSlot<'slot, (A, B), Mode>,
    ) -> InitReceipt<'slot, (A, B), Mode> {
        let pointer = slot.__target_ptr();
        // 安全性：tuple 字段通过原始地址投影，不会读取未初始化的 tuple。 Safety: the tuple field is projected through a raw address without reading the uninitialized tuple.
        let first_pointer = unsafe { ptr::addr_of_mut!((*pointer).0) };
        // 安全性：第一个 tuple 字段是目标中唯一且尚未初始化的字段。 Safety: the first tuple field is the unique field currently being initialized.
        let first_slot = unsafe { slot.__project(first_pointer) };
        let first = A::initialize_default(first_slot);

        // 安全性：tuple 字段通过原始地址投影，不会读取未初始化的 tuple。 Safety: the tuple field is projected through a raw address without reading the uninitialized tuple.
        let second_pointer = unsafe { ptr::addr_of_mut!((*pointer).1) };
        // 安全性：第二个 tuple 字段与第一个字段不重叠且尚未初始化。 Safety: the second tuple field is uninitialized and does not overlap the first field.
        let second_slot = unsafe { slot.__project(second_pointer) };
        let second = B::initialize_default(second_slot);

        forget(first);
        forget(second);
        // 安全性：两个字段均已初始化，字段凭证的析构责任已转交给 tuple。 Safety: both fields are initialized, and the tuple now owns their destruction responsibility.
        unsafe { slot.__assume_init() }
    }
}

#[cfg(feature = "alloc")]
macro_rules! empty_collection_default {
    ($($generic:ident),*; $type:ty, $value:expr) => {
        unsafe impl<$($generic,)* Mode> InitDefault<$type, Mode> for $type {
            fn initialize_default<'slot>(
                slot: InitSlot<'slot, $type, Mode>,
            ) -> InitReceipt<'slot, $type, Mode> {
                slot.write($value)
            }
        }
    };
}

#[cfg(feature = "alloc")]
empty_collection_default!(; String, String::new());

#[cfg(feature = "alloc")]
empty_collection_default!(T; Vec<T>, Vec::new());

#[cfg(feature = "alloc")]
empty_collection_default!(T; VecDeque<T>, VecDeque::new());

#[cfg(feature = "alloc")]
empty_collection_default!(T; BinaryHeap<T>, BinaryHeap::new());

#[cfg(feature = "alloc")]
empty_collection_default!(T; LinkedList<T>, LinkedList::new());

#[cfg(feature = "alloc")]
empty_collection_default!(K, V; BTreeMap<K, V>, BTreeMap::new());

#[cfg(feature = "alloc")]
empty_collection_default!(T; BTreeSet<T>, BTreeSet::new());

#[cfg(feature = "alloc")]
unsafe impl<T, Mode> InitDefault<Rc<T>, Mode> for Rc<T>
where
    T: InitDefault<T, Mode>,
{
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, Rc<T>, Mode>,
    ) -> InitReceipt<'slot, Rc<T>, Mode> {
        let mut value = Rc::<T>::new_uninit();
        // 安全性：新分配的 Rc 尚未共享，唯一所有者可以独占访问未初始化存储。 Safety: the new Rc is not shared, so its sole owner can access the uninitialized allocation exclusively.
        let pointer = match Rc::get_mut(&mut value) {
            Some(storage) => storage.as_mut_ptr(),
            None => unreachable!("A newly allocated `Rc` must have only one owner"),
        };
        // 安全性：指针来自唯一拥有的 Rc 分配，且其 pointee 尚未初始化。 Safety: the pointer comes from the uniquely owned Rc allocation, whose pointee is uninitialized.
        let pointee_slot = unsafe { InitSlot::__from_raw(pointer) };
        let receipt = T::initialize_default(pointee_slot);
        forget(receipt);
        // 安全性：pointee 已完整初始化，且凭证的析构责任已转交给 Rc。 Safety: the pointee is fully initialized, and its destruction responsibility has been transferred to the Rc.
        let value = unsafe { Rc::<MaybeUninit<T>>::assume_init(value) };
        slot.write(value)
    }
}

#[cfg(feature = "alloc")]
unsafe impl<T, Mode> InitDefault<Arc<T>, Mode> for Arc<T>
where
    T: InitDefault<T, Mode>,
{
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, Arc<T>, Mode>,
    ) -> InitReceipt<'slot, Arc<T>, Mode> {
        let mut value = Arc::<T>::new_uninit();
        // 安全性：新分配的 Arc 尚未共享，唯一所有者可以独占访问未初始化存储。 Safety: the new Arc is not shared, so its sole owner can access the uninitialized allocation exclusively.
        let pointer = match Arc::get_mut(&mut value) {
            Some(storage) => storage.as_mut_ptr(),
            None => unreachable!("A newly allocated `Arc` must have only one owner"),
        };
        // 安全性：指针来自唯一拥有的 Arc 分配，且其 pointee 尚未初始化。 Safety: the pointer comes from the uniquely owned Arc allocation, whose pointee is uninitialized.
        let pointee_slot = unsafe { InitSlot::__from_raw(pointer) };
        let receipt = T::initialize_default(pointee_slot);
        forget(receipt);
        // 安全性：pointee 已完整初始化，且凭证的析构责任已转交给 Arc。 Safety: the pointee is fully initialized, and its destruction responsibility has been transferred to the Arc.
        let value = unsafe { Arc::<MaybeUninit<T>>::assume_init(value) };
        slot.write(value)
    }
}

#[cfg(feature = "std")]
unsafe impl<K, V, Mode> InitDefault<HashMap<K, V>, Mode> for HashMap<K, V> {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, HashMap<K, V>, Mode>,
    ) -> InitReceipt<'slot, HashMap<K, V>, Mode> {
        slot.write(HashMap::new())
    }
}

#[cfg(feature = "std")]
unsafe impl<T, Mode> InitDefault<HashSet<T>, Mode> for HashSet<T> {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, HashSet<T>, Mode>,
    ) -> InitReceipt<'slot, HashSet<T>, Mode> {
        slot.write(HashSet::new())
    }
}
