//! Grammar-free, build-owned immutable byte blobs.
//!
//! Bytes arrive in source order into one preallocated page buffer. Full chunk
//! leaves are folded through the shared resumable balanced-sequence builder,
//! so arbitrarily large values never require one contiguous allocation and
//! every poll allocates at most one arena page. The resulting capability is
//! linear and remains inside the unpublished arena build until a typed parent
//! consumes its root owner.

use std::fmt;

use crate::arena::{
    ARENA_PAGE_BYTES, ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession,
    ArenaError, ArenaId, PageArena,
};
use crate::persistent_sequence::{
    ResumableSequenceProgress, ResumableStreamingSequenceBuilder, SequenceMutationReceipt,
    SequenceNodeKind, SequenceSpec, sequence_node,
};

const FORMAT_VERSION: u8 = 1;
const BLOB_CHUNK_TAG: u8 = 0xd1;
const BLOB_BRANCH_TAG: u8 = 0xd2;
const BLOB_CHUNK_HEADER_BYTES: usize = 24;
const BLOB_BRANCH_BYTES: usize = 32;
pub(crate) const PERSISTENT_BLOB_CHUNK_BYTES: usize = ARENA_PAGE_BYTES - BLOB_CHUNK_HEADER_BYTES;
const MAX_BLOB_HEIGHT: u16 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistentBlobError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
}

impl From<ArenaError> for PersistentBlobError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for PersistentBlobError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl fmt::Display for PersistentBlobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "persistent blob error: {self:?}")
    }
}

impl std::error::Error for PersistentBlobError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistentBlobSummary {
    pub(crate) bytes: u64,
    pub(crate) chunks: u64,
    pub(crate) height: u16,
}

#[derive(Debug)]
pub(crate) struct PersistentBlobSpec;

impl SequenceSpec for PersistentBlobSpec {
    type Summary = PersistentBlobSummary;
    type Error = PersistentBlobError;
    type BranchPayload = [u8; BLOB_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(BLOB_CHUNK_TAG) {
            return Ok(None);
        }
        let bytes = decode_chunk_payload(payload)?.len();
        Ok(Some(PersistentBlobSummary {
            bytes: u64::try_from(bytes)
                .map_err(|_| PersistentBlobError::Overflow("blob chunk bytes"))?,
            chunks: 1,
            height: 1,
        }))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(BLOB_BRANCH_TAG) {
            return Ok(None);
        }
        if payload.len() != BLOB_BRANCH_BYTES || payload[1] != FORMAT_VERSION {
            return Err(PersistentBlobError::Corrupt("invalid blob branch"));
        }
        let summary = PersistentBlobSummary {
            bytes: get_u64(payload, 8),
            chunks: get_u64(payload, 16),
            height: get_u16(payload, 24),
        };
        if summary.bytes == 0
            || summary.chunks < 2
            || !(2..=MAX_BLOB_HEIGHT).contains(&summary.height)
        {
            return Err(PersistentBlobError::Corrupt("invalid blob branch summary"));
        }
        Ok(Some(summary))
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        let mut payload = [0_u8; BLOB_BRANCH_BYTES];
        payload[0] = BLOB_BRANCH_TAG;
        payload[1] = FORMAT_VERSION;
        put_u64(&mut payload, 8, summary.bytes);
        put_u64(&mut payload, 16, summary.chunks);
        put_u16(&mut payload, 24, summary.height);
        payload
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        let bytes = left
            .bytes
            .checked_add(right.bytes)
            .ok_or(PersistentBlobError::Overflow("blob bytes"))?;
        let chunks = left
            .chunks
            .checked_add(right.chunks)
            .ok_or(PersistentBlobError::Overflow("blob chunks"))?;
        let height = left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(PersistentBlobError::Overflow("blob height"))?;
        if height > MAX_BLOB_HEIGHT {
            return Err(PersistentBlobError::Corrupt(
                "blob height exceeds its authenticated envelope",
            ));
        }
        Ok(PersistentBlobSummary {
            bytes,
            chunks,
            height,
        })
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.chunks
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        PersistentBlobError::Invalid(message)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PersistentBlobBuildReceipt {
    pub(crate) polls: u64,
    pub(crate) input_bytes_copied: u64,
    pub(crate) chunk_pages_allocated: u64,
    pub(crate) branch_pages_allocated: usize,
    pub(crate) maximum_input_bytes_copied_per_call: usize,
    pub(crate) maximum_pages_allocated_per_poll: usize,
    pub(crate) maximum_buffer_capacity: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistentBlobBuildProgress {
    ReadyForBytes,
    Pending,
    Complete,
}

#[derive(Debug)]
enum BlobBuildPhase {
    Accepting,
    AllocateChunk { finish_after: bool },
    PushChunk { finish_after: bool },
    FinishSequence,
    Complete,
    Taken,
    Failed,
}

/// Forward-only, allocation-granular immutable-blob builder.
#[derive(Debug)]
pub(crate) struct PersistentByteBlobBuilder {
    build: ArenaBuildId,
    buffer: Vec<u8>,
    sequence: ResumableStreamingSequenceBuilder<PersistentBlobSpec>,
    sequence_receipt: SequenceMutationReceipt,
    root: Option<ArenaBuildOwner>,
    bytes: u64,
    chunks: u64,
    height: u16,
    phase: BlobBuildPhase,
    receipt: PersistentBlobBuildReceipt,
}

impl PersistentByteBlobBuilder {
    pub(crate) fn try_new(build: ArenaBuildId) -> Result<Self, PersistentBlobError> {
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| PersistentBlobError::Invalid("blob page-buffer reservation failed"))?;
        buffer.resize(BLOB_CHUNK_HEADER_BYTES, 0);
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let sequence = ResumableStreamingSequenceBuilder::<PersistentBlobSpec>::try_new(
            &mut sequence_receipt,
        )?;
        let capacity = buffer.capacity();
        Ok(Self {
            build,
            buffer,
            sequence,
            sequence_receipt,
            root: None,
            bytes: 0,
            chunks: 0,
            height: 0,
            phase: BlobBuildPhase::Accepting,
            receipt: PersistentBlobBuildReceipt {
                maximum_buffer_capacity: capacity,
                ..PersistentBlobBuildReceipt::default()
            },
        })
    }

    pub(crate) const fn receipt(&self) -> PersistentBlobBuildReceipt {
        self.receipt
    }

    pub(crate) fn is_ready_for_bytes(&self) -> bool {
        matches!(self.phase, BlobBuildPhase::Accepting)
    }

    pub(crate) fn push_bytes(&mut self, input: &[u8]) -> Result<usize, PersistentBlobError> {
        if !matches!(self.phase, BlobBuildPhase::Accepting) {
            return Err(PersistentBlobError::Invalid(
                "blob builder is not ready for input",
            ));
        }
        let available =
            ARENA_PAGE_BYTES
                .checked_sub(self.buffer.len())
                .ok_or(PersistentBlobError::Corrupt(
                    "blob buffer exceeded one page",
                ))?;
        let copied = available.min(input.len());
        self.buffer.extend_from_slice(&input[..copied]);
        self.bytes = self
            .bytes
            .checked_add(
                u64::try_from(copied)
                    .map_err(|_| PersistentBlobError::Overflow("blob input bytes"))?,
            )
            .ok_or(PersistentBlobError::Overflow("blob input bytes"))?;
        self.receipt.input_bytes_copied = self
            .receipt
            .input_bytes_copied
            .checked_add(
                u64::try_from(copied)
                    .map_err(|_| PersistentBlobError::Overflow("blob receipt bytes"))?,
            )
            .ok_or(PersistentBlobError::Overflow("blob receipt bytes"))?;
        self.receipt.maximum_input_bytes_copied_per_call =
            self.receipt.maximum_input_bytes_copied_per_call.max(copied);
        if self.buffer.len() == ARENA_PAGE_BYTES {
            self.phase = BlobBuildPhase::AllocateChunk {
                finish_after: false,
            };
        }
        Ok(copied)
    }

    pub(crate) fn begin_finish(&mut self) -> Result<(), PersistentBlobError> {
        if !matches!(self.phase, BlobBuildPhase::Accepting) {
            return Err(PersistentBlobError::Invalid(
                "blob finish requires an idle input buffer",
            ));
        }
        if self.buffer.len() > BLOB_CHUNK_HEADER_BYTES {
            self.phase = BlobBuildPhase::AllocateChunk { finish_after: true };
        } else if self.chunks == 0 {
            self.phase = BlobBuildPhase::Complete;
        } else {
            self.sequence.begin_finish(&mut self.sequence_receipt)?;
            self.phase = BlobBuildPhase::FinishSequence;
        }
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<PersistentBlobBuildProgress, PersistentBlobError> {
        if session.id() != self.build {
            return Err(PersistentBlobError::Invalid(
                "blob builder crossed arena build authority",
            ));
        }
        if matches!(self.phase, BlobBuildPhase::Taken | BlobBuildPhase::Failed) {
            return Err(PersistentBlobError::Invalid(
                "blob builder is consumed or failed",
            ));
        }
        let direct_before = self.receipt.chunk_pages_allocated;
        let branches_before = self.sequence_receipt.branches_allocated;
        let phase = std::mem::replace(&mut self.phase, BlobBuildPhase::Failed);
        let result = self.poll_phase(session, phase);
        self.receipt.polls = self
            .receipt
            .polls
            .checked_add(1)
            .ok_or(PersistentBlobError::Overflow("blob polls"))?;
        let direct_delta = self
            .receipt
            .chunk_pages_allocated
            .checked_sub(direct_before)
            .ok_or(PersistentBlobError::Corrupt(
                "blob allocation receipt moved backwards",
            ))?;
        let branch_delta = self
            .sequence_receipt
            .branches_allocated
            .checked_sub(branches_before)
            .ok_or(PersistentBlobError::Corrupt(
                "blob sequence receipt moved backwards",
            ))?;
        let allocated = usize::try_from(direct_delta)
            .map_err(|_| PersistentBlobError::Overflow("blob page allocation delta"))?
            .checked_add(branch_delta)
            .ok_or(PersistentBlobError::Overflow("blob page allocation delta"))?;
        self.receipt.maximum_pages_allocated_per_poll =
            self.receipt.maximum_pages_allocated_per_poll.max(allocated);
        self.receipt.branch_pages_allocated = self.sequence_receipt.branches_allocated;
        if allocated > 1 {
            self.phase = BlobBuildPhase::Failed;
            return Err(PersistentBlobError::Corrupt(
                "one blob poll allocated more than one page",
            ));
        }
        match result {
            Ok((next, progress)) => {
                self.phase = next;
                Ok(progress)
            }
            Err(error) => {
                self.phase = BlobBuildPhase::Failed;
                Err(error)
            }
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        phase: BlobBuildPhase,
    ) -> Result<(BlobBuildPhase, PersistentBlobBuildProgress), PersistentBlobError> {
        match phase {
            BlobBuildPhase::Accepting => Ok((
                BlobBuildPhase::Accepting,
                PersistentBlobBuildProgress::ReadyForBytes,
            )),
            BlobBuildPhase::AllocateChunk { finish_after } => {
                let length = self
                    .buffer
                    .len()
                    .checked_sub(BLOB_CHUNK_HEADER_BYTES)
                    .ok_or(PersistentBlobError::Corrupt(
                        "blob chunk header disappeared",
                    ))?;
                if length == 0 || length > PERSISTENT_BLOB_CHUNK_BYTES {
                    return Err(PersistentBlobError::Corrupt(
                        "blob chunk length escaped its page",
                    ));
                }
                self.buffer[0] = BLOB_CHUNK_TAG;
                self.buffer[1] = FORMAT_VERSION;
                put_u32(
                    &mut self.buffer,
                    8,
                    u32::try_from(length)
                        .map_err(|_| PersistentBlobError::Overflow("blob chunk length"))?,
                );
                let (leaf, allocation) = session.allocate(&self.buffer, &[])?;
                if allocation.payload_bytes_copied > ARENA_PAGE_BYTES {
                    return Err(PersistentBlobError::Corrupt(
                        "blob leaf allocation exceeded one page",
                    ));
                }
                self.receipt.chunk_pages_allocated = self
                    .receipt
                    .chunk_pages_allocated
                    .checked_add(1)
                    .ok_or(PersistentBlobError::Overflow("blob chunk pages"))?;
                self.chunks = self
                    .chunks
                    .checked_add(1)
                    .ok_or(PersistentBlobError::Overflow("blob chunks"))?;
                self.buffer.clear();
                self.buffer.resize(BLOB_CHUNK_HEADER_BYTES, 0);
                self.sequence
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                Ok((
                    BlobBuildPhase::PushChunk { finish_after },
                    PersistentBlobBuildProgress::Pending,
                ))
            }
            BlobBuildPhase::PushChunk { finish_after } => {
                if self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    return Ok((
                        BlobBuildPhase::PushChunk { finish_after },
                        PersistentBlobBuildProgress::Pending,
                    ));
                }
                if finish_after {
                    self.sequence.begin_finish(&mut self.sequence_receipt)?;
                    Ok((
                        BlobBuildPhase::FinishSequence,
                        PersistentBlobBuildProgress::Pending,
                    ))
                } else {
                    Ok((
                        BlobBuildPhase::Accepting,
                        PersistentBlobBuildProgress::ReadyForBytes,
                    ))
                }
            }
            BlobBuildPhase::FinishSequence => {
                if self
                    .sequence
                    .poll_finish(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    return Ok((
                        BlobBuildPhase::FinishSequence,
                        PersistentBlobBuildProgress::Pending,
                    ));
                }
                let root = self.sequence.take_root()?;
                let summary =
                    sequence_node::<PersistentBlobSpec>(session.arena(), session.owner_id(&root)?)?
                        .0;
                if summary.bytes != self.bytes || summary.chunks != self.chunks {
                    return Err(PersistentBlobError::Corrupt(
                        "blob root summary disagrees with streamed input",
                    ));
                }
                self.height = summary.height;
                self.root = Some(root);
                Ok((
                    BlobBuildPhase::Complete,
                    PersistentBlobBuildProgress::Complete,
                ))
            }
            BlobBuildPhase::Complete => Ok((
                BlobBuildPhase::Complete,
                PersistentBlobBuildProgress::Complete,
            )),
            BlobBuildPhase::Taken | BlobBuildPhase::Failed => Err(PersistentBlobError::Invalid(
                "blob builder is consumed or failed",
            )),
        }
    }

    pub(crate) fn take_blob(&mut self) -> Result<PersistentByteBlob, PersistentBlobError> {
        if !matches!(self.phase, BlobBuildPhase::Complete) {
            return Err(PersistentBlobError::Invalid("blob is not complete"));
        }
        let root = self.root.take();
        if (self.bytes == 0) != root.is_none() || (self.chunks == 0) != root.is_none() {
            return Err(PersistentBlobError::Corrupt(
                "blob root disagrees with its empty state",
            ));
        }
        self.phase = BlobBuildPhase::Taken;
        Ok(PersistentByteBlob {
            build: self.build,
            root,
            bytes: self.bytes,
            chunks: self.chunks,
            height: self.height,
        })
    }
}

/// Linear ownership of an immutable byte sequence inside one arena build.
#[derive(Debug)]
#[must_use = "the blob root must be consumed by a typed persistent parent"]
pub(crate) struct PersistentByteBlob {
    build: ArenaBuildId,
    root: Option<ArenaBuildOwner>,
    bytes: u64,
    chunks: u64,
    height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistentByteBlobMetadata {
    pub(crate) root: Option<ArenaId>,
    pub(crate) bytes: u64,
    pub(crate) chunks: u64,
    pub(crate) height: u16,
}

impl PersistentByteBlob {
    pub(crate) fn metadata(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<PersistentByteBlobMetadata, PersistentBlobError> {
        if session.id() != self.build {
            return Err(PersistentBlobError::Invalid(
                "blob crossed arena build authority",
            ));
        }
        Ok(PersistentByteBlobMetadata {
            root: self
                .root
                .as_ref()
                .map(|root| session.owner_id(root))
                .transpose()?,
            bytes: self.bytes,
            chunks: self.chunks,
            height: self.height,
        })
    }

    pub(crate) fn into_owner(self) -> Option<ArenaBuildOwner> {
        self.root
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistentBlobChunk {
    page: ArenaId,
    pub(crate) absolute_start: u64,
    pub(crate) bytes: usize,
}

impl PersistentBlobChunk {
    pub(crate) fn bytes<'a>(self, arena: &'a PageArena) -> Result<&'a [u8], PersistentBlobError> {
        let bytes = decode_chunk_payload(arena.payload(self.page)?)?;
        if bytes.len() != self.bytes || arena.packed_child_count(self.page)? != 0 {
            return Err(PersistentBlobError::Corrupt(
                "blob chunk capability no longer matches its page",
            ));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlobReadFrame {
    right: ArenaId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlobReadPhase {
    Descend(ArenaId),
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistentBlobReadProgress {
    Pending,
    Chunk(PersistentBlobChunk),
    Complete,
}

/// One-node-per-poll in-order reader over an authenticated blob root.
#[derive(Debug)]
pub(crate) struct PersistentByteBlobReadCursor {
    metadata: PersistentByteBlobMetadata,
    frames: Vec<BlobReadFrame>,
    observed_bytes: u64,
    observed_chunks: u64,
    phase: BlobReadPhase,
}

impl PersistentByteBlobReadCursor {
    pub(crate) fn try_new(
        metadata: PersistentByteBlobMetadata,
    ) -> Result<Self, PersistentBlobError> {
        if metadata.root.is_none() {
            if metadata.bytes != 0 || metadata.chunks != 0 || metadata.height != 0 {
                return Err(PersistentBlobError::Corrupt(
                    "empty blob metadata is inconsistent",
                ));
            }
            return Ok(Self {
                metadata,
                frames: Vec::new(),
                observed_bytes: 0,
                observed_chunks: 0,
                phase: BlobReadPhase::Complete,
            });
        }
        if metadata.bytes == 0
            || metadata.chunks == 0
            || !(1..=MAX_BLOB_HEIGHT).contains(&metadata.height)
        {
            return Err(PersistentBlobError::Corrupt(
                "nonempty blob metadata is inconsistent",
            ));
        }
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(usize::from(metadata.height.saturating_sub(1)))
            .map_err(|_| PersistentBlobError::Invalid("blob cursor reservation failed"))?;
        Ok(Self {
            metadata,
            frames,
            observed_bytes: 0,
            observed_chunks: 0,
            phase: BlobReadPhase::Descend(metadata.root.expect("checked blob root")),
        })
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
    ) -> Result<PersistentBlobReadProgress, PersistentBlobError> {
        let phase = std::mem::replace(&mut self.phase, BlobReadPhase::Failed);
        let BlobReadPhase::Descend(node) = phase else {
            self.phase = phase;
            return match phase {
                BlobReadPhase::Complete => Ok(PersistentBlobReadProgress::Complete),
                BlobReadPhase::Failed => {
                    Err(PersistentBlobError::Invalid("blob reader is poisoned"))
                }
                BlobReadPhase::Descend(_) => unreachable!(),
            };
        };
        let (summary, kind) = sequence_node::<PersistentBlobSpec>(arena, node)?;
        match kind {
            SequenceNodeKind::Branch { left, right } => {
                if self.frames.len() >= self.frames.capacity() {
                    return Err(PersistentBlobError::Corrupt(
                        "blob tree exceeded preflighted reader height",
                    ));
                }
                self.frames.push(BlobReadFrame { right });
                self.phase = BlobReadPhase::Descend(left);
                Ok(PersistentBlobReadProgress::Pending)
            }
            SequenceNodeKind::Leaf => {
                let bytes = usize::try_from(summary.bytes)
                    .map_err(|_| PersistentBlobError::Overflow("blob chunk length"))?;
                if summary.chunks != 1 || summary.height != 1 {
                    return Err(PersistentBlobError::Corrupt(
                        "blob leaf summary is inconsistent",
                    ));
                }
                let chunk = PersistentBlobChunk {
                    page: node,
                    absolute_start: self.observed_bytes,
                    bytes,
                };
                self.observed_bytes = self
                    .observed_bytes
                    .checked_add(summary.bytes)
                    .ok_or(PersistentBlobError::Overflow("blob observed bytes"))?;
                self.observed_chunks = self
                    .observed_chunks
                    .checked_add(1)
                    .ok_or(PersistentBlobError::Overflow("blob observed chunks"))?;
                self.phase = if let Some(frame) = self.frames.pop() {
                    BlobReadPhase::Descend(frame.right)
                } else {
                    if self.observed_bytes != self.metadata.bytes
                        || self.observed_chunks != self.metadata.chunks
                    {
                        return Err(PersistentBlobError::Corrupt(
                            "blob traversal disagrees with its metadata",
                        ));
                    }
                    BlobReadPhase::Complete
                };
                Ok(PersistentBlobReadProgress::Chunk(chunk))
            }
        }
    }
}

fn decode_chunk_payload(payload: &[u8]) -> Result<&[u8], PersistentBlobError> {
    if payload.len() < BLOB_CHUNK_HEADER_BYTES
        || payload[0] != BLOB_CHUNK_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(PersistentBlobError::Corrupt("invalid blob chunk"));
    }
    let length = get_u32(payload, 8) as usize;
    if length == 0
        || length > PERSISTENT_BLOB_CHUNK_BYTES
        || payload.len() != BLOB_CHUNK_HEADER_BYTES + length
    {
        return Err(PersistentBlobError::Corrupt("invalid blob chunk length"));
    }
    Ok(&payload[BLOB_CHUNK_HEADER_BYTES..])
}

fn put_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(input: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(input[offset..offset + 2].try_into().expect("u16 field"))
}

fn get_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(input[offset..offset + 4].try_into().expect("u32 field"))
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(input[offset..offset + 8].try_into().expect("u64 field"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageArena;

    #[test]
    fn streamed_multi_page_blob_round_trips_in_order_with_one_allocation_per_poll() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut ticket = Some(ticket);
        let mut builder = PersistentByteBlobBuilder::try_new(build).unwrap();
        let input = (0..(PERSISTENT_BLOB_CHUNK_BYTES * 5 + 37))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let mut offset = 0;
        let mut finishing = false;
        loop {
            if !finishing && offset < input.len() {
                if let Ok(copied) = builder.push_bytes(&input[offset..]) {
                    offset += copied;
                }
            } else if !finishing && builder.begin_finish().is_ok() {
                finishing = true;
            }
            let current = ticket.take().unwrap();
            let mut session = arena.resume_build(current).unwrap();
            let progress = builder.poll(&mut session).unwrap();
            ticket = Some(session.suspend().unwrap());
            if finishing && progress == PersistentBlobBuildProgress::Complete {
                break;
            }
        }
        let blob = builder.take_blob().unwrap();
        let current = ticket.take().unwrap();
        let mut session = arena.resume_build(current).unwrap();
        let metadata = blob.metadata(&session).unwrap();
        let mut cursor = PersistentByteBlobReadCursor::try_new(metadata).unwrap();
        let mut output = Vec::new();
        loop {
            match cursor.poll(session.arena()).unwrap() {
                PersistentBlobReadProgress::Pending => {}
                PersistentBlobReadProgress::Chunk(chunk) => {
                    output.extend_from_slice(chunk.bytes(session.arena()).unwrap());
                }
                PersistentBlobReadProgress::Complete => break,
            }
        }
        assert_eq!(output, input);
        assert!(builder.receipt().maximum_pages_allocated_per_poll <= 1);
        let owner = blob.into_owner().unwrap();
        session.release(owner).unwrap();
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        while arena.poll_reclaim(1).unwrap().pending_after != 0 {}
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn empty_blob_is_explicit_and_owns_no_arena_page() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut builder = PersistentByteBlobBuilder::try_new(build).unwrap();
        builder.begin_finish().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            builder.poll(&mut session).unwrap(),
            PersistentBlobBuildProgress::Complete
        );
        let blob = builder.take_blob().unwrap();
        let metadata = blob.metadata(&session).unwrap();
        assert_eq!(metadata.root, None);
        assert_eq!(metadata.bytes, 0);
        assert!(blob.into_owner().is_none());
        let abort = session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(abort, 0).unwrap().complete);
    }
}
