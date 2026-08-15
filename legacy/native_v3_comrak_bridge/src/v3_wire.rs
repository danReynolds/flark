//! Strict, allocation-neutral framing shared by native and Wasm v3 endpoints.
//!
//! Payload schemas live beside their owning session operation. This module
//! owns only the transport envelope, so both endpoint implementations reject
//! malformed buffers in exactly the same order before any document state is
//! touched.

use std::fmt;

pub const HEADER_BYTES: usize = 24;
pub const ABI_MAJOR: u16 = 1;
pub const ABI_MINOR: u16 = 1;
pub const MAXIMUM_PAYLOAD_BYTES: usize = 256 * 1024;
pub const ALLOWED_FLAGS: u32 = 0;

const MAGIC: [u8; 4] = *b"FLK3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Request,
    Response,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Opcode {
    Close = 0x0001,
    Drain = 0x0002,
    HostOpen = 0x0100,
    ObserveSource = 0x0101,
    PublishBegin = 0x0110,
    PublishPacket = 0x0111,
    PublishCommit = 0x0112,
    PublishAbort = 0x0113,
    HostPoll = 0x0120,
    Query = 0x0130,
    AcknowledgeDelivery = 0x0140,
    ParserOpen = 0x0200,
    SnapshotPage = 0x0201,
    Edit = 0x0202,
    ParserRefineInline = 0x0203,
    ParserPresentViewport = 0x0204,
    ParserPoll = 0x0210,
    Supersede = 0x0211,
    ParserAcknowledge = 0x0220,
}

impl Opcode {
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            0x0001 => Self::Close,
            0x0002 => Self::Drain,
            0x0100 => Self::HostOpen,
            0x0101 => Self::ObserveSource,
            0x0110 => Self::PublishBegin,
            0x0111 => Self::PublishPacket,
            0x0112 => Self::PublishCommit,
            0x0113 => Self::PublishAbort,
            0x0120 => Self::HostPoll,
            0x0130 => Self::Query,
            0x0140 => Self::AcknowledgeDelivery,
            0x0200 => Self::ParserOpen,
            0x0201 => Self::SnapshotPage,
            0x0202 => Self::Edit,
            0x0203 => Self::ParserRefineInline,
            0x0204 => Self::ParserPresentViewport,
            0x0210 => Self::ParserPoll,
            0x0211 => Self::Supersede,
            0x0220 => Self::ParserAcknowledge,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Status {
    Ok = 0x0000,
    UnsupportedMajor = 0x0010,
    UnsupportedMinor = 0x0011,
    MalformedFrame = 0x0012,
    UnknownOpcode = 0x0013,
    ForbiddenFlags = 0x0014,
    Invalid = 0x0100,
    InvalidState = 0x0101,
    Backpressure = 0x0102,
    StaleSource = 0x0103,
    ExactSourceMismatch = 0x0104,
    SessionSnapshotRequired = 0x0105,
    BaseMismatch = 0x0106,
    WrongOffer = 0x0107,
    CorruptPayload = 0x0108,
    QueryBoundExceeded = 0x0109,
    ForegroundBoundExceeded = 0x010a,
    Superseded = 0x010b,
    Closed = 0x010c,
    NotReady = 0x010d,
    UnsupportedSchema = 0x010e,
    AllocationFailed = 0x010f,
    InvalidUtf8 = 0x0110,
    InternalFault = 0x0111,
}

impl Status {
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            0x0000 => Self::Ok,
            0x0010 => Self::UnsupportedMajor,
            0x0011 => Self::UnsupportedMinor,
            0x0012 => Self::MalformedFrame,
            0x0013 => Self::UnknownOpcode,
            0x0014 => Self::ForbiddenFlags,
            0x0100 => Self::Invalid,
            0x0101 => Self::InvalidState,
            0x0102 => Self::Backpressure,
            0x0103 => Self::StaleSource,
            0x0104 => Self::ExactSourceMismatch,
            0x0105 => Self::SessionSnapshotRequired,
            0x0106 => Self::BaseMismatch,
            0x0107 => Self::WrongOffer,
            0x0108 => Self::CorruptPayload,
            0x0109 => Self::QueryBoundExceeded,
            0x010a => Self::ForegroundBoundExceeded,
            0x010b => Self::Superseded,
            0x010c => Self::Closed,
            0x010d => Self::NotReady,
            0x010e => Self::UnsupportedSchema,
            0x010f => Self::AllocationFailed,
            0x0110 => Self::InvalidUtf8,
            0x0111 => Self::InternalFault,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub opcode: Opcode,
    pub status: Status,
    pub flags: u32,
    pub correlation_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedFrame<'buffer> {
    pub header: Header,
    pub payload: &'buffer [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    pub supported_minor: u16,
    pub maximum_payload_bytes: usize,
    pub allowed_flags: u32,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            supported_minor: ABI_MINOR,
            maximum_payload_bytes: MAXIMUM_PAYLOAD_BYTES,
            allowed_flags: ALLOWED_FLAGS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    InvalidMaximumPayload,
    TruncatedHeader,
    InvalidMagic,
    UnsupportedMajor,
    UnsupportedMinor,
    UnknownOpcode,
    UnknownStatus,
    RequestStatusNotZero,
    ForbiddenFlags,
    PayloadTooLarge,
    TruncatedPayload,
    TrailingBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub failure: DecodeFailure,
    pub byte_offset: usize,
    pub expected: Option<usize>,
    pub actual: Option<usize>,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Flark v3 frame: {:?}", self.failure)
    }
}

impl std::error::Error for DecodeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    ForbiddenFlags,
    PayloadTooLarge,
    RequestStatusNotZero,
    BufferTooSmall { required: usize, available: usize },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot encode Flark v3 frame: {self:?}")
    }
}

impl std::error::Error for EncodeError {}

/// Decodes one exact, already-owned transport buffer without copying payload.
pub fn decode(
    bytes: &[u8],
    kind: FrameKind,
    limits: DecodeLimits,
) -> Result<DecodedFrame<'_>, DecodeError> {
    if limits.maximum_payload_bytes > MAXIMUM_PAYLOAD_BYTES {
        return Err(error(
            DecodeFailure::InvalidMaximumPayload,
            0,
            Some(MAXIMUM_PAYLOAD_BYTES),
            Some(limits.maximum_payload_bytes),
        ));
    }
    if bytes.len() < HEADER_BYTES {
        return Err(error(
            DecodeFailure::TruncatedHeader,
            bytes.len(),
            Some(HEADER_BYTES),
            Some(bytes.len()),
        ));
    }
    for (index, expected) in MAGIC.iter().copied().enumerate() {
        let actual = bytes[index];
        if actual != expected {
            return Err(error(
                DecodeFailure::InvalidMagic,
                index,
                Some(usize::from(expected)),
                Some(usize::from(actual)),
            ));
        }
    }

    let major = read_u16(bytes, 4);
    if major != ABI_MAJOR {
        return Err(error(
            DecodeFailure::UnsupportedMajor,
            4,
            Some(usize::from(ABI_MAJOR)),
            Some(usize::from(major)),
        ));
    }
    let minor = read_u16(bytes, 6);
    if minor > limits.supported_minor {
        return Err(error(
            DecodeFailure::UnsupportedMinor,
            6,
            Some(usize::from(limits.supported_minor)),
            Some(usize::from(minor)),
        ));
    }

    let opcode_code = read_u16(bytes, 8);
    let opcode = Opcode::from_code(opcode_code).ok_or_else(|| {
        error(
            DecodeFailure::UnknownOpcode,
            8,
            None,
            Some(usize::from(opcode_code)),
        )
    })?;
    let status_code = read_u16(bytes, 10);
    let status = Status::from_code(status_code).ok_or_else(|| {
        error(
            DecodeFailure::UnknownStatus,
            10,
            None,
            Some(usize::from(status_code)),
        )
    })?;
    if kind == FrameKind::Request && status != Status::Ok {
        return Err(error(
            DecodeFailure::RequestStatusNotZero,
            10,
            Some(usize::from(Status::Ok.code())),
            Some(usize::from(status_code)),
        ));
    }

    let flags = read_u32(bytes, 12);
    if flags & !limits.allowed_flags != 0 {
        return Err(error(
            DecodeFailure::ForbiddenFlags,
            12,
            Some(limits.allowed_flags as usize),
            Some(flags as usize),
        ));
    }
    let correlation_id = read_u32(bytes, 16);
    let payload_length = read_u32(bytes, 20) as usize;
    if payload_length > limits.maximum_payload_bytes {
        return Err(error(
            DecodeFailure::PayloadTooLarge,
            20,
            Some(limits.maximum_payload_bytes),
            Some(payload_length),
        ));
    }
    let expected_length = HEADER_BYTES + payload_length;
    if bytes.len() < expected_length {
        return Err(error(
            DecodeFailure::TruncatedPayload,
            bytes.len(),
            Some(expected_length),
            Some(bytes.len()),
        ));
    }
    if bytes.len() > expected_length {
        return Err(error(
            DecodeFailure::TrailingBytes,
            expected_length,
            Some(expected_length),
            Some(bytes.len()),
        ));
    }

    Ok(DecodedFrame {
        header: Header {
            opcode,
            status,
            flags,
            correlation_id,
        },
        payload: &bytes[HEADER_BYTES..expected_length],
    })
}

/// Encodes one frame into caller-owned storage and returns the written length.
pub fn encode_into(
    kind: FrameKind,
    header: Header,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    if header.flags & !ALLOWED_FLAGS != 0 {
        return Err(EncodeError::ForbiddenFlags);
    }
    if payload.len() > MAXIMUM_PAYLOAD_BYTES {
        return Err(EncodeError::PayloadTooLarge);
    }
    if kind == FrameKind::Request && header.status != Status::Ok {
        return Err(EncodeError::RequestStatusNotZero);
    }
    let required = HEADER_BYTES + payload.len();
    if output.len() < required {
        return Err(EncodeError::BufferTooSmall {
            required,
            available: output.len(),
        });
    }

    output[..4].copy_from_slice(&MAGIC);
    write_u16(output, 4, ABI_MAJOR);
    write_u16(output, 6, ABI_MINOR);
    write_u16(output, 8, header.opcode.code());
    write_u16(output, 10, header.status.code());
    write_u32(output, 12, header.flags);
    write_u32(output, 16, header.correlation_id);
    write_u32(output, 20, payload.len() as u32);
    output[HEADER_BYTES..required].copy_from_slice(payload);
    Ok(required)
}

const fn error(
    failure: DecodeFailure,
    byte_offset: usize,
    expected: Option<usize>,
    actual: Option<usize>,
) -> DecodeError {
    DecodeError {
        failure,
        byte_offset,
        expected,
        actual,
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            opcode: Opcode::HostOpen,
            status: Status::Ok,
            flags: 0,
            correlation_id: 0x0102_0304,
        }
    }

    fn encoded(payload: &[u8]) -> Vec<u8> {
        let mut output = vec![0; HEADER_BYTES + payload.len()];
        let written = encode_into(FrameKind::Request, header(), payload, &mut output).unwrap();
        assert_eq!(written, output.len());
        output
    }

    #[test]
    fn envelope_matches_the_dart_little_endian_golden() {
        let bytes = encoded(&[0xaa, 0xbb, 0xcc]);
        assert_eq!(
            bytes,
            vec![
                0x46, 0x4c, 0x4b, 0x33, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x04, 0x03, 0x02, 0x01, 0x03, 0x00, 0x00, 0x00, 0xaa, 0xbb, 0xcc,
            ]
        );

        let decoded = decode(&bytes, FrameKind::Request, DecodeLimits::default()).unwrap();
        assert_eq!(decoded.header, header());
        assert_eq!(decoded.payload, [0xaa, 0xbb, 0xcc]);
        assert_eq!(decoded.payload.as_ptr(), bytes[HEADER_BYTES..].as_ptr());
    }

    #[test]
    fn every_declared_opcode_and_status_round_trips() {
        let opcodes = [
            Opcode::Close,
            Opcode::Drain,
            Opcode::HostOpen,
            Opcode::ObserveSource,
            Opcode::PublishBegin,
            Opcode::PublishPacket,
            Opcode::PublishCommit,
            Opcode::PublishAbort,
            Opcode::HostPoll,
            Opcode::Query,
            Opcode::AcknowledgeDelivery,
            Opcode::ParserOpen,
            Opcode::SnapshotPage,
            Opcode::Edit,
            Opcode::ParserRefineInline,
            Opcode::ParserPresentViewport,
            Opcode::ParserPoll,
            Opcode::Supersede,
            Opcode::ParserAcknowledge,
        ];
        for opcode in opcodes {
            assert_eq!(Opcode::from_code(opcode.code()), Some(opcode));
        }
        let statuses = [
            Status::Ok,
            Status::UnsupportedMajor,
            Status::UnsupportedMinor,
            Status::MalformedFrame,
            Status::UnknownOpcode,
            Status::ForbiddenFlags,
            Status::Invalid,
            Status::InvalidState,
            Status::Backpressure,
            Status::StaleSource,
            Status::ExactSourceMismatch,
            Status::SessionSnapshotRequired,
            Status::BaseMismatch,
            Status::WrongOffer,
            Status::CorruptPayload,
            Status::QueryBoundExceeded,
            Status::ForegroundBoundExceeded,
            Status::Superseded,
            Status::Closed,
            Status::NotReady,
            Status::UnsupportedSchema,
            Status::AllocationFailed,
            Status::InvalidUtf8,
            Status::InternalFault,
        ];
        for status in statuses {
            assert_eq!(Status::from_code(status.code()), Some(status));
        }
    }

    #[test]
    fn decoder_rejects_unknown_and_non_exact_envelopes() {
        let good = encoded(&[1, 2, 3]);

        let mut unknown_opcode = good.clone();
        unknown_opcode[8..10].copy_from_slice(&0xffff_u16.to_le_bytes());
        assert_eq!(
            decode(&unknown_opcode, FrameKind::Request, DecodeLimits::default())
                .unwrap_err()
                .failure,
            DecodeFailure::UnknownOpcode
        );

        assert_eq!(
            decode(
                &good[..good.len() - 1],
                FrameKind::Request,
                DecodeLimits::default()
            )
            .unwrap_err()
            .failure,
            DecodeFailure::TruncatedPayload
        );

        let mut trailing = good;
        trailing.push(0);
        assert_eq!(
            decode(&trailing, FrameKind::Request, DecodeLimits::default())
                .unwrap_err()
                .failure,
            DecodeFailure::TrailingBytes
        );
    }

    #[test]
    fn request_status_and_flags_fail_closed() {
        let mut bytes = encoded(&[]);
        bytes[10..12].copy_from_slice(&Status::Invalid.code().to_le_bytes());
        assert_eq!(
            decode(&bytes, FrameKind::Request, DecodeLimits::default())
                .unwrap_err()
                .failure,
            DecodeFailure::RequestStatusNotZero
        );

        bytes[10..12].copy_from_slice(&Status::Ok.code().to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode(&bytes, FrameKind::Request, DecodeLimits::default())
                .unwrap_err()
                .failure,
            DecodeFailure::ForbiddenFlags
        );
    }

    #[test]
    fn caller_owned_encoding_is_bounded() {
        let payload = vec![0x5a; MAXIMUM_PAYLOAD_BYTES];
        let mut exact = vec![0; HEADER_BYTES + payload.len()];
        assert_eq!(
            encode_into(FrameKind::Request, header(), &payload, &mut exact).unwrap(),
            exact.len()
        );

        let mut short = vec![0; exact.len() - 1];
        assert_eq!(
            encode_into(FrameKind::Request, header(), &payload, &mut short),
            Err(EncodeError::BufferTooSmall {
                required: exact.len(),
                available: short.len(),
            })
        );

        let too_large = vec![0; MAXIMUM_PAYLOAD_BYTES + 1];
        assert_eq!(
            encode_into(FrameKind::Request, header(), &too_large, &mut []),
            Err(EncodeError::PayloadTooLarge)
        );
    }
}
