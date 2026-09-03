//! Exact authority and canonical storage identity for the live References role.
//!
//! Reference storage is parser-authenticated and authority-bound. This module
//! owns only the identity, header, and capacity vocabulary shared by the live
//! reference journal, root, and resolver.

use std::fmt;

use crate::identity::RuntimeIdentity;
use crate::source::SourceVersion;
use crate::storage::{ArenaLimits, PageArena};

pub(crate) const REFERENCE_STORAGE_FORMAT_VERSION: u8 = 1;
pub(crate) const REFERENCE_CANONICAL_NODE_HEADER_BYTES: usize = 4;

/// Authority shared by one live reference journal and its immutable index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceAuthority {
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) journal: RuntimeIdentity,
    pub(crate) source: SourceVersion,
    pub(crate) syntax_profile: u32,
}

impl ReferenceAuthority {
    pub(crate) fn new(
        runtime: RuntimeIdentity,
        journal: RuntimeIdentity,
        source: SourceVersion,
        syntax_profile: u32,
    ) -> Result<Self, ReferenceAuthorityError> {
        if runtime == journal || syntax_profile == 0 {
            return Err(ReferenceAuthorityError::InvalidAuthority);
        }
        Ok(Self {
            runtime,
            journal,
            source,
            syntax_profile,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceReserve {
    pub(crate) nodes: usize,
    pub(crate) payload_bytes: usize,
}

#[derive(Debug)]
pub(crate) enum ReferenceAuthorityError {
    InvalidAuthority,
    CapacityPreflight,
    Corrupt(&'static str),
}

impl fmt::Display for ReferenceAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAuthority => formatter.write_str("invalid reference authority"),
            Self::CapacityPreflight => {
                formatter.write_str("reference storage exceeds remaining arena capacity")
            }
            Self::Corrupt(message) => write!(formatter, "corrupt reference storage: {message}"),
        }
    }
}

impl std::error::Error for ReferenceAuthorityError {}

pub(crate) fn encode_reference_node_header(tag: u8) -> Vec<u8> {
    vec![tag, REFERENCE_STORAGE_FORMAT_VERSION, 0, 0]
}

pub(crate) fn decode_reference_node_header(
    payload: &[u8],
    expected_tag: u8,
) -> Result<(), ReferenceAuthorityError> {
    if payload.len() < REFERENCE_CANONICAL_NODE_HEADER_BYTES
        || payload[..REFERENCE_CANONICAL_NODE_HEADER_BYTES]
            != [expected_tag, REFERENCE_STORAGE_FORMAT_VERSION, 0, 0]
    {
        return Err(ReferenceAuthorityError::Corrupt(
            "canonical reference node header changed",
        ));
    }
    Ok(())
}

pub(crate) fn preflight_reference_capacity(
    arena: &PageArena,
    limits: ArenaLimits,
    nodes: usize,
    payload_bytes: usize,
) -> Result<(), ReferenceAuthorityError> {
    let metrics = arena.metrics();
    if arena.limits() != limits
        || metrics
            .resident_nodes
            .checked_add(nodes)
            .is_none_or(|total| total > limits.max_slots)
        || metrics
            .live_payload_bytes
            .checked_add(metrics.reserved_external_payload_bytes)
            .and_then(|admitted| admitted.checked_add(payload_bytes))
            .is_none_or(|total| total > limits.max_live_payload_bytes)
    {
        return Err(ReferenceAuthorityError::CapacityPreflight);
    }
    Ok(())
}
