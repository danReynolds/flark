use std::num::NonZeroU64;

/// Stable, nonzero identity for the configured markdown grammar/syntax policy.
///
/// Binding this identity to parser-produced captures prevents syntax facts
/// created under one profile from being consumed under a different profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParserProfileId(NonZeroU64);

impl ParserProfileId {
    /// Creates a parser profile identity. Zero is reserved as "unbound".
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}
