//! Endpoint-owned persistent recursive-Green authority and session lifecycle.
//!
//! This module owns clean construction, local adoption, delivery commit,
//! cancellation, and bounded root cleanup. The candidate endpoint remains
//! the facade that schedules this authority alongside publication work.

use super::*;

struct InstalledRecursiveGreen {
    ack: StructuralAck,
    session: M11PersistentRecursiveGreenSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecursiveGreenCleanOrigin {
    Initial,
    IncrementalFallback,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecursiveGreenPathReceipt {
    pub(crate) local_adoption_deliveries: u64,
    pub(crate) clean_fallback_deliveries: u64,
}

enum PendingRecursiveGreen {
    CleanPlan {
        plan: Option<M11PersistentRecursiveGreenCleanPlan>,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    CleanBuild {
        build: M11PersistentRecursiveGreenCleanBuild,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    Adoption {
        base_ack: StructuralAck,
        adoption: M11PersistentRecursiveGreenAdoption,
    },
    CancellingAdoptionForFallback {
        base_ack: StructuralAck,
        syntax_profile: u32,
        adoption: M11PersistentRecursiveGreenAdoption,
        begun: bool,
    },
    ReadyClean {
        target: M11PersistentRecursiveGreenSession,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    ReadyUpdate {
        base_ack: StructuralAck,
        update: M11PersistentRecursiveGreenUpdate,
    },
}

enum RecursiveGreenCleanup {
    Session {
        session: M11PersistentRecursiveGreenSession,
        begun: bool,
    },
    CleanBuild {
        build: M11PersistentRecursiveGreenCleanBuild,
        restore: Option<InstalledRecursiveGreen>,
        begun: bool,
    },
    Adoption {
        adoption: M11PersistentRecursiveGreenAdoption,
        restore_ack: StructuralAck,
        begun: bool,
    },
}

/// Endpoint-owned recursive-Green/reference authority. The candidate's flat
/// publication remains a sidecar and cross-check; it never owns the ranges
/// returned for the recursive-Green inline-leaf target.
pub(super) struct RecursiveGreenEndpointSlot {
    installed: Option<InstalledRecursiveGreen>,
    pending: Option<PendingRecursiveGreen>,
    cleanup: VecDeque<RecursiveGreenCleanup>,
    path_receipt: RecursiveGreenPathReceipt,
}

impl RecursiveGreenEndpointSlot {
    pub(super) const fn new() -> Self {
        Self {
            installed: None,
            pending: None,
            cleanup: VecDeque::new(),
            path_receipt: RecursiveGreenPathReceipt {
                local_adoption_deliveries: 0,
                clean_fallback_deliveries: 0,
            },
        }
    }

    pub(super) const fn is_unowned(&self) -> bool {
        self.installed.is_none() && self.pending.is_none()
    }

    pub(super) const fn has_installed_session(&self) -> bool {
        self.installed.is_some()
    }

    pub(super) fn start_clean(
        &mut self,
        plan: M11PersistentRecursiveGreenCleanPlan,
    ) -> Result<(), CandidateEndpointError> {
        if self.pending.is_some() || (!self.cleanup.is_empty() && self.installed.is_none()) {
            return Err(CandidateEndpointError::Busy);
        }
        self.pending = Some(PendingRecursiveGreen::CleanPlan {
            plan: Some(plan),
            origin: if self.installed.is_some() {
                RecursiveGreenCleanOrigin::IncrementalFallback
            } else {
                RecursiveGreenCleanOrigin::Initial
            },
            base: self.installed.take(),
        });
        Ok(())
    }

    pub(super) fn start_incremental(
        &mut self,
        runtime: &DocumentRuntime,
        base_ack: StructuralAck,
        base_edit: Range<usize>,
        syntax_profile: u32,
    ) -> Result<(), CandidateEndpointError> {
        if self.pending.is_some() || (!self.cleanup.is_empty() && self.installed.is_none()) {
            return Err(CandidateEndpointError::Busy);
        }
        let target_plan = || -> Result<_, CandidateEndpointError> {
            Ok(M11PersistentRecursiveGreenCleanPlan::new(
                runtime.snapshot_current_source()?,
                runtime.snapshot_current_source()?,
                syntax_profile,
            )?)
        };
        let Some(installed) = self.installed.as_ref() else {
            self.pending = Some(PendingRecursiveGreen::CleanPlan {
                plan: Some(target_plan()?),
                base: None,
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
            });
            return Ok(());
        };
        if installed.ack != base_ack
            || installed.session.source().revision().get()
                != u64::from(base_ack.source_version.revision)
            || installed.session.syntax_profile() != syntax_profile
        {
            let plan = target_plan()?;
            let installed = self
                .installed
                .take()
                .ok_or(CandidateEndpointError::InvalidState)?;
            self.pending = Some(PendingRecursiveGreen::CleanPlan {
                plan: Some(plan),
                base: Some(installed),
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
            });
            return Ok(());
        }
        let target_lease = runtime.snapshot_current_source()?;
        let installed = self
            .installed
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match installed
            .session
            .begin_local_adoption(runtime, target_lease, base_edit)
        {
            Ok(adoption) => {
                self.pending = Some(PendingRecursiveGreen::Adoption { base_ack, adoption });
            }
            Err(failure) => {
                let base = InstalledRecursiveGreen {
                    ack: base_ack,
                    session: failure.into_base(),
                };
                let plan = match target_plan() {
                    Ok(plan) => plan,
                    Err(error) => {
                        self.installed = Some(base);
                        return Err(error);
                    }
                };
                self.pending = Some(PendingRecursiveGreen::CleanPlan {
                    plan: Some(plan),
                    base: Some(base),
                    origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                });
            }
        }
        Ok(())
    }

    pub(super) const fn target_work_pending(&self) -> bool {
        self.pending.is_some()
            && !matches!(
                self.pending.as_ref(),
                Some(
                    PendingRecursiveGreen::ReadyClean { .. }
                        | PendingRecursiveGreen::ReadyUpdate { .. }
                )
            )
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        !self.cleanup.is_empty()
    }

    pub(super) fn has_work(&self) -> bool {
        self.pending.is_some() || !self.cleanup.is_empty()
    }

    pub(super) fn poll_target(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        if fuel == 0 || !self.target_work_pending() {
            return Ok(0);
        }
        let pending = self
            .pending
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match pending {
            PendingRecursiveGreen::CleanPlan { plan, .. } => {
                let plan = plan.take().ok_or(CandidateEndpointError::InvalidState)?;
                let build = plan.begin(runtime)?;
                let (base, origin) = match self
                    .pending
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?
                {
                    PendingRecursiveGreen::CleanPlan { base, origin, .. } => (base, origin),
                    _ => return Err(CandidateEndpointError::InvalidState),
                };
                self.pending = Some(PendingRecursiveGreen::CleanBuild {
                    build,
                    base,
                    origin,
                });
                Ok(1)
            }
            PendingRecursiveGreen::CleanBuild { build, .. } => {
                let poll = build.poll(runtime, fuel)?;
                let transitions = poll.transitions();
                if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                    let target = build
                        .take_session()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let (base, origin) = match self
                        .pending
                        .take()
                        .ok_or(CandidateEndpointError::InvalidState)?
                    {
                        PendingRecursiveGreen::CleanBuild { base, origin, .. } => (base, origin),
                        _ => return Err(CandidateEndpointError::InvalidState),
                    };
                    self.pending = Some(PendingRecursiveGreen::ReadyClean {
                        target,
                        base,
                        origin,
                    });
                }
                Ok(transitions)
            }
            PendingRecursiveGreen::Adoption { base_ack, adoption } => {
                let poll = adoption.poll(runtime, fuel)?;
                let transitions = poll.transitions();
                match poll.status() {
                    M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                    M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                        let update = adoption
                            .take_update()
                            .ok_or(CandidateEndpointError::InvalidState)?;
                        self.pending = Some(PendingRecursiveGreen::ReadyUpdate {
                            base_ack: *base_ack,
                            update,
                        });
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                        let base_ack = *base_ack;
                        let syntax_profile = u32::try_from(base_ack.syntax_profile)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
                        let adoption = match self
                            .pending
                            .take()
                            .ok_or(CandidateEndpointError::InvalidState)?
                        {
                            PendingRecursiveGreen::Adoption { adoption, .. } => adoption,
                            _ => return Err(CandidateEndpointError::InvalidState),
                        };
                        self.pending = Some(PendingRecursiveGreen::CancellingAdoptionForFallback {
                            base_ack,
                            syntax_profile,
                            adoption,
                            begun: false,
                        });
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                }
                Ok(transitions)
            }
            PendingRecursiveGreen::CancellingAdoptionForFallback {
                base_ack,
                syntax_profile,
                adoption,
                begun,
            } => {
                let mut transitions = 0;
                if !*begun {
                    adoption.begin_cancel(runtime)?;
                    *begun = true;
                    transitions = 1;
                    if transitions == fuel {
                        return Ok(transitions);
                    }
                }
                if adoption.poll_cancel(runtime, fuel - transitions)? {
                    let plan = M11PersistentRecursiveGreenCleanPlan::new(
                        runtime.snapshot_current_source()?,
                        runtime.snapshot_current_source()?,
                        *syntax_profile,
                    )?;
                    let base = adoption
                        .take_base_after_cancel()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let base_ack = *base_ack;
                    self.pending = Some(PendingRecursiveGreen::CleanPlan {
                        plan: Some(plan),
                        base: Some(InstalledRecursiveGreen {
                            ack: base_ack,
                            session: base,
                        }),
                        origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                    });
                }
                Ok(fuel)
            }
            PendingRecursiveGreen::ReadyClean { .. }
            | PendingRecursiveGreen::ReadyUpdate { .. } => Ok(0),
        }
    }

    pub(super) fn ready_for(&self, ack: StructuralAck) -> bool {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean { target, .. }) => {
                recursive_green_session_matches_ack(target, ack)
            }
            Some(PendingRecursiveGreen::ReadyUpdate { update, .. }) => {
                recursive_green_session_matches_ack(update.target_session(), ack)
            }
            _ => false,
        }
    }

    pub(super) fn initial_clean_ready_session(
        &self,
        source: flark_engine::SourceVersion,
        syntax_profile: u32,
    ) -> Result<&M11PersistentRecursiveGreenSession, CandidateEndpointError> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean {
                target,
                base: None,
                origin: RecursiveGreenCleanOrigin::Initial,
            }) if target.source() == source && target.syntax_profile() == syntax_profile => {
                Ok(target)
            }
            Some(PendingRecursiveGreen::ReadyClean { .. }) => {
                Err(CandidateEndpointError::InvalidAuthority)
            }
            _ => Err(CandidateEndpointError::InvalidState),
        }
    }

    pub(super) fn incremental_clean_ready_session(
        &self,
        source: flark_engine::SourceVersion,
        syntax_profile: u32,
    ) -> Option<&M11PersistentRecursiveGreenSession> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean {
                target,
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                ..
            }) if target.source() == source && target.syntax_profile() == syntax_profile => {
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn ready_update_for(
        &self,
        base_ack: StructuralAck,
        target: flark_engine::SourceVersion,
    ) -> Option<&M11PersistentRecursiveGreenUpdate> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyUpdate {
                base_ack: ready_base,
                update,
            }) if *ready_base == base_ack && update.target_source() == target => Some(update),
            _ => None,
        }
    }

    pub(super) fn incremental_clean_ready_for_recursive_base(
        &self,
        base_ack: StructuralAck,
        target: flark_engine::SourceVersion,
        syntax_profile: u32,
    ) -> bool {
        matches!(
            self.pending.as_ref(),
            Some(PendingRecursiveGreen::ReadyClean {
                target: ready,
                base: Some(base),
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
            }) if base.ack == base_ack
                && ready.source() == target
                && ready.syntax_profile() == syntax_profile
        )
    }

    pub(super) fn owns_recursive_base_authority(&self, ack: StructuralAck) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.ack == ack)
            || match self.pending.as_ref() {
                Some(PendingRecursiveGreen::Adoption { base_ack, .. })
                | Some(PendingRecursiveGreen::CancellingAdoptionForFallback { base_ack, .. })
                | Some(PendingRecursiveGreen::ReadyUpdate { base_ack, .. }) => *base_ack == ack,
                Some(PendingRecursiveGreen::CleanPlan {
                    base: Some(base), ..
                })
                | Some(PendingRecursiveGreen::CleanBuild {
                    base: Some(base), ..
                })
                | Some(PendingRecursiveGreen::ReadyClean {
                    base: Some(base), ..
                }) => base.ack == ack,
                _ => false,
            }
    }

    pub(super) fn commit_delivery(
        &mut self,
        ack: StructuralAck,
    ) -> Result<(), CandidateEndpointError> {
        if !self.ready_for(ack) || self.installed.is_some() {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        self.cleanup
            .try_reserve(1)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        let ready = self
            .pending
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let (target, base, clean_origin) = match ready {
            PendingRecursiveGreen::ReadyClean {
                target,
                base,
                origin,
            } => (
                target,
                base.map(|installed| installed.session),
                Some(origin),
            ),
            PendingRecursiveGreen::ReadyUpdate {
                base_ack,
                mut update,
            } => {
                if base_ack.host_revision >= ack.host_revision {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                let Some(target) = update.take_target() else {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidState);
                };
                let Some(base) = update.take_base() else {
                    self.cleanup.push_back(RecursiveGreenCleanup::Session {
                        session: target,
                        begun: false,
                    });
                    return Err(CandidateEndpointError::InvalidState);
                };
                (target, Some(base), None)
            }
            other => {
                self.pending = Some(other);
                return Err(CandidateEndpointError::InvalidState);
            }
        };
        if let Some(session) = base {
            self.cleanup.push_back(RecursiveGreenCleanup::Session {
                session,
                begun: false,
            });
        }
        self.installed = Some(InstalledRecursiveGreen {
            ack,
            session: target,
        });
        match clean_origin {
            None => {
                self.path_receipt.local_adoption_deliveries = self
                    .path_receipt
                    .local_adoption_deliveries
                    .saturating_add(1);
            }
            Some(RecursiveGreenCleanOrigin::IncrementalFallback) => {
                self.path_receipt.clean_fallback_deliveries = self
                    .path_receipt
                    .clean_fallback_deliveries
                    .saturating_add(1);
            }
            Some(RecursiveGreenCleanOrigin::Initial) => {}
        }
        Ok(())
    }

    pub(super) const fn path_receipt(&self) -> RecursiveGreenPathReceipt {
        self.path_receipt
    }

    pub(super) fn installed_session(
        &self,
        ack: StructuralAck,
    ) -> Result<&M11PersistentRecursiveGreenSession, CandidateEndpointError> {
        let installed = self
            .installed
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?;
        if installed.ack != ack || !recursive_green_session_matches_ack(&installed.session, ack) {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        Ok(&installed.session)
    }

    pub(super) fn has_installed_session_for(&self, ack: StructuralAck) -> bool {
        self.installed.as_ref().is_some_and(|installed| {
            installed.ack == ack && recursive_green_session_matches_ack(&installed.session, ack)
        })
    }

    pub(super) fn request_cancel_pending(&mut self) -> Result<(), CandidateEndpointError> {
        if self.pending.is_none() {
            return Ok(());
        }
        self.cleanup
            .try_reserve(1)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        let pending = self
            .pending
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match pending {
            PendingRecursiveGreen::CleanPlan { base, .. } => {
                self.installed = base;
            }
            PendingRecursiveGreen::CleanBuild { build, base, .. } => {
                self.cleanup.push_back(RecursiveGreenCleanup::CleanBuild {
                    build,
                    restore: base,
                    begun: false,
                });
            }
            PendingRecursiveGreen::Adoption { base_ack, adoption } => {
                self.cleanup.push_back(RecursiveGreenCleanup::Adoption {
                    adoption,
                    restore_ack: base_ack,
                    begun: false,
                });
            }
            PendingRecursiveGreen::CancellingAdoptionForFallback {
                base_ack,
                adoption,
                begun,
                ..
            } => {
                self.cleanup.push_back(RecursiveGreenCleanup::Adoption {
                    adoption,
                    restore_ack: base_ack,
                    begun,
                });
            }
            PendingRecursiveGreen::ReadyClean { target, base, .. } => {
                self.installed = base;
                self.cleanup.push_back(RecursiveGreenCleanup::Session {
                    session: target,
                    begun: false,
                });
            }
            PendingRecursiveGreen::ReadyUpdate {
                base_ack,
                mut update,
            } => {
                let Some(target) = update.take_target() else {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidState);
                };
                let Some(base) = update.take_base() else {
                    self.cleanup.push_back(RecursiveGreenCleanup::Session {
                        session: target,
                        begun: false,
                    });
                    return Err(CandidateEndpointError::InvalidState);
                };
                self.installed = Some(InstalledRecursiveGreen {
                    ack: base_ack,
                    session: base,
                });
                self.cleanup.push_back(RecursiveGreenCleanup::Session {
                    session: target,
                    begun: false,
                });
            }
        }
        Ok(())
    }

    pub(super) fn begin_close(&mut self) -> Result<(), CandidateEndpointError> {
        self.request_cancel_pending()?;
        if self.installed.is_some() {
            self.cleanup
                .try_reserve(1)
                .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        }
        if let Some(installed) = self.installed.take() {
            self.cleanup.push_back(RecursiveGreenCleanup::Session {
                session: installed.session,
                begun: false,
            });
        }
        Ok(())
    }

    pub(super) fn poll_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        if fuel == 0 || self.cleanup.is_empty() {
            return Ok(0);
        }
        let cleanup = self
            .cleanup
            .front_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match cleanup {
            RecursiveGreenCleanup::Session { session, begun } => {
                let mut consumed = 0;
                if !*begun {
                    session.begin_release(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if session.poll_release(runtime, fuel - consumed)? {
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                }
                Ok(fuel)
            }
            RecursiveGreenCleanup::CleanBuild {
                build,
                restore,
                begun,
            } => {
                let mut consumed = 0;
                if !*begun {
                    build.begin_cancel(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if build.poll_cancel(runtime, fuel - consumed)?.status()
                    == M11PersistentRecursiveGreenBuildStatus::Cancelled
                {
                    if self.installed.is_some() && restore.is_some() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let restore = restore.take();
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                    if let Some(restore) = restore {
                        self.installed = Some(restore);
                    }
                }
                Ok(fuel)
            }
            RecursiveGreenCleanup::Adoption {
                adoption,
                restore_ack,
                begun,
            } => {
                let mut consumed = 0;
                if !*begun {
                    adoption.begin_cancel(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if adoption.poll_cancel(runtime, fuel - consumed)? {
                    if self.installed.is_some() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let base = adoption
                        .take_base_after_cancel()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let restore_ack = *restore_ack;
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                    self.installed = Some(InstalledRecursiveGreen {
                        ack: restore_ack,
                        session: base,
                    });
                }
                Ok(fuel)
            }
        }
    }
}

fn recursive_green_session_matches_ack(
    session: &M11PersistentRecursiveGreenSession,
    ack: StructuralAck,
) -> bool {
    let source = session.source();
    source.revision().get() == u64::from(ack.source_version.revision)
        && source.byte_len()
            == usize::try_from(ack.source_version.utf8_length).unwrap_or(usize::MAX)
        && source.utf16_len()
            == usize::try_from(ack.source_version.utf16_length).unwrap_or(usize::MAX)
        && split_u64(source.root().get()) == ack.source_root
        && session.syntax_profile() == ack.syntax_profile
}
