//! Generation-checked native ownership for Flark v3 parser endpoints.
//!
//! A registry handle is two scalar values, never a persistent pointer. The
//! slot generation changes before a slot can be reused, and every operation
//! resolves the pair to an `Arc<Mutex<Endpoint>>`. This lets a logically
//! serialized Dart isolate migrate between host threads while preserving
//! exclusive endpoint mutation.

use std::fmt;
use std::sync::{Arc, Mutex, TryLockError};

use crate::v3_endpoint::{Endpoint, EndpointConfig, EndpointLifecycle};
use crate::v3_session_wire::SessionBinding;

/// Hard process-wide ceiling for resident native parser endpoint slots.
///
/// Recovery is create-before-revoke: a replacement is made live before its
/// predecessor's handle is retired. Half of these slots are therefore kept
/// out of fresh admission so every endpoint admitted at the advertised
/// steady-state ceiling can have one replacement resident concurrently.
pub const MAXIMUM_RESIDENT_ENDPOINT_SLOTS: u32 = 4_096;
/// Maximum process-wide steady-state admission for fresh parser endpoints.
pub const MAXIMUM_FRESH_ENDPOINTS: u32 = MAXIMUM_RESIDENT_ENDPOINT_SLOTS / 2;

/// Opaque registry identity. Both words are required for every lookup.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EndpointHandle {
    slot: u32,
    generation: u32,
}

impl EndpointHandle {
    #[must_use]
    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    InvalidLimit,
    InvalidHandle,
    StaleHandle,
    CapacityExceeded,
    AllocationFailed,
    InvalidRecovery,
    Poisoned,
    InUse,
    NotRemovable,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Flark v3 endpoint registry failure: {self:?}")
    }
}

impl std::error::Error for RegistryError {}

struct RegistrySlot {
    generation: u32,
    endpoint: Option<Arc<Mutex<Endpoint>>>,
    next_free: Option<u32>,
    generation_exhausted: bool,
}

struct RegistryState {
    slots: Vec<RegistrySlot>,
    free_head: Option<u32>,
    resident_endpoints: u32,
    generation_exhausted_slots: u32,
}

/// Bounded registry of independently serialized parser endpoints.
pub struct EndpointRegistry {
    maximum_fresh_endpoints: u32,
    maximum_resident_slots: u32,
    state: Mutex<RegistryState>,
}

#[derive(Clone, Copy)]
enum Admission {
    Fresh,
    Recovery,
}

impl EndpointRegistry {
    /// Creates a registry with equal steady-state and recovery headroom.
    ///
    /// `maximum_fresh_endpoints` is the advertised logical admission limit;
    /// the registry reserves the same number of physical slots for
    /// create-before-revoke recovery replacements.
    pub fn new(maximum_fresh_endpoints: u32) -> Result<Self, RegistryError> {
        if maximum_fresh_endpoints == 0 || maximum_fresh_endpoints > MAXIMUM_FRESH_ENDPOINTS {
            return Err(RegistryError::InvalidLimit);
        }
        let maximum_resident_slots = maximum_fresh_endpoints
            .checked_mul(2)
            .filter(|slots| *slots <= MAXIMUM_RESIDENT_ENDPOINT_SLOTS)
            .ok_or(RegistryError::InvalidLimit)?;
        Ok(Self {
            maximum_fresh_endpoints,
            maximum_resident_slots,
            state: Mutex::new(RegistryState {
                slots: Vec::new(),
                free_head: None,
                resident_endpoints: 0,
                generation_exhausted_slots: 0,
            }),
        })
    }

    pub fn production() -> Self {
        Self::new(MAXIMUM_FRESH_ENDPOINTS)
            .expect("the declared production endpoint limit must be valid")
    }

    pub fn create_fresh(&self, config: EndpointConfig) -> Result<EndpointHandle, RegistryError> {
        self.insert(Endpoint::fresh(config), Admission::Fresh)
    }

    pub fn create_recovery(
        &self,
        previous: SessionBinding,
        config: EndpointConfig,
    ) -> Result<EndpointHandle, RegistryError> {
        let endpoint =
            Endpoint::recovery(previous, config).map_err(|_| RegistryError::InvalidRecovery)?;
        self.insert(endpoint, Admission::Recovery)
    }

    /// Runs one operation under the endpoint's exclusive lock. The registry
    /// lock is released before waiting for the endpoint lock.
    pub fn with_endpoint<R>(
        &self,
        handle: EndpointHandle,
        operation: impl FnOnce(&mut Endpoint) -> R,
    ) -> Result<R, RegistryError> {
        let endpoint = self.resolve(handle)?;
        let mut endpoint = match endpoint.try_lock() {
            Ok(endpoint) => endpoint,
            Err(TryLockError::WouldBlock) => return Err(RegistryError::InUse),
            Err(TryLockError::Poisoned(_)) => return Err(RegistryError::Poisoned),
        };
        Ok(operation(&mut endpoint))
    }

    /// Validates only the generation-checked registry identity without
    /// acquiring the endpoint operation lock.
    pub fn validate_live(&self, handle: EndpointHandle) -> Result<(), RegistryError> {
        let state = self.state.lock().map_err(|_| RegistryError::Poisoned)?;
        let _ = validate_handle(&state, handle)?;
        Ok(())
    }

    /// Removes a normally closed endpoint. No already-started operation may
    /// still hold the endpoint, and the accepted `Closed` receipt must already
    /// have advanced it to `Removable`.
    pub fn remove(&self, handle: EndpointHandle) -> Result<(), RegistryError> {
        let endpoint = self.resolve(handle)?;
        let endpoint_guard = match endpoint.try_lock() {
            Ok(endpoint) => endpoint,
            Err(TryLockError::WouldBlock) => return Err(RegistryError::InUse),
            Err(TryLockError::Poisoned(_)) => return Err(RegistryError::Poisoned),
        };
        if endpoint_guard.status().lifecycle != EndpointLifecycle::Removable {
            return Err(RegistryError::NotRemovable);
        }
        let registered = {
            let mut state = self.state.lock().map_err(|_| RegistryError::Poisoned)?;
            let index = validate_handle(&state, handle)?;
            {
                let registered = state.slots[index]
                    .endpoint
                    .as_ref()
                    .ok_or(RegistryError::StaleHandle)?;
                if !Arc::ptr_eq(registered, &endpoint) {
                    return Err(RegistryError::StaleHandle);
                }
                // Exactly the slot plus this remove operation may own the Arc.
                if Arc::strong_count(registered) != 2 {
                    return Err(RegistryError::InUse);
                }
            }
            let registered = state.slots[index]
                .endpoint
                .take()
                .ok_or(RegistryError::StaleHandle)?;
            recycle(&mut state, index);
            registered
        };
        drop(endpoint_guard);
        drop(registered);
        drop(endpoint);
        Ok(())
    }

    /// Revokes a handle without the credited close protocol. This is reserved
    /// for process/isolate finalization and invokes Endpoint's visibly
    /// unmetered emergency containment Drop. It fails with `InUse` rather than
    /// recycling a slot while an operation lease still owns the old endpoint,
    /// preserving the hard resident-endpoint ceiling.
    pub fn emergency_destroy(&self, handle: EndpointHandle) -> Result<(), RegistryError> {
        let endpoint = {
            let mut state = self.state.lock().map_err(|_| RegistryError::Poisoned)?;
            let index = validate_handle(&state, handle)?;
            let endpoint = state.slots[index]
                .endpoint
                .as_ref()
                .ok_or(RegistryError::StaleHandle)?;
            if Arc::strong_count(endpoint) != 1 {
                return Err(RegistryError::InUse);
            }
            let endpoint = state.slots[index]
                .endpoint
                .take()
                .ok_or(RegistryError::StaleHandle)?;
            recycle(&mut state, index);
            endpoint
        };
        drop(endpoint);
        Ok(())
    }

    fn insert(
        &self,
        endpoint: Endpoint,
        admission: Admission,
    ) -> Result<EndpointHandle, RegistryError> {
        let endpoint = Arc::new(Mutex::new(endpoint));
        let mut state = self.state.lock().map_err(|_| RegistryError::Poisoned)?;
        match admission {
            Admission::Fresh if !self.can_admit_fresh(&state) => {
                return Err(RegistryError::CapacityExceeded);
            }
            Admission::Recovery if state.resident_endpoints >= self.maximum_resident_slots => {
                return Err(RegistryError::CapacityExceeded);
            }
            Admission::Fresh | Admission::Recovery => {}
        }
        if let Some(index_u32) = state.free_head {
            let index = index_u32 as usize;
            let next = state.slots[index].next_free;
            state.free_head = next;
            let generation = {
                let slot = &mut state.slots[index];
                debug_assert!(slot.endpoint.is_none());
                debug_assert!(!slot.generation_exhausted);
                slot.next_free = None;
                slot.endpoint = Some(endpoint);
                slot.generation
            };
            state.resident_endpoints += 1;
            return Ok(EndpointHandle::from_parts(index_u32 + 1, generation));
        }

        if state.slots.len() >= self.maximum_resident_slots as usize {
            return Err(RegistryError::CapacityExceeded);
        }
        state
            .slots
            .try_reserve(1)
            .map_err(|_| RegistryError::AllocationFailed)?;
        let index = state.slots.len();
        state.slots.push(RegistrySlot {
            generation: 1,
            endpoint: Some(endpoint),
            next_free: None,
            generation_exhausted: false,
        });
        state.resident_endpoints += 1;
        let slot = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(RegistryError::CapacityExceeded)?;
        Ok(EndpointHandle::from_parts(slot, 1))
    }

    fn can_admit_fresh(&self, state: &RegistryState) -> bool {
        let Some(resident_after_admission) = state.resident_endpoints.checked_add(1) else {
            return false;
        };
        if resident_after_admission > self.maximum_fresh_endpoints {
            return false;
        }

        // A permanently exhausted generation consumes its physical slot. Do
        // not silently spend another endpoint's promised recovery headroom by
        // admitting a fresh endpoint after effective capacity has degraded.
        let reusable_physical_slots = self
            .maximum_resident_slots
            .saturating_sub(state.generation_exhausted_slots);
        resident_after_admission
            .checked_mul(2)
            .is_some_and(|required| required <= reusable_physical_slots)
    }

    fn resolve(&self, handle: EndpointHandle) -> Result<Arc<Mutex<Endpoint>>, RegistryError> {
        let state = self.state.lock().map_err(|_| RegistryError::Poisoned)?;
        let index = validate_handle(&state, handle)?;
        state.slots[index]
            .endpoint
            .as_ref()
            .cloned()
            .ok_or(RegistryError::StaleHandle)
    }
}

fn validate_handle(state: &RegistryState, handle: EndpointHandle) -> Result<usize, RegistryError> {
    if handle.slot == 0 || handle.generation == 0 {
        return Err(RegistryError::InvalidHandle);
    }
    let index = (handle.slot - 1) as usize;
    let slot = state.slots.get(index).ok_or(RegistryError::InvalidHandle)?;
    if slot.generation != handle.generation || slot.endpoint.is_none() {
        return Err(RegistryError::StaleHandle);
    }
    Ok(index)
}

fn recycle(state: &mut RegistryState, index: usize) {
    let free_head = state.free_head;
    debug_assert!(state.resident_endpoints > 0);
    state.resident_endpoints -= 1;
    let slot = &mut state.slots[index];
    debug_assert!(slot.endpoint.is_none());
    match slot.generation.checked_add(1) {
        Some(generation) => {
            slot.generation = generation;
            slot.next_free = free_head;
            state.free_head = Some(index as u32);
        }
        None => {
            slot.generation_exhausted = true;
            slot.next_free = None;
            state.generation_exhausted_slots += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3_endpoint::EndpointConfig;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Barrier;
    use std::thread;

    fn config() -> EndpointConfig {
        EndpointConfig::standard().unwrap()
    }

    fn binding(identity: u32, worker_generation: u32) -> SessionBinding {
        SessionBinding {
            document_session: [identity, 2, 3, 4],
            source_session_identity: identity,
            worker_generation,
        }
    }

    #[test]
    fn invalid_limits_and_handles_are_rejected_without_allocating() {
        assert!(matches!(
            EndpointRegistry::new(0),
            Err(RegistryError::InvalidLimit)
        ));
        assert!(matches!(
            EndpointRegistry::new(MAXIMUM_FRESH_ENDPOINTS + 1),
            Err(RegistryError::InvalidLimit)
        ));
        let registry = EndpointRegistry::new(1).unwrap();
        assert!(matches!(
            registry.with_endpoint(EndpointHandle::from_parts(0, 1), |_| ()),
            Err(RegistryError::InvalidHandle)
        ));
        assert!(matches!(
            registry.with_endpoint(EndpointHandle::from_parts(1, 0), |_| ()),
            Err(RegistryError::InvalidHandle)
        ));
    }

    #[test]
    fn emergency_reuse_changes_generation_and_old_handle_never_aliases() {
        let registry = EndpointRegistry::new(1).unwrap();
        let first = registry.create_fresh(config()).unwrap();
        assert_eq!(first, EndpointHandle::from_parts(1, 1));
        registry.emergency_destroy(first).unwrap();

        let second = registry.create_fresh(config()).unwrap();
        assert_eq!(second, EndpointHandle::from_parts(1, 2));
        assert!(matches!(
            registry.with_endpoint(first, |_| ()),
            Err(RegistryError::StaleHandle)
        ));
        registry.with_endpoint(second, |_| ()).unwrap();
        registry.emergency_destroy(second).unwrap();
    }

    #[test]
    fn capacity_is_hard_and_reuse_needs_no_free_list_allocation() {
        let registry = EndpointRegistry::new(1).unwrap();
        let first = registry.create_fresh(config()).unwrap();
        assert!(matches!(
            registry.create_fresh(config()),
            Err(RegistryError::CapacityExceeded)
        ));
        registry.emergency_destroy(first).unwrap();
        let second = registry.create_fresh(config()).unwrap();
        assert_eq!(second.slot(), first.slot());
        assert_ne!(second.generation(), first.generation());
        registry.emergency_destroy(second).unwrap();
    }

    #[test]
    fn full_fresh_admission_reserves_one_concurrent_recovery_per_endpoint() {
        let registry = EndpointRegistry::production();
        let fresh = (1..=MAXIMUM_FRESH_ENDPOINTS)
            .map(|_| registry.create_fresh(config()).expect("fresh admission"))
            .collect::<Vec<_>>();

        assert!(matches!(
            registry.create_fresh(config()),
            Err(RegistryError::CapacityExceeded)
        ));

        let recoveries = (1..=MAXIMUM_FRESH_ENDPOINTS)
            .map(|identity| {
                registry
                    .create_recovery(binding(identity, 1), config())
                    .expect("reserved recovery admission")
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            registry.create_fresh(config()),
            Err(RegistryError::CapacityExceeded)
        ));
        assert!(matches!(
            registry.create_recovery(binding(MAXIMUM_FRESH_ENDPOINTS + 1, 1), config()),
            Err(RegistryError::CapacityExceeded)
        ));
        {
            let state = registry.state.lock().unwrap();
            assert_eq!(state.resident_endpoints, MAXIMUM_RESIDENT_ENDPOINT_SLOTS);
            assert_eq!(state.slots.len(), MAXIMUM_RESIDENT_ENDPOINT_SLOTS as usize);
        }

        for handle in fresh.iter().copied() {
            registry.emergency_destroy(handle).unwrap();
            assert!(matches!(
                registry.validate_live(handle),
                Err(RegistryError::StaleHandle)
            ));
        }
        for handle in recoveries.iter().copied() {
            registry.validate_live(handle).unwrap();
        }

        // Every replacement can itself be replaced concurrently, reusing the
        // predecessor slots without growing past the hard physical ceiling.
        let second_recoveries = (1..=MAXIMUM_FRESH_ENDPOINTS)
            .map(|identity| {
                registry
                    .create_recovery(binding(identity, 2), config())
                    .expect("reused recovery admission")
            })
            .collect::<Vec<_>>();
        {
            let state = registry.state.lock().unwrap();
            assert_eq!(state.resident_endpoints, MAXIMUM_RESIDENT_ENDPOINT_SLOTS);
            assert_eq!(state.slots.len(), MAXIMUM_RESIDENT_ENDPOINT_SLOTS as usize);
        }

        for handle in recoveries {
            registry.emergency_destroy(handle).unwrap();
        }
        for handle in second_recoveries {
            registry.emergency_destroy(handle).unwrap();
        }
    }

    #[test]
    fn recovery_rejects_invalid_or_exhausted_previous_binding() {
        let registry = EndpointRegistry::new(2).unwrap();
        let invalid = SessionBinding {
            document_session: [1, 2, 3, 4],
            source_session_identity: 0,
            worker_generation: 1,
        };
        assert!(matches!(
            registry.create_recovery(invalid, config()),
            Err(RegistryError::InvalidRecovery)
        ));
        let exhausted = SessionBinding {
            source_session_identity: 1,
            worker_generation: u32::MAX,
            ..invalid
        };
        assert!(matches!(
            registry.create_recovery(exhausted, config()),
            Err(RegistryError::InvalidRecovery)
        ));
    }

    #[test]
    fn registry_endpoint_container_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EndpointRegistry>();
        assert_send_sync::<Arc<Mutex<Endpoint>>>();
    }

    #[test]
    fn sequential_host_thread_migration_preserves_one_endpoint() {
        let registry = Arc::new(EndpointRegistry::new(1).unwrap());
        let handle = registry.create_fresh(config()).unwrap();
        let first_registry = Arc::clone(&registry);
        thread::spawn(move || {
            first_registry
                .with_endpoint(handle, |endpoint| endpoint.status().lifecycle)
                .unwrap()
        })
        .join()
        .unwrap();
        let second_registry = Arc::clone(&registry);
        assert_eq!(
            thread::spawn(move || {
                second_registry
                    .with_endpoint(handle, |endpoint| endpoint.status().lifecycle)
                    .unwrap()
            })
            .join()
            .unwrap(),
            EndpointLifecycle::AwaitingOpen
        );
        registry.emergency_destroy(handle).unwrap();
    }

    #[test]
    fn concurrent_call_is_backpressure_and_reuse_cannot_alias_in_flight_arc() {
        let registry = Arc::new(EndpointRegistry::new(1).unwrap());
        let first = registry.create_fresh(config()).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let thread_registry = Arc::clone(&registry);
        let thread_entered = Arc::clone(&entered);
        let thread_release = Arc::clone(&release);
        let in_flight = thread::spawn(move || {
            thread_registry
                .with_endpoint(first, |_| {
                    thread_entered.wait();
                    thread_release.wait();
                })
                .unwrap();
        });
        entered.wait();

        assert!(matches!(
            registry.with_endpoint(first, |_| ()),
            Err(RegistryError::InUse)
        ));
        assert!(matches!(
            registry.emergency_destroy(first),
            Err(RegistryError::InUse)
        ));
        assert!(matches!(
            registry.create_fresh(config()),
            Err(RegistryError::CapacityExceeded)
        ));

        release.wait();
        in_flight.join().unwrap();
        registry.emergency_destroy(first).unwrap();
        let second = registry.create_fresh(config()).unwrap();
        assert_eq!(second.slot(), first.slot());
        assert_ne!(second.generation(), first.generation());
        assert!(matches!(
            registry.with_endpoint(first, |_| ()),
            Err(RegistryError::StaleHandle)
        ));
        assert_eq!(
            registry
                .with_endpoint(second, |endpoint| endpoint.status().lifecycle)
                .unwrap(),
            EndpointLifecycle::AwaitingOpen
        );

        registry.emergency_destroy(second).unwrap();
    }

    #[test]
    fn endpoint_work_does_not_hold_the_global_registry_lock() {
        let registry = Arc::new(EndpointRegistry::new(2).unwrap());
        let first = registry.create_fresh(config()).unwrap();
        let second = registry.create_fresh(config()).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let thread_registry = Arc::clone(&registry);
        let thread_entered = Arc::clone(&entered);
        let thread_release = Arc::clone(&release);
        let worker = thread::spawn(move || {
            thread_registry
                .with_endpoint(first, |_| {
                    thread_entered.wait();
                    thread_release.wait();
                })
                .unwrap();
        });
        entered.wait();
        assert_eq!(
            registry
                .with_endpoint(second, |endpoint| endpoint.status().lifecycle)
                .unwrap(),
            EndpointLifecycle::AwaitingOpen
        );
        release.wait();
        worker.join().unwrap();
        registry.emergency_destroy(first).unwrap();
        registry.emergency_destroy(second).unwrap();
    }

    #[test]
    fn normal_remove_never_recycles_while_an_operation_lease_exists() {
        let registry = EndpointRegistry::new(1).unwrap();
        let handle = registry.create_fresh(config()).unwrap();
        let operation_lease = registry.resolve(handle).unwrap();
        let operation_guard = operation_lease.try_lock().unwrap();
        assert!(matches!(registry.remove(handle), Err(RegistryError::InUse)));
        drop(operation_guard);
        drop(operation_lease);
        assert!(matches!(
            registry.remove(handle),
            Err(RegistryError::NotRemovable)
        ));
        registry.emergency_destroy(handle).unwrap();
    }

    #[test]
    fn endpoint_panic_is_contained_as_a_poisoned_handle() {
        let registry = EndpointRegistry::new(1).unwrap();
        let handle = registry.create_fresh(config()).unwrap();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _ = registry.with_endpoint(handle, |_| panic!("synthetic endpoint panic"));
        }));
        assert!(panic.is_err());
        assert!(matches!(
            registry.with_endpoint(handle, |_| ()),
            Err(RegistryError::Poisoned)
        ));
        registry.emergency_destroy(handle).unwrap();
    }

    #[test]
    fn exhausted_slot_generation_is_permanently_retired() {
        let registry = EndpointRegistry::new(1).unwrap();
        let initial = registry.create_fresh(config()).unwrap();
        {
            let mut state = registry.state.lock().unwrap();
            state.slots[0].generation = u32::MAX;
        }
        let exhausted = EndpointHandle::from_parts(initial.slot(), u32::MAX);
        registry.emergency_destroy(exhausted).unwrap();
        {
            let state = registry.state.lock().unwrap();
            assert!(state.slots[0].generation_exhausted);
            assert!(state.free_head.is_none());
        }
        assert!(matches!(
            registry.create_fresh(config()),
            Err(RegistryError::CapacityExceeded)
        ));
    }
}
