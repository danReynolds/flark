//! Parser-owned admission for the bounded D0 multi-surface transition.
//!
//! This module intentionally recognizes only the frozen D0 fenced-code
//! journey.  It does not publish block semantics itself: the runtime asks the
//! ordinary parser for every declared counterfactual result before exposing a
//! plan to the host.

/// Maximum number of bytes in one bounded pending-presentation sequence.
pub const M11_PENDING_PRESENTATION_SEQUENCE_MAX_BYTES: usize = 8;

/// Parser-authored seed for a bounded sequence of counterfactual parses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PendingPresentationPlanSeed {
    sequence: &'static [u8],
    trigger_byte: usize,
    trigger_utf16: usize,
    replaced_row_count: u8,
}

impl M11PendingPresentationPlanSeed {
    #[must_use]
    pub const fn sequence(self) -> &'static [u8] {
        self.sequence
    }

    #[must_use]
    pub const fn trigger_byte(self) -> usize {
        self.trigger_byte
    }

    #[must_use]
    pub const fn trigger_utf16(self) -> usize {
        self.trigger_utf16
    }

    #[must_use]
    pub const fn replaced_row_count(self) -> u8 {
        self.replaced_row_count
    }
}

const D0_BODY: &[u8] = b"change this line\n\n**sentinel**\n";
const D0_CLOSING_BASE: &[u8] = b"```dart\nchange this line\n\n**sentinel**\n";

/// Selects one exact D0 seed. Every other source fails closed.
#[must_use]
pub fn derive_m11_pending_presentation_plan_seed(
    source: &[u8],
) -> Option<M11PendingPresentationPlanSeed> {
    let seed = if source == D0_BODY {
        M11PendingPresentationPlanSeed {
            sequence: b"```dart\n",
            trigger_byte: 0,
            trigger_utf16: 0,
            replaced_row_count: 2,
        }
    } else if source == D0_CLOSING_BASE {
        M11PendingPresentationPlanSeed {
            sequence: b"\n```",
            trigger_byte: 24,
            trigger_utf16: 24,
            replaced_row_count: 1,
        }
    } else {
        return None;
    };
    debug_assert!(!seed.sequence.is_empty());
    debug_assert!(seed.sequence.len() <= M11_PENDING_PRESENTATION_SEQUENCE_MAX_BYTES);
    Some(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_fence_journey_has_two_exact_fail_closed_seeds() {
        let opening = derive_m11_pending_presentation_plan_seed(D0_BODY).expect("opening seed");
        assert_eq!(opening.sequence(), b"```dart\n");
        assert_eq!(opening.trigger_byte(), 0);
        assert_eq!(opening.trigger_utf16(), 0);
        assert_eq!(opening.replaced_row_count(), 2);

        let closing =
            derive_m11_pending_presentation_plan_seed(D0_CLOSING_BASE).expect("closing seed");
        assert_eq!(closing.sequence(), b"\n```");
        assert_eq!(closing.trigger_byte(), 24);
        assert_eq!(closing.trigger_utf16(), 24);
        assert_eq!(closing.replaced_row_count(), 1);

        assert!(derive_m11_pending_presentation_plan_seed(b"").is_none());
        assert!(derive_m11_pending_presentation_plan_seed(b"change this line\n").is_none());
        assert!(derive_m11_pending_presentation_plan_seed(
            b"prefix\nchange this line\n\n**sentinel**\n"
        )
        .is_none());
        assert!(derive_m11_pending_presentation_plan_seed(
            b"```dart\nchange this line\n**sentinel**\n"
        )
        .is_none());
    }
}
