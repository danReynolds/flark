//! Generation-checked process registry for independent native candidate hosts.

use std::fmt;
use std::sync::{Arc, Mutex, TryLockError};

use crate::v3_host_store::{HostConfig, HostStoreError, NativeCandidateHost};
use crate::v3_publication_wire::decode_publication_packet_envelope;

pub const MAXIMUM_RESIDENT_HOSTS: u32 = 2_048;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HostHandle {
    slot: u32,
    generation: u32,
}

impl HostHandle {
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
pub enum HostRegistryError {
    InvalidLimit,
    InvalidConfig,
    InvalidPacketEnvelope,
    InvalidHandle,
    StaleHandle,
    CapacityExceeded,
    AllocationFailed,
    Poisoned,
    InUse,
    NotRemovable,
}

impl fmt::Display for HostRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Flark v3 host registry failure: {self:?}")
    }
}

impl std::error::Error for HostRegistryError {}

struct HostSlot {
    generation: u32,
    host: Option<Arc<Mutex<NativeCandidateHost>>>,
    next_free: Option<u32>,
    generation_exhausted: bool,
}

struct HostRegistryState {
    slots: Vec<HostSlot>,
    free_head: Option<u32>,
    resident_hosts: u32,
    generation_exhausted_slots: u32,
}

pub struct HostRegistry {
    maximum_hosts: u32,
    state: Mutex<HostRegistryState>,
}

impl HostRegistry {
    pub fn new(maximum_hosts: u32) -> Result<Self, HostRegistryError> {
        if maximum_hosts == 0 || maximum_hosts > MAXIMUM_RESIDENT_HOSTS {
            return Err(HostRegistryError::InvalidLimit);
        }
        Ok(Self {
            maximum_hosts,
            state: Mutex::new(HostRegistryState {
                slots: Vec::new(),
                free_head: None,
                resident_hosts: 0,
                generation_exhausted_slots: 0,
            }),
        })
    }

    pub fn production() -> Self {
        Self::new(MAXIMUM_RESIDENT_HOSTS).expect("the declared production host limit must be valid")
    }

    pub fn create(&self, config: HostConfig) -> Result<HostHandle, HostRegistryError> {
        let host =
            NativeCandidateHost::new(config).map_err(|_| HostRegistryError::InvalidConfig)?;
        let host = Arc::new(Mutex::new(host));
        let mut state = self.state.lock().map_err(|_| HostRegistryError::Poisoned)?;
        let reusable_capacity = self
            .maximum_hosts
            .saturating_sub(state.generation_exhausted_slots);
        if state.resident_hosts >= reusable_capacity {
            return Err(HostRegistryError::CapacityExceeded);
        }
        if let Some(index_u32) = state.free_head {
            let index = index_u32 as usize;
            state.free_head = state.slots[index].next_free;
            let generation = {
                let slot = &mut state.slots[index];
                debug_assert!(slot.host.is_none());
                debug_assert!(!slot.generation_exhausted);
                slot.next_free = None;
                slot.host = Some(host);
                slot.generation
            };
            state.resident_hosts += 1;
            return Ok(HostHandle::from_parts(index_u32 + 1, generation));
        }
        if state.slots.len() >= self.maximum_hosts as usize {
            return Err(HostRegistryError::CapacityExceeded);
        }
        state
            .slots
            .try_reserve(1)
            .map_err(|_| HostRegistryError::AllocationFailed)?;
        let index = state.slots.len();
        state.slots.push(HostSlot {
            generation: 1,
            host: Some(host),
            next_free: None,
            generation_exhausted: false,
        });
        state.resident_hosts += 1;
        let slot = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(HostRegistryError::CapacityExceeded)?;
        Ok(HostHandle::from_parts(slot, 1))
    }

    pub fn with_host<R>(
        &self,
        handle: HostHandle,
        operation: impl FnOnce(&mut NativeCandidateHost) -> R,
    ) -> Result<R, HostRegistryError> {
        let host = self.resolve(handle)?;
        let mut host = match host.try_lock() {
            Ok(host) => host,
            Err(TryLockError::WouldBlock) => return Err(HostRegistryError::InUse),
            Err(TryLockError::Poisoned(_)) => return Err(HostRegistryError::Poisoned),
        };
        Ok(operation(&mut host))
    }

    pub fn validate_live(&self, handle: HostHandle) -> Result<(), HostRegistryError> {
        let state = self.state.lock().map_err(|_| HostRegistryError::Poisoned)?;
        let _ = validate_handle(&state, handle)?;
        Ok(())
    }

    /// Envelope-decodes one exact FPK3 packet before serializing admission on
    /// the addressed host. The decode is fixed-header work; descriptor and
    /// frame validation remain fuelled host-poll work. An invalid envelope is
    /// an ABI-level invalid command, not an offer-scoped host rejection.
    pub fn admit_packet(
        &self,
        handle: HostHandle,
        encoded: &[u8],
    ) -> Result<Result<(), HostStoreError>, HostRegistryError> {
        let packet = decode_publication_packet_envelope(encoded)
            .map_err(|_| HostRegistryError::InvalidPacketEnvelope)?;
        self.with_host(handle, |host| host.admit_packet(packet))
    }

    /// Envelope-decodes one exact FPK3 packet before serializing admission on
    /// the addressed viewport-presentation host. VPB1 wrapper, child closure,
    /// ordering, and digest validation remain fuelled host-poll work.
    pub fn admit_viewport_presentation_packet(
        &self,
        handle: HostHandle,
        encoded: &[u8],
    ) -> Result<Result<(), HostStoreError>, HostRegistryError> {
        let packet = decode_publication_packet_envelope(encoded)
            .map_err(|_| HostRegistryError::InvalidPacketEnvelope)?;
        self.with_host(handle, |host| {
            host.admit_viewport_presentation_packet(packet)
        })
    }

    pub fn remove(&self, handle: HostHandle) -> Result<(), HostRegistryError> {
        let host = self.resolve(handle)?;
        let guard = match host.try_lock() {
            Ok(host) => host,
            Err(TryLockError::WouldBlock) => return Err(HostRegistryError::InUse),
            Err(TryLockError::Poisoned(_)) => return Err(HostRegistryError::Poisoned),
        };
        if !guard.is_removable() {
            return Err(HostRegistryError::NotRemovable);
        }
        let registered = {
            let mut state = self.state.lock().map_err(|_| HostRegistryError::Poisoned)?;
            let index = validate_handle(&state, handle)?;
            let registered = state.slots[index]
                .host
                .as_ref()
                .ok_or(HostRegistryError::StaleHandle)?;
            if !Arc::ptr_eq(registered, &host) {
                return Err(HostRegistryError::StaleHandle);
            }
            if Arc::strong_count(registered) != 2 {
                return Err(HostRegistryError::InUse);
            }
            let registered = state.slots[index]
                .host
                .take()
                .ok_or(HostRegistryError::StaleHandle)?;
            recycle(&mut state, index);
            registered
        };
        drop(guard);
        drop(registered);
        drop(host);
        Ok(())
    }

    pub fn emergency_destroy(&self, handle: HostHandle) -> Result<(), HostRegistryError> {
        let host = {
            let mut state = self.state.lock().map_err(|_| HostRegistryError::Poisoned)?;
            let index = validate_handle(&state, handle)?;
            let host = state.slots[index]
                .host
                .as_ref()
                .ok_or(HostRegistryError::StaleHandle)?;
            if Arc::strong_count(host) != 1 {
                return Err(HostRegistryError::InUse);
            }
            let host = state.slots[index]
                .host
                .take()
                .ok_or(HostRegistryError::StaleHandle)?;
            recycle(&mut state, index);
            host
        };
        drop(host);
        Ok(())
    }

    fn resolve(
        &self,
        handle: HostHandle,
    ) -> Result<Arc<Mutex<NativeCandidateHost>>, HostRegistryError> {
        let state = self.state.lock().map_err(|_| HostRegistryError::Poisoned)?;
        let index = validate_handle(&state, handle)?;
        state.slots[index]
            .host
            .as_ref()
            .cloned()
            .ok_or(HostRegistryError::StaleHandle)
    }
}

fn validate_handle(
    state: &HostRegistryState,
    handle: HostHandle,
) -> Result<usize, HostRegistryError> {
    if handle.slot == 0 || handle.generation == 0 {
        return Err(HostRegistryError::InvalidHandle);
    }
    let index = (handle.slot - 1) as usize;
    let slot = state
        .slots
        .get(index)
        .ok_or(HostRegistryError::InvalidHandle)?;
    if slot.generation != handle.generation || slot.host.is_none() {
        return Err(HostRegistryError::StaleHandle);
    }
    Ok(index)
}

fn recycle(state: &mut HostRegistryState, index: usize) {
    debug_assert!(state.resident_hosts > 0);
    state.resident_hosts -= 1;
    let free_head = state.free_head;
    let slot = &mut state.slots[index];
    debug_assert!(slot.host.is_none());
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
    use crate::v3_host_store::{HostPollOutcome, HostWorkGrant};

    fn config(identity: u32) -> HostConfig {
        HostConfig {
            document_session: [identity, 2, 3, 4],
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 4 * 1024,
        }
    }

    #[test]
    fn emergency_reuse_changes_generation_and_stales_old_handle() {
        let registry = HostRegistry::new(1).unwrap();
        let first = registry.create(config(1)).unwrap();
        registry.emergency_destroy(first).unwrap();
        let second = registry.create(config(2)).unwrap();
        assert_eq!(second.slot(), first.slot());
        assert_ne!(second.generation(), first.generation());
        assert!(matches!(
            registry.validate_live(first),
            Err(HostRegistryError::StaleHandle)
        ));
        registry.emergency_destroy(second).unwrap();
    }

    #[test]
    fn exhausted_generation_is_never_reused_or_hidden_as_capacity() {
        let registry = HostRegistry::new(1).unwrap();
        {
            let mut state = registry.state.lock().unwrap();
            state.slots.push(HostSlot {
                generation: u32::MAX,
                host: Some(Arc::new(Mutex::new(
                    NativeCandidateHost::new(config(1)).unwrap(),
                ))),
                next_free: None,
                generation_exhausted: false,
            });
            state.resident_hosts = 1;
        }
        let handle = HostHandle::from_parts(1, u32::MAX);
        registry.emergency_destroy(handle).unwrap();
        assert!(matches!(
            registry.create(config(2)),
            Err(HostRegistryError::CapacityExceeded)
        ));
        let state = registry.state.lock().unwrap();
        assert_eq!(state.generation_exhausted_slots, 1);
        assert!(state.slots[0].generation_exhausted);
    }

    #[test]
    fn normal_remove_requires_fuelled_close_completion() {
        let registry = HostRegistry::new(1).unwrap();
        let handle = registry.create(config(1)).unwrap();
        assert!(matches!(
            registry.remove(handle),
            Err(HostRegistryError::NotRemovable)
        ));
        registry
            .with_host(handle, |host| {
                host.begin_close().unwrap();
                assert_eq!(
                    host.poll(HostWorkGrant {
                        inspect_bytes: 0,
                        copy_bytes: 0,
                        transitions: 1,
                    })
                    .unwrap(),
                    HostPollOutcome::Closed
                );
            })
            .unwrap();
        registry.remove(handle).unwrap();
        assert!(matches!(
            registry.validate_live(handle),
            Err(HostRegistryError::StaleHandle)
        ));
    }

    #[test]
    fn packet_registry_rejects_non_fpk3_bytes_before_host_admission() {
        let registry = HostRegistry::new(1).unwrap();
        let handle = registry.create(config(9)).unwrap();
        assert!(matches!(
            registry.admit_packet(handle, b"not-an-fpk3-packet"),
            Err(HostRegistryError::InvalidPacketEnvelope)
        ));
        registry.emergency_destroy(handle).unwrap();
    }
}
