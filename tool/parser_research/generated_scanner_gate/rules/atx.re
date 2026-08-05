// Generated with:
// re2rust 4.3.1 --no-generation-date --storable-state \
//   -o src/atx_generated.rs rules/atx.re
//
// Do not add Markdown syntax here. generate.py inserts the exact ATX pattern
// extracted from the pinned Comrak scanners.re source.

/// Result of one resumable scanner poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanResult {
    /// The scanner reached the current source grant and saved exact DFA state.
    NeedMore,
    /// The scanner accepted the ATX prefix and reports the donor byte count.
    Matched(usize),
    /// The input cannot match the ATX rule.
    NoMatch,
}

/// A proof-only source window around a generated storable DFA.
///
/// The complete byte vector stands in for a Crop-backed random-access cursor;
/// `available` is the only prefix the generated scanner is allowed to inspect.
pub struct AtxScanner {
    yyinput: Vec<u8>,
    source_len: usize,
    available: usize,
    yylimit: usize,
    yycursor: usize,
    yymarker: usize,
    yystate: isize,
}

impl AtxScanner {
    /// Creates a scanner with no source bytes granted yet.
    #[must_use]
    pub fn new(source: &[u8]) -> Self {
        assert!(!source.contains(&0xff), "UTF-8 source cannot contain 0xff");
        let mut yyinput = source.to_vec();
        yyinput.extend([0xff; YYMAXFILL]);
        Self {
            yyinput,
            source_len: source.len(),
            available: 0,
            yylimit: 0,
            yycursor: 0,
            yymarker: 0,
            yystate: -1,
        }
    }

    /// Grants at most `bytes` additional source bytes, then resumes the DFA.
    pub fn poll(&mut self, bytes: usize) -> ScanResult {
        self.available = self
            .available
            .saturating_add(bytes)
            .min(self.source_len);
        // YYMAXFILL fake sentinels become visible only after every real byte
        // has been granted. Before then YYFILL suspends with exact DFA state.
        self.yylimit = if self.available == self.source_len {
            self.source_len + YYMAXFILL
        } else {
            self.available
        };
        scan(self)
    }

    /// Returns the number of source bytes inspected or skipped by the DFA.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.yycursor.min(self.source_len)
    }
}

fn scan(yyrecord: &mut AtxScanner) -> ScanResult {
    let mut yych;
/*!re2c
    re2c:api = record;
    re2c:YYCTYPE = "u8";
    re2c:YYFILL = "return ScanResult::NeedMore;";

    // Inserted mechanically from pinned scanners.re::atx_heading_start.
    [#]{1,6} ([ \t]+|[\r\n\xff]) {
        return ScanResult::Matched(yyrecord.yycursor.min(yyrecord.source_len));
    }
    * { return ScanResult::NoMatch; }
*/}

/*!max:re2c*/

