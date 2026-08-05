// Generated with:
// re2rust 4.3.1 --no-generation-date --storable-state --no-unsafe \
//   -o src/open_code_fence_cursor_generated.rs rules/open_code_fence_cursor.re
//
// Do not add Markdown syntax here. generate.py inserts the exact two
// open_code_fence patterns extracted from pinned Comrak scanners.re.

/// Immutable identity plus byte access for one physical line. The generated
/// scanner owns only scalar cursor/DFA state and never retains source bytes.
pub trait OpenCodeFenceCursorSource {
    fn source_key(&self) -> u64;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn byte_at(&mut self, absolute_offset: usize) -> u8;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenCodeFenceScanResult {
    NeedMore,
    Matched(usize),
    NoMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenCodeFenceScanError {
    ZeroFuel,
    WrongSource,
    SourceContainsSentinel { absolute_offset: usize },
    PollAfterComplete,
    PollAfterFailure,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenCodeFenceScanReceipt {
    pub result: Option<OpenCodeFenceScanResult>,
    /// Real-source byte requests made by the DFA this poll. Repeated peeks of
    /// one offset are counted because an adapter must serve each request.
    pub source_byte_requests: usize,
    pub maximum_lookahead_slack: usize,
    /// Largest logical `YYRESTORE` or trailing-context `YYRESTORECTX` in this
    /// poll. This can be much larger than actual source-access rewind.
    pub maximum_logical_rewind_bytes: usize,
    /// Largest backwards jump between consecutive real-source byte requests.
    pub maximum_source_request_rewind_bytes: usize,
    pub retained_source_bytes: usize,
}

/// Generated resumable DFA for pinned Comrak's `open_code_fence` rules.
///
/// Trailing context makes this materially different from the ATX proof: the
/// DFA may scan to line end and then restore its logical cursor to the fence
/// run. The receipt separately measures that logical rewind and any actual
/// backwards source request, so a strict forward-only adapter can prove the
/// cache it really needs.
#[derive(Clone, Debug)]
pub struct OpenCodeFenceCursorScanner {
    source_key: u64,
    source_len: usize,
    yycursor: usize,
    yymarker: usize,
    yyctxmarker: usize,
    yystate: isize,
    poll_source_limit: usize,
    poll_source_byte_requests: usize,
    poll_maximum_logical_rewind: usize,
    poll_maximum_source_request_rewind: usize,
    maximum_logical_rewind: usize,
    maximum_source_request_rewind: usize,
    last_source_request: Option<usize>,
    source_high_water: usize,
    source_error: Option<OpenCodeFenceScanError>,
    terminal: Option<OpenCodeFenceScanResult>,
    failed: bool,
}

impl OpenCodeFenceCursorScanner {
    #[must_use]
    pub fn new(source_key: u64, source_len: usize) -> Self {
        Self {
            source_key,
            source_len,
            yycursor: 0,
            yymarker: 0,
            yyctxmarker: 0,
            yystate: -1,
            poll_source_limit: 0,
            poll_source_byte_requests: 0,
            poll_maximum_logical_rewind: 0,
            poll_maximum_source_request_rewind: 0,
            maximum_logical_rewind: 0,
            maximum_source_request_rewind: 0,
            last_source_request: None,
            source_high_water: 0,
            source_error: None,
            terminal: None,
            failed: false,
        }
    }

    pub fn poll<S: OpenCodeFenceCursorSource>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<OpenCodeFenceScanReceipt, OpenCodeFenceScanError> {
        if fuel == 0 {
            return Err(OpenCodeFenceScanError::ZeroFuel);
        }
        if self.failed {
            return Err(OpenCodeFenceScanError::PollAfterFailure);
        }
        if self.terminal.is_some() {
            return Err(OpenCodeFenceScanError::PollAfterComplete);
        }
        if source.source_key() != self.source_key || source.len() != self.source_len {
            self.failed = true;
            return Err(OpenCodeFenceScanError::WrongSource);
        }

        self.poll_source_byte_requests = 0;
        self.poll_source_limit = fuel.saturating_add(LOOKAHEAD_SLACK);
        self.poll_maximum_logical_rewind = 0;
        self.poll_maximum_source_request_rewind = 0;
        self.source_error = None;
        let result = scan_open_code_fence_cursor(self, source);
        if let Some(error) = self.source_error.take() {
            self.failed = true;
            return Err(error);
        }
        if result != OpenCodeFenceScanResult::NeedMore {
            self.terminal = Some(result);
        }
        Ok(OpenCodeFenceScanReceipt {
            result: Some(result),
            source_byte_requests: self.poll_source_byte_requests,
            maximum_lookahead_slack: LOOKAHEAD_SLACK,
            maximum_logical_rewind_bytes: self.poll_maximum_logical_rewind,
            maximum_source_request_rewind_bytes: self
                .poll_maximum_source_request_rewind,
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
    pub const fn source_high_water(&self) -> usize {
        self.source_high_water
    }

    #[must_use]
    pub const fn maximum_logical_rewind_bytes(&self) -> usize {
        self.maximum_logical_rewind
    }

    #[must_use]
    pub const fn maximum_source_request_rewind_bytes(&self) -> usize {
        self.maximum_source_request_rewind
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
        self.poll_source_byte_requests
            .saturating_add(real_source_needed)
            > self.poll_source_limit
    }

    fn peek<S: OpenCodeFenceCursorSource>(&mut self, source: &mut S) -> u8 {
        if self.yycursor >= self.source_len {
            return 0xff;
        }
        let offset = self.yycursor;
        if let Some(previous) = self.last_source_request {
            let rewind = previous.saturating_sub(offset);
            self.poll_maximum_source_request_rewind =
                self.poll_maximum_source_request_rewind.max(rewind);
            self.maximum_source_request_rewind =
                self.maximum_source_request_rewind.max(rewind);
        }
        self.last_source_request = Some(offset);
        self.source_high_water = self.source_high_water.max(offset + 1);
        let byte = source.byte_at(offset);
        self.poll_source_byte_requests = self.poll_source_byte_requests.saturating_add(1);
        if byte == 0xff {
            self.source_error = Some(OpenCodeFenceScanError::SourceContainsSentinel {
                absolute_offset: offset,
            });
        }
        byte
    }

    fn restore_marker(&mut self) {
        self.record_logical_rewind(self.yymarker);
        self.yycursor = self.yymarker;
    }

    fn restore_context(&mut self) {
        self.record_logical_rewind(self.yyctxmarker);
        self.yycursor = self.yyctxmarker;
    }

    fn record_logical_rewind(&mut self, target: usize) {
        let rewind = self.yycursor.saturating_sub(target);
        self.poll_maximum_logical_rewind = self.poll_maximum_logical_rewind.max(rewind);
        self.maximum_logical_rewind = self.maximum_logical_rewind.max(rewind);
    }
}

fn scan_open_code_fence_cursor<S: OpenCodeFenceCursorSource>(
    scanner: &mut OpenCodeFenceCursorScanner,
    source: &mut S,
) -> OpenCodeFenceScanResult {
    let mut yych: u8;
/*!re2c
    re2c:api = generic;
    re2c:YYCTYPE = "u8";
    re2c:define:YYCURSOR = "scanner.yycursor";
    re2c:define:YYMARKER = "scanner.yymarker";
    re2c:define:YYCTXMARKER = "scanner.yyctxmarker";
    re2c:define:YYLIMIT = "scanner.source_len";
    re2c:define:YYPEEK = "scanner.peek(source)";
    re2c:define:YYSKIP = "scanner.yycursor += 1;";
    re2c:define:YYBACKUP = "scanner.yymarker = scanner.yycursor;";
    re2c:define:YYRESTORE = "scanner.restore_marker();";
    re2c:define:YYBACKUPCTX = "scanner.yyctxmarker = scanner.yycursor;";
    re2c:define:YYRESTORECTX = "scanner.restore_context();";
    re2c:define:YYLESSTHAN = "scanner.less_than(@@)";
    re2c:define:YYGETSTATE = "scanner.yystate";
    re2c:define:YYSETSTATE = "scanner.yystate = @@;";
    re2c:YYFILL = "return OpenCodeFenceScanResult::NeedMore;";

    // Inserted mechanically from pinned scanners.re::open_code_fence.
    [`]{3,} / [^`\r\n\xff]*[\r\n\xff] {
        return OpenCodeFenceScanResult::Matched(scanner.yycursor.min(scanner.source_len));
    }
    [~]{3,} / [^\r\n\xff]*[\r\n\xff] {
        return OpenCodeFenceScanResult::Matched(scanner.yycursor.min(scanner.source_len));
    }
    * { return OpenCodeFenceScanResult::NoMatch; }
*/}

/*!max:re2c*/

const LOOKAHEAD_SLACK: usize = YYMAXFILL - 1;
