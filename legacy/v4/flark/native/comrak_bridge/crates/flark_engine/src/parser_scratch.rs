//! Exact admission for parser-local, fixed-size transient scratch.
//!
//! Mutable parser algorithms keep their bytes in `flark-parser`; the engine
//! retains only a move-only accounting capability. The charge shares the
//! document arena's payload budget, so transient scratch and persistent parser
//! roots cannot independently over-admit memory.

use std::fmt;

use crate::document::{DocumentRuntime, DocumentState};
use crate::identity::RuntimeIdentity;
use crate::source::SourceVersion;
use crate::storage::{
    ArenaError, ExternalPayloadReservation, ExternalPayloadReservationReleaseFailure,
};

/// Admission, authority, or lifecycle failure for parser-local scratch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ParserScratchError {
    ZeroBytes,
    InvalidSplit {
        prefix_bytes: usize,
        available_bytes: usize,
    },
    NotOpen {
        state: DocumentState,
    },
    SourceAuthorityMismatch,
    WrongRuntime,
    Arena(ArenaError),
}

impl M11ParserScratchError {
    #[must_use]
    pub const fn is_resource_limit(self) -> bool {
        matches!(
            self,
            Self::Arena(
                ArenaError::CapacityExceeded
                    | ArenaError::PayloadBudgetExceeded
                    | ArenaError::AllocationFailed
            )
        )
    }
}

impl fmt::Display for M11ParserScratchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBytes => formatter.write_str("parser scratch admission requires bytes"),
            Self::InvalidSplit {
                prefix_bytes,
                available_bytes,
            } => write!(
                formatter,
                "parser scratch split of {prefix_bytes} bytes is not a strict prefix of \
                 {available_bytes} admitted bytes"
            ),
            Self::NotOpen { state } => {
                write!(
                    formatter,
                    "parser scratch cannot be admitted while {state:?}"
                )
            }
            Self::SourceAuthorityMismatch => {
                formatter.write_str("parser scratch source does not match the current runtime")
            }
            Self::WrongRuntime => {
                formatter.write_str("parser scratch admission belongs to another runtime")
            }
            Self::Arena(error) => write!(formatter, "parser scratch admission failed: {error}"),
        }
    }
}

impl std::error::Error for M11ParserScratchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Arena(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ArenaError> for M11ParserScratchError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

/// Move-only exact charge paired with parser-owned fixed storage.
///
/// Release is allowed after source supersession and while the runtime is
/// closing. The recorded source remains immutable audit authority; only the
/// owning runtime may consume the charge.
#[must_use = "parser scratch admission must remain paired with bytes until explicit release"]
pub struct M11ParserScratchAdmission {
    runtime_identity: RuntimeIdentity,
    source: SourceVersion,
    reservation: Option<ExternalPayloadReservation>,
}

impl fmt::Debug for M11ParserScratchAdmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserScratchAdmission")
            .field("source", &self.source)
            .field("bytes", &self.bytes())
            .finish_non_exhaustive()
    }
}

impl M11ParserScratchAdmission {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.reservation
            .as_ref()
            .map_or(0, ExternalPayloadReservation::bytes)
    }

    /// Splits one strict, nonempty prefix into a second move-only capability.
    ///
    /// Aggregate arena accounting is unchanged. This lets one atomic radix
    /// allocation preflight its complete fixed bundle before distributing the
    /// exact charges among independently reclaimed pages and directories.
    pub fn split_prefix(&mut self, prefix_bytes: usize) -> Result<Self, M11ParserScratchError> {
        let available_bytes = self.bytes();
        let Some(prefix) = self
            .reservation
            .as_mut()
            .and_then(|reservation| reservation.split_prefix(prefix_bytes))
        else {
            return Err(M11ParserScratchError::InvalidSplit {
                prefix_bytes,
                available_bytes,
            });
        };
        Ok(Self {
            runtime_identity: self.runtime_identity,
            source: self.source,
            reservation: Some(prefix),
        })
    }
}

impl Drop for M11ParserScratchAdmission {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.reservation.is_none(),
                "parser scratch admission requires explicit release"
            );
        }
    }
}

/// Failed release that preserves the exact move-only admission capability.
pub struct M11ParserScratchReleaseFailure {
    error: M11ParserScratchError,
    admission: M11ParserScratchAdmission,
}

impl fmt::Debug for M11ParserScratchReleaseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ParserScratchReleaseFailure")
            .field("error", &self.error)
            .field("admission", &self.admission)
            .finish()
    }
}

impl M11ParserScratchReleaseFailure {
    #[must_use]
    pub const fn error(&self) -> M11ParserScratchError {
        self.error
    }

    #[must_use = "parser scratch admission must remain paired with bytes until explicit release"]
    pub fn into_admission(self) -> M11ParserScratchAdmission {
        self.admission
    }
}

impl DocumentRuntime {
    /// Admits fixed parser-local bytes for the exact current source.
    pub fn try_admit_parser_scratch(
        &mut self,
        source: SourceVersion,
        bytes: usize,
    ) -> Result<M11ParserScratchAdmission, M11ParserScratchError> {
        if bytes == 0 {
            return Err(M11ParserScratchError::ZeroBytes);
        }
        if self.state() != DocumentState::Open {
            return Err(M11ParserScratchError::NotOpen {
                state: self.state(),
            });
        }
        if self.current_source_version() != Some(source) {
            return Err(M11ParserScratchError::SourceAuthorityMismatch);
        }
        let runtime_identity = self.producer_identity();
        let reservation = self.producer_arena_mut().reserve_external_payload(bytes)?;
        Ok(M11ParserScratchAdmission {
            runtime_identity,
            source,
            reservation: Some(reservation),
        })
    }

    /// Releases an exact parser-local charge.
    ///
    /// A cross-runtime mistake returns the still-armed capability so the owner
    /// can retry against the correct actor. Current source/lifecycle are not
    /// revalidated because supersession and close must be able to reclaim old
    /// parser work.
    pub fn release_parser_scratch(
        &mut self,
        mut admission: M11ParserScratchAdmission,
    ) -> Result<(), M11ParserScratchReleaseFailure> {
        if admission.runtime_identity != self.producer_identity() {
            return Err(M11ParserScratchReleaseFailure {
                error: M11ParserScratchError::WrongRuntime,
                admission,
            });
        }
        let reservation = admission
            .reservation
            .take()
            .expect("armed admission retains one arena reservation");
        match self
            .producer_arena_mut()
            .release_external_payload(reservation)
        {
            Ok(()) => Ok(()),
            Err(ExternalPayloadReservationReleaseFailure { error, reservation }) => {
                admission.reservation = Some(reservation);
                Err(M11ParserScratchReleaseFailure {
                    error: M11ParserScratchError::Arena(error),
                    admission,
                })
            }
        }
    }
}
