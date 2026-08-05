// Generated with:
// re2rust 4.3.1 --no-generation-date --storable-state --no-unsafe \
//   -o src/atx_cursor_generated.rs rules/atx_cursor.re
//
// Do not add Markdown syntax here. generate.py inserts the exact ATX pattern
// extracted from the pinned Comrak scanners.re source.

/// Immutable identity plus random/sequential byte access for one physical
/// line. Production adapters can cache a Crop cursor internally; the scanner
/// retains only absolute offsets and never owns source bytes.
pub trait AtxCursorSource {
    fn source_key(&self) -> u64;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn byte_at(&mut self, absolute_offset: usize) -> u8;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorScanResult {
    NeedMore,
    Matched(usize),
    NoMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorScanError {
    ZeroFuel,
    WrongSource,
    SourceContainsSentinel { absolute_offset: usize },
    PollAfterComplete,
    PollAfterFailure,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CursorScanReceipt {
    pub result: Option<CursorScanResult>,
    pub source_bytes_inspected: usize,
    pub maximum_lookahead_slack: usize,
    pub retained_source_bytes: usize,
}

/// Exact generated DFA state over a caller-owned physical-line cursor.
///
/// `poll` may inspect at most `fuel + LOOKAHEAD_SLACK` real source bytes. The
/// constant slack is generated as `YYMAXFILL - 1`: re2c groups bounds checks
/// around the longest non-looping DFA path. This is a hard constant, not a
/// line-length-dependent buffer.
#[derive(Clone, Debug)]
pub struct CursorAtxScanner {
    source_key: u64,
    source_len: usize,
    yycursor: usize,
    yymarker: usize,
    yystate: isize,
    poll_source_limit: usize,
    poll_source_bytes: usize,
    source_error: Option<CursorScanError>,
    terminal: Option<CursorScanResult>,
    failed: bool,
}

impl CursorAtxScanner {
    #[must_use]
    pub fn new(source_key: u64, source_len: usize) -> Self {
        Self {
            source_key,
            source_len,
            yycursor: 0,
            yymarker: 0,
            yystate: -1,
            poll_source_limit: 0,
            poll_source_bytes: 0,
            source_error: None,
            terminal: None,
            failed: false,
        }
    }

    pub fn poll<S: AtxCursorSource>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<CursorScanReceipt, CursorScanError> {
        if fuel == 0 {
            return Err(CursorScanError::ZeroFuel);
        }
        if self.failed {
            return Err(CursorScanError::PollAfterFailure);
        }
        if self.terminal.is_some() {
            return Err(CursorScanError::PollAfterComplete);
        }
        if source.source_key() != self.source_key || source.len() != self.source_len {
            self.failed = true;
            return Err(CursorScanError::WrongSource);
        }

        self.poll_source_bytes = 0;
        self.poll_source_limit = fuel.saturating_add(LOOKAHEAD_SLACK);
        self.source_error = None;
        let result = scan_cursor(self, source);
        if let Some(error) = self.source_error.take() {
            self.failed = true;
            return Err(error);
        }
        if result != CursorScanResult::NeedMore {
            self.terminal = Some(result);
        }
        Ok(CursorScanReceipt {
            result: Some(result),
            source_bytes_inspected: self.poll_source_bytes,
            maximum_lookahead_slack: LOOKAHEAD_SLACK,
            retained_source_bytes: 0,
        })
    }

    #[must_use]
    pub const fn cursor(&self) -> usize {
        if self.yycursor < self.source_len {
            self.yycursor
        } else {
            self.source_len
        }
    }

    #[must_use]
    pub const fn retained_source_bytes(&self) -> usize {
        0
    }

    fn less_than(&self, requested: usize) -> bool {
        let remaining_source = self.source_len.saturating_sub(self.yycursor);
        let real_source_needed = if requested < remaining_source {
            requested
        } else {
            remaining_source
        };
        self.poll_source_bytes.saturating_add(real_source_needed) > self.poll_source_limit
    }

    fn peek<S: AtxCursorSource>(&mut self, source: &mut S) -> u8 {
        if self.yycursor >= self.source_len {
            return 0xff;
        }
        let offset = self.yycursor;
        let byte = source.byte_at(offset);
        self.poll_source_bytes = self.poll_source_bytes.saturating_add(1);
        if byte == 0xff {
            self.source_error = Some(CursorScanError::SourceContainsSentinel {
                absolute_offset: offset,
            });
        }
        byte
    }
}

fn scan_cursor<S: AtxCursorSource>(
    scanner: &mut CursorAtxScanner,
    source: &mut S,
) -> CursorScanResult {
    let mut yych: u8;
/*!re2c
    re2c:api = generic;
    re2c:YYCTYPE = "u8";
    re2c:define:YYCURSOR = "scanner.yycursor";
    re2c:define:YYMARKER = "scanner.yymarker";
    re2c:define:YYLIMIT = "scanner.source_len";
    re2c:define:YYPEEK = "scanner.peek(source)";
    re2c:define:YYSKIP = "scanner.yycursor += 1;";
    re2c:define:YYBACKUP = "scanner.yymarker = scanner.yycursor;";
    re2c:define:YYRESTORE = "scanner.yycursor = scanner.yymarker;";
    re2c:define:YYLESSTHAN = "scanner.less_than(@@)";
    re2c:define:YYGETSTATE = "scanner.yystate";
    re2c:define:YYSETSTATE = "scanner.yystate = @@;";
    re2c:YYFILL = "return CursorScanResult::NeedMore;";

    // Inserted mechanically from pinned scanners.re::atx_heading_start.
    [#]{1,6} ([ \t]+|[\r\n\xff]) {
        return CursorScanResult::Matched(scanner.yycursor.min(scanner.source_len));
    }
    * { return CursorScanResult::NoMatch; }
*/}

/*!max:re2c*/

/// Maximum real-source accesses the generated DFA may perform beyond one
/// caller fuel grant because re2c groups checks around its longest fixed path.
pub const CURSOR_ATX_MAX_LOOKAHEAD_SLACK: usize = YYMAXFILL - 1;

/// Maximum unique prefix bytes the pinned ATX DFA can inspect before a
/// `NoMatch`. Long separator loops are accepting paths, so the fused line
/// scanner needs no source-proportional rejection cache.
pub const CURSOR_ATX_REJECTION_PREFIX_CAP: usize = YYMAXFILL;

const LOOKAHEAD_SLACK: usize = CURSOR_ATX_MAX_LOOKAHEAD_SLACK;
