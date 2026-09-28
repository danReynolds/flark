//! The extractor's internal record layout. It is richer than the published
//! model: every range carries UTF-8 bytes beside UTF-16 code units, which the
//! derivations and their validation work in. `model::Extractor::encode` packs
//! these records into the wire format described by `schema`. Tests expand a
//! published model back into this layout to check it against the source.
#![allow(dead_code)]

pub const HEADER_WORDS: usize = 11;
pub mod header {
    pub const MAGIC: usize = 0;
    pub const VERSION: usize = 1;
    pub const SRC_BYTES: usize = 2;
    pub const SRC_UTF16: usize = 3;
    pub const LINE_COUNT: usize = 4;
    pub const BLOCK_COUNT: usize = 5;
    pub const CONTENT_COUNT: usize = 6;
    pub const RUN_COUNT: usize = 7;
    pub const DEFINITION_COUNT: usize = 8;
    pub const STRING_BYTES: usize = 9;
    pub const RESERVED: usize = 10;
}
pub mod line {
    pub const WORDS: usize = 2;
    pub const START_BYTE: usize = 0;
    pub const START_UTF16: usize = 1;
}
pub mod block {
    pub const WORDS: usize = 16;
    pub const KIND: usize = 0;
    pub const PARENT: usize = 1;
    pub const START_BYTE: usize = 2;
    pub const END_BYTE: usize = 3;
    pub const START_UTF16: usize = 4;
    pub const END_UTF16: usize = 5;
    pub const FIRST_LINE: usize = 6;
    pub const LINE_COUNT: usize = 7;
    pub const CONTENT_OFFSET: usize = 8;
    pub const CONTENT_COUNT: usize = 9;
    /// Heading level, fence length, 1 if an ordered list, item content
    /// column, table column count, or table cell column.
    pub const ATTR0: usize = 10;
    /// Info string, task symbol or footnote label start byte; list start
    /// number; table alignments; table cell alignment.
    pub const ATTR1: usize = 11;
    /// Info string, task symbol or footnote label end byte.
    pub const ATTR2: usize = 12;
    pub const FLAGS: usize = 13;
    pub const MARKER_END_BYTE: usize = 14;
    pub const MARKER_END_UTF16: usize = 15;
}
pub mod content {
    pub const WORDS: usize = 8;
    pub const LINE: usize = 0;
    pub const START_BYTE: usize = 1;
    pub const START_UTF16: usize = 2;
    pub const END_BYTE: usize = 3;
    pub const END_UTF16: usize = 4;
    pub const VIRTUAL_LEADING_SPACES: usize = 5;
    pub const PREFIX_START_BYTE: usize = 6;
    pub const PREFIX_START_UTF16: usize = 7;
}
pub mod run {
    pub const WORDS: usize = 20;
    pub const KIND: usize = 0;
    pub const BLOCK: usize = 1;
    pub const PARENT: usize = 2;
    pub const START_BYTE: usize = 3;
    pub const END_BYTE: usize = 4;
    pub const CONTENT_START_BYTE: usize = 5;
    pub const CONTENT_END_BYTE: usize = 6;
    pub const START_UTF16: usize = 7;
    pub const END_UTF16: usize = 8;
    pub const CONTENT_START_UTF16: usize = 9;
    pub const CONTENT_END_UTF16: usize = 10;
    /// Link, image and autolink destination bytes; footnote label bytes;
    /// replacement text in the string table; code backtick count.
    pub const AUX0: usize = 11;
    pub const AUX1: usize = 12;
    /// Link and image title bytes; code display text in the string table.
    pub const AUX2: usize = 13;
    pub const AUX3: usize = 14;
    /// Bit 0 reference style, bit 1 has title or display text, bit 8 spans
    /// more than one line.
    pub const FLAGS: usize = 15;
    pub const DESTINATION_OFFSET: usize = 16;
    pub const DESTINATION_LENGTH: usize = 17;
    pub const TITLE_OFFSET: usize = 18;
    pub const TITLE_LENGTH: usize = 19;
}
pub mod definition {
    pub const WORDS: usize = 8;
    pub const START_BYTE: usize = 0;
    pub const END_BYTE: usize = 1;
    pub const START_UTF16: usize = 2;
    pub const END_UTF16: usize = 3;
    pub const LABEL_START_BYTE: usize = 4;
    pub const LABEL_END_BYTE: usize = 5;
    pub const DEST_START_BYTE: usize = 6;
    pub const DEST_END_BYTE: usize = 7;
}

/// Expand a published model into the extractor's internal layout
/// (`records`), deriving every byte offset from its UTF-16 offset through the
/// source. Fails when the packed structure is inconsistent: an offset between
/// the units of a surrogate pair, an extra record out of bounds or missing,
/// or block ranges that are not in order.
pub fn expand(src: &str, w: &[u32]) -> Result<Vec<u32>, String> {
    use crate::schema::{self, block as wb, block_kind, content as wc, definition as wd, extra, header as wh, run as wr, run_extra as wx, run_kind};
    if w.len() < schema::HEADER_WORDS { return Err("short header".into()); }
    if w[wh::MAGIC] != schema::MAGIC || w[wh::VERSION] != schema::VERSION { return Err("bad magic/version".into()); }
    let n = |f: usize| w[f] as usize;
    let (nl, nb, nc, nr, nd, nx, ne, ns) = (n(wh::LINE_COUNT), n(wh::BLOCK_COUNT), n(wh::CONTENT_COUNT), n(wh::RUN_COUNT), n(wh::DEFINITION_COUNT), n(wh::RUN_EXTRA_COUNT), n(wh::EXTRA_WORDS), n(wh::STRING_BYTES));
    let lines_off = schema::HEADER_WORDS;
    let blocks_off = lines_off + nl * schema::line::WORDS;
    let content_off = blocks_off + nb * wb::WORDS;
    let runs_off = content_off + nc * wc::WORDS;
    let defs_off = runs_off + nr * wr::WORDS;
    let index_off = defs_off + nd * wd::WORDS;
    let extras_off = index_off + nx * wx::WORDS;
    let strings_off = extras_off + ne;
    if w.len() != strings_off + ns.div_ceil(4) { return Err(format!("buffer words {} != expected {}", w.len(), strings_off + ns.div_ceil(4))); }
    if n(wh::SRC_BYTES) != src.len() { return Err("src_bytes".into()); }
    let src16 = src.encode_utf16().count();
    if n(wh::SRC_UTF16) != src16 { return Err("src_utf16".into()); }
    let mut byte_of: Vec<Option<u32>> = vec![None; src16 + 1];
    let mut unit = 0usize;
    for (i, ch) in src.char_indices() { byte_of[unit] = Some(i as u32); unit += ch.len_utf16(); }
    byte_of[src16] = Some(src.len() as u32);
    let byte = |u: u32, what: &str| byte_of.get(u as usize).copied().flatten().ok_or_else(|| format!("{what}: UTF-16 offset {u} is not a scalar boundary of {src16} units"));
    let extra_at = |offset: u32, words: usize, what: &str| -> Result<&[u32], String> {
        let o = offset as usize;
        if o + words > ne { return Err(format!("{what}: extra {o}+{words} beyond {ne} words")); }
        Ok(&w[extras_off + o..extras_off + o + words])
    };
    let string_in_bounds = |offset: u32, length: u32, what: &str| if offset as usize + length as usize > ns { Err(format!("{what}: string {offset}+{length} beyond {ns}")) } else { Ok(()) };
    let mut out = vec![0u32; HEADER_WORDS];
    out[header::MAGIC] = schema::MAGIC; out[header::VERSION] = schema::VERSION;
    out[header::SRC_BYTES] = src.len() as u32; out[header::SRC_UTF16] = src16 as u32;
    out[header::LINE_COUNT] = nl as u32; out[header::BLOCK_COUNT] = nb as u32;
    out[header::CONTENT_COUNT] = nc as u32; out[header::RUN_COUNT] = nr as u32;
    out[header::DEFINITION_COUNT] = nd as u32; out[header::STRING_BYTES] = ns as u32;
    for l in 0..nl { let s16 = w[lines_off + l]; out.extend([byte(s16, "line")?, s16]); }
    let blk = |b: usize, f: usize| w[blocks_off + b * wb::WORDS + f];
    let (mut first_run, mut content_at) = (vec![nr as u32; nb + 1], vec![nc as u32; nb + 1]);
    for b in 0..nb {
        first_run[b] = blk(b, wb::FIRST_RUN); content_at[b] = blk(b, wb::CONTENT_OFFSET);
        if first_run[b] as usize > nr || (b > 0 && first_run[b] < first_run[b - 1]) { return Err(format!("block {b} first run {}", first_run[b])); }
        if content_at[b] as usize > nc || (b > 0 && content_at[b] < content_at[b - 1]) { return Err(format!("block {b} content offset {}", content_at[b])); }
    }
    if nb > 0 && first_run[0] != 0 { return Err("first block's runs do not start at 0".into()); }
    if nb > 0 && content_at[0] != 0 { return Err("first block's content does not start at 0".into()); }
    for b in 0..nb {
        let kind_flags = blk(b, wb::KIND_FLAGS);
        let kind = kind_flags >> wb::kind_flags::KIND_SHIFT & wb::kind_flags::KIND_MASK;
        let flags = kind_flags >> wb::kind_flags::FLAGS_SHIFT & wb::kind_flags::FLAGS_MASK;
        let (s16, e16, attr, x) = (blk(b, wb::START), blk(b, wb::END), blk(b, wb::ATTR), blk(b, wb::EXTRA));
        let owns_extra = match kind { block_kind::CODE_BLOCK => flags & 1 != 0, block_kind::ITEM | block_kind::TABLE | block_kind::FOOTNOTE_DEFINITION => true, _ => false };
        if owns_extra != (x != extra::NONE) { return Err(format!("block {b} of kind {kind} has extra {x:#x}")); }
        let (mut attr0, mut attr1, mut attr2, mut v4_flags, mut marker) = (attr, 0, 0, flags, (0, 0));
        match kind {
            block_kind::CODE_BLOCK if flags & 1 != 0 => {
                let e = extra_at(x, extra::code_block::WORDS, "code block")?;
                attr1 = byte(e[extra::code_block::INFO_START], "info start")?; attr2 = byte(e[extra::code_block::INFO_END], "info end")?;
            }
            block_kind::LIST => { attr0 = flags >> 1 & 1; attr1 = attr; v4_flags = flags & 1; }
            block_kind::ITEM => {
                let e = extra_at(x, extra::item::WORDS, "item")?;
                marker = (byte(e[extra::item::MARKER_END], "marker end")?, e[extra::item::MARKER_END]);
                attr1 = byte(e[extra::item::TASK_START], "task start")?; attr2 = byte(e[extra::item::TASK_END], "task end")?;
            }
            block_kind::TABLE => attr1 = extra_at(x, extra::table::WORDS, "table")?[extra::table::ALIGNMENTS],
            block_kind::FOOTNOTE_DEFINITION => {
                let e = extra_at(x, extra::footnote_definition::WORDS, "footnote definition")?;
                attr1 = byte(e[extra::footnote_definition::LABEL_START], "label start")?; attr2 = byte(e[extra::footnote_definition::LABEL_END], "label end")?;
            }
            _ => {}
        }
        out.extend([
            kind, blk(b, wb::PARENT), byte(s16, "block start")?, byte(e16, "block end")?, s16, e16,
            blk(b, wb::FIRST_LINE), blk(b, wb::LINE_COUNT), content_at[b], content_at[b + 1] - content_at[b],
            attr0, attr1, attr2, v4_flags, marker.0, marker.1,
        ]);
    }
    for c in 0..nc {
        let cw = |f: usize| w[content_off + c * wc::WORDS + f];
        let packed = cw(wc::LINE_VIRTUAL);
        let (line, virt) = (packed >> wc::line_virtual::LINE_SHIFT & wc::line_virtual::LINE_MASK, packed >> wc::line_virtual::VIRTUAL_LEADING_SPACES_SHIFT & wc::line_virtual::VIRTUAL_LEADING_SPACES_MASK);
        let (cs, ce, prefix) = (cw(wc::START), cw(wc::END), cw(wc::PREFIX_START));
        out.extend([line, byte(cs, "content start")?, cs, byte(ce, "content end")?, ce, virt, byte(prefix, "prefix start")?, prefix]);
    }
    let mut run_extra = vec![None; nr];
    for i in 0..nx {
        let (r, offset) = (w[index_off + i * wx::WORDS + wx::RUN], w[index_off + i * wx::WORDS + wx::OFFSET]);
        if r as usize >= nr || (i > 0 && r <= w[index_off + (i - 1) * wx::WORDS + wx::RUN]) { return Err(format!("run-extra index entry {i} names run {r}")); }
        run_extra[r as usize] = Some(offset);
    }
    let mut owner = 0usize;
    for i in 0..nr {
        while owner + 1 < nb && first_run[owner + 1] as usize <= i { owner += 1; }
        let rw = |f: usize| w[runs_off + i * wr::WORDS + f];
        let (s16, e16, hidden, packed) = (rw(wr::START), rw(wr::END), rw(wr::HIDDEN), rw(wr::KIND_FLAGS_PARENT));
        let kind = packed >> wr::kind_flags_parent::KIND_SHIFT & wr::kind_flags_parent::KIND_MASK;
        let flags = packed >> wr::kind_flags_parent::FLAGS_SHIFT & wr::kind_flags_parent::FLAGS_MASK;
        let distance = packed >> wr::kind_flags_parent::PARENT_DISTANCE_SHIFT & wr::kind_flags_parent::PARENT_DISTANCE_MASK;
        let wide = flags & 8 != 0;
        let owns_extra = wide || matches!(kind, run_kind::LINK | run_kind::IMAGE | run_kind::AUTOLINK | run_kind::REPLACEMENT | run_kind::FOOTNOTE_REF) || (kind == run_kind::CODE && flags & 2 != 0);
        if owns_extra != run_extra[i].is_some() { return Err(format!("run {i} of kind {kind} extra index {:?}", run_extra[i])); }
        let mut cursor = run_extra[i].unwrap_or(0);
        let (cs16, ce16, parent);
        if wide {
            let e = extra_at(cursor, extra::wide_run::WORDS, "wide run")?;
            (cs16, ce16, parent) = (e[extra::wide_run::CONTENT_START], e[extra::wide_run::CONTENT_END], e[extra::wide_run::PARENT]);
            cursor += extra::wide_run::WORDS as u32;
        } else {
            let (before, after) = (hidden >> wr::hidden::BEFORE_SHIFT & wr::hidden::BEFORE_MASK, hidden >> wr::hidden::AFTER_SHIFT & wr::hidden::AFTER_MASK);
            if before + s16 > e16 || after > e16 { return Err(format!("run {i} hidden lengths {before}/{after} exceed {s16}..{e16}")); }
            (cs16, ce16) = (s16 + before, e16 - after);
            parent = if distance == 0 { u32::MAX } else if distance as usize > i { return Err(format!("run {i} parent distance {distance}")); } else { i as u32 - distance };
        }
        let (mut aux, mut resolved) = ([0u32; 4], [0u32; 4]);
        match kind {
            run_kind::LINK | run_kind::IMAGE | run_kind::AUTOLINK => {
                let e = extra_at(cursor, extra::link::WORDS, "link")?;
                for (k, f) in [extra::link::DESTINATION_START, extra::link::DESTINATION_END, extra::link::TITLE_START, extra::link::TITLE_END].into_iter().enumerate() { aux[k] = byte(e[f], "link range")?; }
                resolved = [e[extra::link::DESTINATION_OFFSET], e[extra::link::DESTINATION_LENGTH], e[extra::link::TITLE_OFFSET], e[extra::link::TITLE_LENGTH]];
                string_in_bounds(resolved[0], resolved[1], "destination")?; string_in_bounds(resolved[2], resolved[3], "title")?;
            }
            run_kind::REPLACEMENT => {
                let e = extra_at(cursor, extra::display_text::WORDS, "replacement")?;
                aux[0] = e[extra::display_text::OFFSET]; aux[1] = e[extra::display_text::LENGTH];
                string_in_bounds(aux[0], aux[1], "replacement")?;
            }
            run_kind::CODE if flags & 2 != 0 => {
                let e = extra_at(cursor, extra::display_text::WORDS, "code display")?;
                aux[2] = e[extra::display_text::OFFSET]; aux[3] = e[extra::display_text::LENGTH];
                string_in_bounds(aux[2], aux[3], "code display")?;
            }
            run_kind::FOOTNOTE_REF => {
                let e = extra_at(cursor, extra::footnote_ref::WORDS, "footnote reference")?;
                aux[0] = byte(e[extra::footnote_ref::LABEL_START], "label start")?; aux[1] = byte(e[extra::footnote_ref::LABEL_END], "label end")?;
            }
            _ => {}
        }
        out.extend([
            kind, owner as u32, parent, byte(s16, "run start")?, byte(e16, "run end")?, byte(cs16, "run content start")?, byte(ce16, "run content end")?,
            s16, e16, cs16, ce16, aux[0], aux[1], aux[2], aux[3], flags & 3 | (flags >> 2 & 1) << 8,
            resolved[0], resolved[1], resolved[2], resolved[3],
        ]);
    }
    for d in 0..nd {
        let dw = |f: usize| w[defs_off + d * wd::WORDS + f];
        let (s16, e16) = (dw(wd::START), dw(wd::END));
        out.extend([byte(s16, "definition start")?, byte(e16, "definition end")?, s16, e16,
            byte(dw(wd::LABEL_START), "label start")?, byte(dw(wd::LABEL_END), "label end")?,
            byte(dw(wd::DEST_START), "destination start")?, byte(dw(wd::DEST_END), "destination end")?]);
    }
    out.extend_from_slice(&w[strings_off..]);
    Ok(out)
}
