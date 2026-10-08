use inplace_init::{
    Init, InitDefault, InitDefaultInit, InitDefaultValue, InitFn, InitFnRaw, InitMode, InitReceipt,
    InitSlot, InitStorage, InitTyBool, InitTyChar, InitTyI8, InitTyI16, InitTyI32, InitTyI64,
    InitTyI128, InitTyIsize, InitTyU8, InitTyU16, InitTyU32, InitTyU64, InitTyU128, InitTyUsize,
    PinInitMode, init, init_in, pin_init_local,
};

use core::{
    marker::{PhantomData, PhantomPinned},
    mem::{MaybeUninit, forget},
    ptr,
};

#[cfg(feature = "alloc")]
use inplace_init::{InitBoxExt, InitRaw, PinInitBoxExt, RawInit, pin_init};

use std::{
    convert::Infallible,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[cfg(feature = "alloc")]
use std::{rc::Rc, sync::Arc};

#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

#[cfg(feature = "alloc")]
use std::{
    collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque},
    string::String,
};

static DROPS: AtomicUsize = AtomicUsize::new(0);
static DROPS_LOCK: Mutex<()> = Mutex::new(());
static ORDER_EVENTS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

struct Tracked(usize);

impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

struct OrderedTracked(u8);

impl Drop for OrderedTracked {
    fn drop(&mut self) {
        ORDER_EVENTS.lock().unwrap().push(self.0 + 100);
    }
}

struct OrderedInit(u8);

// 安全性依据：初始化器一次性写入完整值并返回其唯一凭证。
// Safety: the initializer writes one complete value and returns its sole receipt.
unsafe impl Init<OrderedTracked> for OrderedInit {
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, OrderedTracked>,
    ) -> Result<InitReceipt<'slot, OrderedTracked>, Infallible> {
        ORDER_EVENTS.lock().unwrap().push(self.0);
        Ok(slot.write(OrderedTracked(self.0)))
    }
}

#[derive(Init)]
struct OrderedPair {
    first: OrderedTracked,
    second: OrderedTracked,
}

#[derive(Init)]
struct OrderedTriple {
    first: OrderedTracked,
    second: OrderedTracked,
    third: OrderedTracked,
}

#[derive(Debug, PartialEq, Eq)]
enum OrderFailure {
    Probe,
}

struct OrderedFail;

// 安全性依据：失败发生在任何写入之前，槽位保持未初始化。
// Safety: failure occurs before any write, so the slot remains uninitialized.
unsafe impl Init<OrderedTracked, OrderFailure> for OrderedFail {
    fn initialize<'slot>(
        self,
        _slot: InitSlot<'slot, OrderedTracked>,
    ) -> Result<InitReceipt<'slot, OrderedTracked>, OrderFailure> {
        Err(OrderFailure::Probe)
    }
}

struct MarkerDefault(u32);

unsafe impl<Mode> InitDefault<MarkerDefault, Mode> for MarkerDefault {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, MarkerDefault, Mode>,
    ) -> InitReceipt<'slot, MarkerDefault, Mode> {
        slot.write(MarkerDefault(67))
    }
}

struct MarkerDefaultInit(u32);

// 安全性依据：初始化器一次性写入完整值并返回其唯一凭证。
// Safety: the initializer writes one complete value and returns its sole receipt.
unsafe impl Init<MarkerDefault> for MarkerDefaultInit {
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, MarkerDefault>,
    ) -> Result<InitReceipt<'slot, MarkerDefault>, Infallible> {
        Ok(slot.write(MarkerDefault(self.0)))
    }
}

#[derive(Init)]
struct DerivedDefaultBuilder {
    #[init(default)]
    value: MarkerDefault,
}

#[derive(Init)]
struct GenericDerivedDefaultBuilder<T> {
    #[init(default)]
    value: T,
}

#[derive(Init)]
struct PinnedDerivedDefaultBuilder {
    #[pin]
    #[init(default)]
    value: MarkerDefault,
}

struct TrackedInit(usize);

// 安全性依据：初始化器一次性写入完整值并返回其唯一凭证。
// Safety: the initializer writes one complete value and returns its sole receipt.
unsafe impl Init<Tracked> for TrackedInit {
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, Tracked>,
    ) -> Result<InitReceipt<'slot, Tracked>, Infallible> {
        Ok(slot.write(Tracked(self.0)))
    }
}

#[derive(Init)]
struct Container {
    first: Tracked,
    second: Tracked,
}

#[derive(Init)]
struct DefaultPair {
    first: Tracked,
    second: Tracked,
}

#[derive(Init)]
struct DefaultBuilder {
    #[init(default = InitTyU32::<42>)]
    count: u32,
    retries: u8,
}

#[derive(Init)]
struct PinnedDefaultBuilder {
    #[pin]
    #[init(default = InitTyU32::<17>)]
    count: u32,
}

#[cfg(feature = "alloc")]
struct PinOnlyDefault(u32);

#[cfg(feature = "alloc")]
unsafe impl InitDefault<PinOnlyDefault, PinInitMode> for PinOnlyDefault {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, PinOnlyDefault, PinInitMode>,
    ) -> InitReceipt<'slot, PinOnlyDefault, PinInitMode> {
        slot.write(Self(73))
    }
}

#[cfg(feature = "alloc")]
struct MovableOnlyDefault(u32);

#[cfg(feature = "alloc")]
unsafe impl InitDefault<MovableOnlyDefault, InitMode> for MovableOnlyDefault {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, MovableOnlyDefault, InitMode>,
    ) -> InitReceipt<'slot, MovableOnlyDefault, InitMode> {
        slot.write(Self(29))
    }
}

#[cfg(feature = "alloc")]
#[derive(Init)]
struct ModeAwareDefaultBuilder {
    #[pin]
    #[init(default)]
    pinned: PinOnlyDefault,
    #[init(default)]
    movable: MovableOnlyDefault,
}

unsafe impl<Mode> InitDefault<Tracked, Mode> for Tracked {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, Tracked, Mode>,
    ) -> InitReceipt<'slot, Tracked, Mode> {
        slot.write(Tracked(29))
    }
}

unsafe impl<Mode> InitDefault<DefaultPair, Mode> for DefaultPair {
    fn initialize_default<'slot>(
        mut slot: InitSlot<'slot, DefaultPair, Mode>,
    ) -> InitReceipt<'slot, DefaultPair, Mode> {
        let target = slot.__target_ptr();
        // 安全性：字段地址来自尚未初始化的目标，仅形成原始指针。
        let first_pointer = unsafe { ptr::addr_of_mut!((*target).first) };
        // 安全性：投影字段属于目标且此前尚未初始化。
        let first_slot = unsafe { slot.__project(first_pointer) };
        let first = Tracked::initialize_default(first_slot);

        // 安全性：字段地址来自尚未初始化的目标，仅形成原始指针。
        let second_pointer = unsafe { ptr::addr_of_mut!((*target).second) };
        // 安全性：第二个字段与第一个字段不重叠且此前尚未初始化。
        let second_slot = unsafe { slot.__project(second_pointer) };
        let second = Tracked::initialize_default(second_slot);

        forget(first);
        forget(second);
        // 安全性：两个字段均已完整初始化，析构责任转交给父对象凭证。
        unsafe { slot.__assume_init() }
    }
}

struct PanickingDefault;

unsafe impl<Mode> InitDefault<PanickingDefault, Mode> for PanickingDefault {
    fn initialize_default<'slot>(
        _slot: InitSlot<'slot, PanickingDefault, Mode>,
    ) -> InitReceipt<'slot, PanickingDefault, Mode> {
        panic!("默认初始化失败");
    }
}

#[derive(Init)]
struct StandardCoreDefaultBuilder {
    #[init(default)]
    optional: Option<u32>,
    #[init(default)]
    values: [u8; 3],
    #[init(default)]
    pair: (bool, char),
}

#[cfg(feature = "alloc")]
#[derive(Init)]
struct AllocDefaultBuilder {
    #[init(default)]
    text: String,
    #[init(default)]
    vector: Vec<u8>,
    #[init(default)]
    deque: VecDeque<u8>,
    #[init(default)]
    heap: BinaryHeap<u8>,
    #[init(default)]
    linked_list: LinkedList<u8>,
    #[init(default)]
    map: BTreeMap<u8, u8>,
    #[init(default)]
    set: BTreeSet<u8>,
}

static ARRAY_DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
static ARRAY_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static ARRAY_DEFAULT_LOCK: Mutex<()> = Mutex::new(());

struct ArrayDefaultElement;

impl Drop for ArrayDefaultElement {
    fn drop(&mut self) {
        ARRAY_DEFAULT_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

unsafe impl<Mode> InitDefault<ArrayDefaultElement, Mode> for ArrayDefaultElement {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, ArrayDefaultElement, Mode>,
    ) -> InitReceipt<'slot, ArrayDefaultElement, Mode> {
        if ARRAY_DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst) == 2 {
            panic!("数组元素默认初始化失败");
        }
        slot.write(Self)
    }
}

#[test]
fn init_fn_raw_initializes_directly_in_the_provided_slot() {
    let mut storage = MaybeUninit::<usize>::uninit();
    let target = storage.as_mut_ptr();
    // 安全性依据：闭包写入一个完整 usize 后立即返回对应凭证，且不执行可失败或 panic 的操作。
    // Safety: the closure writes one complete usize and immediately returns its receipt without any fallible or panicking work afterward.
    let initializer = unsafe {
        InitFnRaw::<usize, Infallible, _>::new(|slot| {
            let target_address = slot.__target_ptr() as usize;
            Ok(slot.write(target_address))
        })
    };

    let value = init_in(&mut storage, initializer).unwrap();

    assert_eq!(*value, target as usize);
}

#[test]
fn init_fn_initializes_a_value_returned_by_a_safe_closure() {
    let mut storage = MaybeUninit::<usize>::uninit();
    let initializer = InitFn::<usize, Infallible, _>::new(|| Ok(42));

    let value = init_in(&mut storage, initializer).unwrap();

    assert_eq!(*value, 42);
}

#[test]
fn init_fn_returns_closure_errors_without_initializing_the_slot() {
    let mut storage = MaybeUninit::<usize>::uninit();
    let initializer = InitFn::<usize, &'static str, _>::new(|| Err("rejected"));

    let result = init_in(&mut storage, initializer);

    assert!(matches!(result, Err("rejected")));
}

#[test]
fn owned_storage_initializes_default_in_its_slot() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    {
        let storage = InitStorage::<DefaultPair>::uninit();
        let value = storage.initialize_default();
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 29);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
}

#[test]
fn owned_storage_stays_uninitialized_when_default_panics() {
    let storage = InitStorage::<PanickingDefault>::uninit();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = storage.initialize_default();
    }));
    assert!(result.is_err());
}

#[test]
fn tuple_default_drops_prior_fields_when_initialization_panics() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    let storage = InitStorage::<(Tracked, PanickingDefault)>::uninit();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = storage.initialize_default();
    }));

    assert!(result.is_err());
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[test]
fn chained_builder_uses_and_allows_overriding_field_defaults() {
    let initializer = DefaultBuilder::init().retries(InitTyU8::<3>).build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.count, 42);
    assert_eq!(value.retries, 3);

    let initializer = DefaultBuilder::init()
        .count(InitTyU32::<9>)
        .retries(InitTyU8::<5>)
        .build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.count, 9);
    assert_eq!(value.retries, 5);
}

#[test]
fn pinned_builder_uses_field_default_initializer() {
    pin_init_local! {
        let mut value = PinnedDefaultBuilder {
            count <- InitTyU32::<17>,
        };
    }
    assert_eq!(value.count, 17);
}

#[cfg(feature = "alloc")]
#[test]
fn pinned_builder_uses_each_fields_default_address_mode() {
    let value = ModeAwareDefaultBuilder::pin_init()
        .build()
        .pin_box()
        .unwrap();
    assert_eq!(value.pinned.0, 73);
    assert_eq!(value.movable.0, 29);
}

#[test]
fn derive_default_attribute_uses_init_default_and_can_be_overridden() {
    let initializer = DerivedDefaultBuilder::init().build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.value.0, 67);

    let initializer = DerivedDefaultBuilder::init()
        .value(MarkerDefaultInit(83))
        .build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.value.0, 83);

    let initializer = GenericDerivedDefaultBuilder::<MarkerDefault>::init().build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.value.0, 67);
}

#[test]
fn pinned_derive_default_attribute_uses_init_default() {
    pin_init_local! {
        let mut value = PinnedDerivedDefaultBuilder {
            value <- default,
        };
    }
    assert_eq!(value.value.0, 67);
}

#[test]
fn init_macro_supports_whole_and_field_default_values() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);

    let mut storage = MaybeUninit::uninit();
    {
        let value = init_in(&mut storage, init!(DefaultPair { .. })).unwrap();
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 29);
    }
    let mut storage = MaybeUninit::uninit();
    {
        let value = init_in(
            &mut storage,
            init!(DefaultPair {
                second <- TrackedInit(41).map_err(|never| -> OrderFailure { match never {} }),
                first <- default,
            } ? OrderFailure),
        )
        .unwrap();
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 41);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 4);

    let mut storage = MaybeUninit::uninit();
    let value = init_in(&mut storage, init!(DefaultPair { .. } ? OrderFailure)).unwrap();
    assert_eq!(value.first.0, 29);
    assert_eq!(value.second.0, 29);
}

#[test]
fn pinned_local_macro_supports_default_values() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    {
        pin_init_local! {
            let mut value = DefaultPair { .. };
        }
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 29);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);

    DROPS.store(0, Ordering::SeqCst);
    {
        pin_init_local! {
            let mut value = DefaultPair { .. };
        }
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 29);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);

    DROPS.store(0, Ordering::SeqCst);
    {
        pin_init_local! {
            let mut value = DefaultPair {
                second <- TrackedInit(43),
                first <- default,
            };
        }
        assert_eq!(value.first.0, 29);
        assert_eq!(value.second.0, 43);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
}

#[test]
fn forgotten_pin_receipt_cannot_reuse_hidden_storage() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);

    pin_init_local! {
        let mut value = DefaultPair { .. };
    }
    forget(value);

    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
}

#[test]
fn field_receipts_preserve_initialization_and_rollback_order() {
    ORDER_EVENTS.lock().unwrap().clear();

    let mut storage = MaybeUninit::uninit();
    {
        let value = init_in(
            &mut storage,
            init!(OrderedPair {
                second <- OrderedInit(2),
                first <- OrderedInit(1),
            }),
        )
        .unwrap();
        assert_eq!(*ORDER_EVENTS.lock().unwrap(), [2, 1]);
        drop(value);
    }
    assert_eq!(*ORDER_EVENTS.lock().unwrap(), [2, 1, 101, 102]);

    ORDER_EVENTS.lock().unwrap().clear();
    let mut storage = MaybeUninit::uninit();
    let initializer = init!(OrderedTriple {
        second <- OrderedInit(2).map_err(|never| -> OrderFailure { match never {} }),
        first <- OrderedInit(1).map_err(|never| -> OrderFailure { match never {} }),
        third <- OrderedFail,
    } ? OrderFailure);
    let result = init_in(&mut storage, initializer);

    assert!(matches!(result, Err(OrderFailure::Probe)));
    assert_eq!(*ORDER_EVENTS.lock().unwrap(), [2, 1, 101, 102]);
}

fn assert_scalar_default<T>(expected: T)
where
    T: InitDefault<T, InitMode> + PartialEq,
{
    let storage = InitStorage::<T>::uninit();
    assert!(*storage.initialize_default() == expected);
}

fn assert_default_provider<I, T>(expected: T)
where
    I: InitDefault<T, InitMode>,
    T: PartialEq,
{
    let storage = InitStorage::<T>::uninit();
    assert!(*storage.initialize_default_with::<I>() == expected);
}

#[test]
fn core_standard_types_support_default_initialization() {
    assert_scalar_default::<()>(());
    assert_scalar_default::<Option<u32>>(None);
    assert_scalar_default::<[u8; 3]>([0; 3]);
    assert_scalar_default::<(bool, char)>((false, '\0'));
    assert_scalar_default::<PhantomData<u8>>(PhantomData);
    assert_scalar_default::<PhantomPinned>(PhantomPinned);

    let initializer = StandardCoreDefaultBuilder::init().build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.optional, None);
    assert_eq!(value.values, [0; 3]);
    assert_eq!(value.pair, (false, '\0'));

    let storage = InitStorage::<Option<u32>>::uninit();
    let value = storage.initialize(init!(Option<u32> { .. })).unwrap();
    assert_eq!(*value, None);
}

#[test]
fn default_trait_values_use_the_safe_by_value_adapter() {
    let storage = InitStorage::<u32>::uninit();
    assert_eq!(
        *storage.initialize_default_with::<InitDefaultValue<u32>>(),
        0
    );
    let mut slot = MaybeUninit::uninit();
    let value = init_in(
        &mut slot,
        InitDefaultInit::<u32, InitDefaultValue<u32>>::new(),
    )
    .unwrap();
    assert_eq!(*value, 0);

    pin_init_local! {
        let mut pinned_default = u32 { .. };
    }
    assert_eq!(*pinned_default, 0);
}

#[test]
fn array_default_drops_elements_when_initialization_panics() {
    let _lock = ARRAY_DEFAULT_LOCK.lock().unwrap();
    ARRAY_DEFAULT_CALLS.store(0, Ordering::SeqCst);
    ARRAY_DEFAULT_DROPS.store(0, Ordering::SeqCst);
    let storage = InitStorage::<[ArrayDefaultElement; 3]>::uninit();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = storage.initialize_default();
    }));

    assert!(result.is_err());
    assert_eq!(ARRAY_DEFAULT_DROPS.load(Ordering::SeqCst), 2);
}

struct AddressObservedDefault {
    initialized_at: usize,
}

unsafe impl InitDefault<AddressObservedDefault, InitMode> for AddressObservedDefault {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, AddressObservedDefault, InitMode>,
    ) -> InitReceipt<'slot, AddressObservedDefault, InitMode> {
        let initialized_at = slot.__target_ptr() as usize;
        slot.write(Self { initialized_at })
    }
}

struct PinnedAddressDefault {
    initialized_at: *const PinnedAddressDefault,
    _pinned: PhantomPinned,
}

unsafe impl InitDefault<PinnedAddressDefault, PinInitMode> for PinnedAddressDefault {
    fn initialize_default<'slot>(
        slot: InitSlot<'slot, PinnedAddressDefault, PinInitMode>,
    ) -> InitReceipt<'slot, PinnedAddressDefault, PinInitMode> {
        let target = slot.__target_ptr();
        // 安全性：目标槽位有效且未初始化；逐字段写入可避免移动已建立自引用的值。
        unsafe {
            ptr::addr_of_mut!((*target).initialized_at).write(target);
            ptr::addr_of_mut!((*target)._pinned).write(PhantomPinned);
        }
        // 安全性：两个字段均已写入，且自引用指向值的固定目标地址。
        unsafe { slot.__assume_init() }
    }
}

#[test]
fn default_initializers_receive_the_final_target_address() {
    let mut storage = MaybeUninit::uninit();
    let target = storage.as_mut_ptr() as usize;
    let value = init_in(&mut storage, init!(AddressObservedDefault { .. })).unwrap();
    assert_eq!(value.initialized_at, target);

    pin_init_local! {
        let mut pinned_value = PinnedAddressDefault { .. };
    }
    assert_eq!(pinned_value.initialized_at, ptr::addr_of!(*pinned_value));
}

#[cfg(feature = "alloc")]
#[test]
fn alloc_standard_types_support_default_initialization() {
    let initializer = AllocDefaultBuilder::init().build();
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert!(value.text.is_empty());
    assert!(value.vector.is_empty());
    assert!(value.deque.is_empty());
    assert!(value.heap.is_empty());
    assert!(value.linked_list.is_empty());
    assert!(value.map.is_empty());
    assert!(value.set.is_empty());

    let storage = InitStorage::<Vec<u8>>::uninit();
    let value = storage.initialize(init!(Vec<u8> { .. })).unwrap();
    assert!(value.is_empty());
}

#[cfg(feature = "alloc")]
#[test]
fn rc_and_arc_default_initialize_their_pointees_in_place() {
    let rc_storage = InitStorage::<Rc<AddressObservedDefault>>::uninit();
    let rc_value = rc_storage.initialize_default();
    assert_eq!(rc_value.initialized_at, Rc::as_ptr(rc_value.get()) as usize);

    let arc_storage = InitStorage::<Arc<AddressObservedDefault>>::uninit();
    let arc_value = arc_storage.initialize_default();
    assert_eq!(
        arc_value.initialized_at,
        Arc::as_ptr(arc_value.get()) as usize
    );
}

#[cfg(feature = "std")]
#[test]
fn std_hash_collections_support_default_initialization() {
    let map_storage = InitStorage::<HashMap<u8, u8>>::uninit();
    assert!(map_storage.initialize_default().is_empty());

    let set_storage = InitStorage::<HashSet<u8>>::uninit();
    assert!(set_storage.initialize_default().is_empty());
}

#[test]
fn macro_initializes_fields_in_final_storage() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    let initializer = init!(Container {
        first <- TrackedInit(3),
        second <- TrackedInit(5),
    });
    let mut storage = MaybeUninit::uninit();
    {
        let value = init_in(&mut storage, initializer).unwrap();
        assert_eq!(value.first.0 + value.second.0, 8);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
}

fn container_initializer() -> impl Init<Container> {
    init!(Container {
        first <- TrackedInit(19),
        second <- TrackedInit(23),
    })
}

#[test]
fn init_macro_result_can_be_returned_from_a_function() {
    let initializer = container_initializer();
    let mut storage = MaybeUninit::uninit();
    let value = init_in(&mut storage, initializer).unwrap();

    assert_eq!(value.first.0, 19);
    assert_eq!(value.second.0, 23);
}

#[test]
fn owned_storage_drops_the_initialized_value() {
    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    {
        let value = InitStorage::uninit()
            .initialize(init!(Container {
                first <- TrackedInit(11),
                second <- TrackedInit(17),
            }))
            .unwrap();
        assert_eq!(value.first.0 + value.second.0, 28);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
}

#[test]
fn failed_field_initialization_rolls_back_prior_fields() {
    #[derive(Debug, PartialEq, Eq)]
    enum Failure {
        Probe,
    }

    struct FailingInit;

    // 安全性依据：失败发生在任何写入之前，槽位保持未初始化。
    // Safety: failure occurs before any write, so the slot remains uninitialized.
    unsafe impl Init<Tracked, Failure> for FailingInit {
        fn initialize<'slot>(
            self,
            _slot: InitSlot<'slot, Tracked>,
        ) -> Result<InitReceipt<'slot, Tracked>, Failure> {
            Err(Failure::Probe)
        }
    }

    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    let initializer = init!(Container {
        first <- TrackedInit(7).map_err(|never| -> Failure { match never {} }),
        second <- FailingInit,
    } ? Failure);
    let mut storage = MaybeUninit::uninit();
    let result = init_in(&mut storage, initializer);
    assert!(matches!(result, Err(Failure::Probe)));
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[test]
fn panic_drops_fields_that_were_already_initialized() {
    struct PanicInit;

    // 安全性依据：panic 发生在任何写入之前，槽位保持未初始化。
    // Safety: the panic occurs before any write, so the slot remains uninitialized.
    unsafe impl Init<Tracked> for PanicInit {
        fn initialize<'slot>(
            self,
            _slot: InitSlot<'slot, Tracked>,
        ) -> Result<InitReceipt<'slot, Tracked>, Infallible> {
            panic!("初始化失败");
        }
    }

    let _lock = DROPS_LOCK.lock().unwrap();
    DROPS.store(0, Ordering::SeqCst);
    let initializer = init!(Container {
        first <- TrackedInit(13),
        second <- PanicInit,
    });
    let mut storage = MaybeUninit::uninit();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = init_in(&mut storage, initializer);
    }));
    assert!(result.is_err());
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[test]
fn constant_initializers_write_values_in_place() {
    let mut movable_storage = MaybeUninit::uninit();
    {
        let value = init_in(&mut movable_storage, InitTyU32::<42>).unwrap();
        assert_eq!(*value, 42);
    }
    #[derive(Init)]
    struct PinnedBool {
        value: bool,
    }

    pin_init_local! {
        let mut pinned_value = PinnedBool {
            value <- InitTyBool::<true>,
        };
    }
    assert!(pinned_value.value);
}

#[test]
fn const_init_targets_support_in_place_scalar_defaults() {
    assert_scalar_default::<bool>(false);
    assert_scalar_default::<char>('\0');
    assert_scalar_default::<i8>(0);
    assert_scalar_default::<i16>(0);
    assert_scalar_default::<i32>(0);
    assert_scalar_default::<i64>(0);
    assert_scalar_default::<i128>(0);
    assert_scalar_default::<isize>(0);
    assert_scalar_default::<u8>(0);
    assert_scalar_default::<u16>(0);
    assert_scalar_default::<u32>(0);
    assert_scalar_default::<u64>(0);
    assert_scalar_default::<u128>(0);
    assert_scalar_default::<usize>(0);

    assert_default_provider::<InitTyBool<true>, bool>(true);
    assert_default_provider::<InitTyChar<'x'>, char>('x');
    assert_default_provider::<InitTyI8<1>, i8>(1);
    assert_default_provider::<InitTyI16<2>, i16>(2);
    assert_default_provider::<InitTyI32<3>, i32>(3);
    assert_default_provider::<InitTyI64<4>, i64>(4);
    assert_default_provider::<InitTyI128<5>, i128>(5);
    assert_default_provider::<InitTyIsize<6>, isize>(6);
    assert_default_provider::<InitTyU8<7>, u8>(7);
    assert_default_provider::<InitTyU16<8>, u16>(8);
    assert_default_provider::<InitTyU32<9>, u32>(9);
    assert_default_provider::<InitTyU64<10>, u64>(10);
    assert_default_provider::<InitTyU128<11>, u128>(11);
    assert_default_provider::<InitTyUsize<12>, usize>(12);

    let mut movable_storage = MaybeUninit::uninit();
    let value = init_in(&mut movable_storage, init!(u32 { .. })).unwrap();
    assert_eq!(*value, 0);

    pin_init_local! {
        let mut pinned_value = bool { .. };
    }
    assert!(!*pinned_value);

    pin_init_local! {
        let mut pinned_value = PinnedDefaultBuilder {
            count <- InitTyU32::<13>,
        };
    }
    assert_eq!(pinned_value.count, 13);
}

#[cfg(feature = "alloc")]
#[test]
fn pinned_box_initializes_self_reference_at_final_address() {
    #[derive(Init)]
    #[repr(C)]
    struct SelfRef {
        #[pin]
        address: *const SelfRef,
        #[pin]
        pinned: PhantomPinned,
    }

    struct SelfPointerInit;

    unsafe impl InitRaw<*const SelfRef, Infallible, PinInitMode> for SelfPointerInit {
        unsafe fn init_raw(self, slot: *mut *const SelfRef) -> Result<(), Infallible> {
            // 安全性：address 是首字段，其地址与 SelfRef 的最终地址相同。
            unsafe { slot.write(slot.cast::<SelfRef>()) };
            Ok(())
        }
    }

    struct PinnedInit;

    // 安全性依据：PhantomPinned 的值不依赖自身地址，槽位写入完整值并返回唯一凭证。
    // Safety: PhantomPinned does not depend on its address, and the slot write returns the sole receipt for the complete value.
    unsafe impl Init<PhantomPinned, Infallible, PinInitMode> for PinnedInit {
        fn initialize<'slot>(
            self,
            slot: InitSlot<'slot, PhantomPinned, PinInitMode>,
        ) -> Result<InitReceipt<'slot, PhantomPinned, PinInitMode>, Infallible> {
            Ok(slot.write(PhantomPinned))
        }
    }

    pin_init_local! {
        let mut local_value = SelfRef {
            address <- @target,
            pinned <- PinnedInit,
        };
    }
    let local_address = ptr::addr_of!(*local_value);
    assert_eq!(local_value.address, local_address);
    let _ = &local_value.pinned;

    let initializer = pin_init!(SelfRef {
        address <- @target,
        pinned <- PinnedInit,
    });
    let value = initializer.pin_box().unwrap();
    let final_address = ptr::addr_of!(*value);
    assert_eq!(value.address, final_address);
    let _ = &value.pinned;

    let builder_value = SelfRef::pin_init()
        .address(RawInit::new(SelfPointerInit))
        .pinned(PinnedInit)
        .build()
        .pin_box()
        .unwrap();
    let builder_address = ptr::addr_of!(*builder_value);
    assert_eq!(builder_value.address, builder_address);
    let _ = &builder_value.pinned;
}

#[derive(Init)]
struct GenericPair<A, B> {
    first: A,
    second: B,
}

struct U8Init;

// 安全性依据：初始化器一次性写入完整标量并返回其唯一凭证。
// Safety: the initializer writes one complete scalar and returns its sole receipt.
unsafe impl Init<u8> for U8Init {
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, u8>,
    ) -> Result<InitReceipt<'slot, u8>, Infallible> {
        Ok(slot.write(7))
    }
}

struct U16Init;

// 安全性依据：初始化器一次性写入完整标量并返回其唯一凭证。
// Safety: the initializer writes one complete scalar and returns its sole receipt.
unsafe impl Init<u16> for U16Init {
    fn initialize<'slot>(
        self,
        slot: InitSlot<'slot, u16>,
    ) -> Result<InitReceipt<'slot, u16>, Infallible> {
        Ok(slot.write(19))
    }
}

#[test]
fn macro_supports_generic_struct_types_with_multiple_parameters() {
    let initializer = init!(GenericPair<u8, u16> {
        first <- U8Init,
        second <- U16Init,
    });
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.first, 7);
    assert_eq!(value.second, 19);
}

#[cfg(feature = "alloc")]
#[test]
fn box_init_returns_a_movable_value() {
    let initializer = GenericPair::<u8, u16>::init()
        .first(U8Init)
        .second(U16Init)
        .build();
    let value = initializer.init_box().unwrap();
    assert_eq!(value.first, 7);
    assert_eq!(value.second, 19);
}

#[derive(Init)]
struct TuplePair<A, B>(A, B);

#[test]
fn macro_supports_tuple_struct_fields() {
    let initializer = init!(TuplePair<u8, u16> {
        0 <- U8Init,
        1 <- U16Init,
    });
    let storage = InitStorage::uninit();
    let value = storage.initialize(initializer).unwrap();
    assert_eq!(value.0, 7);
    assert_eq!(value.1, 19);
}
