use std::collections::VecDeque;
use std::env;
use std::hint::black_box;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "custom")]
use std::sync::Arc;
use std::time::{Duration, Instant};

use crop::{Rope, RopeBuilder};
#[cfg(feature = "custom")]
use flark_integrated_parser_slice::source::PersistentSource;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const HISTORY_ROOTS: usize = 64;
const TEXT_PATTERN: &str = "paragraph *em* and `code` with UTF-8 λ🦀\r\n\r\n";
static NEXT_CROP_ROOT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootIdentity(u64);

impl RootIdentity {
    fn mint() -> Self {
        let value = NEXT_CROP_ROOT_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(value, u64::MAX, "source root identity space exhausted");
        Self(value)
    }
}

/// Production-shaped snapshot wrapper around Crop's deliberately opaque root.
///
/// Crop supplies the immutable COW lease. Flark must supply exact generation
/// identity and edit provenance because neither is exposed by Crop's public
/// API.
#[derive(Clone, Debug)]
struct CropSnapshot {
    root: Rope,
    identity: RootIdentity,
}

impl CropSnapshot {
    fn from_text(text: &str) -> Self {
        Self {
            root: Rope::from(text),
            identity: RootIdentity::mint(),
        }
    }

    fn from_root(root: Rope) -> Self {
        Self {
            root,
            identity: RootIdentity::mint(),
        }
    }

    const fn identity(&self) -> RootIdentity {
        self.identity
    }

    fn byte_len(&self) -> usize {
        self.root.byte_len()
    }

    fn byte(&self, at: usize) -> u8 {
        self.root.byte(at)
    }

    fn is_char_boundary(&self, at: usize) -> bool {
        self.root.is_char_boundary(at)
    }

    fn descriptor(&self, range: Range<usize>) -> RangeDescriptor {
        assert!(range.start <= range.end);
        assert!(range.end <= self.byte_len());
        assert!(self.is_char_boundary(range.start));
        assert!(self.is_char_boundary(range.end));
        RangeDescriptor {
            source: self.identity,
            start: range.start,
            end: range.end,
        }
    }

    fn bind(&self, descriptor: RangeDescriptor) -> Result<BoundRange<'_>, BindError> {
        if descriptor.source != self.identity {
            return Err(BindError::WrongRoot {
                descriptor: descriptor.source,
                snapshot: self.identity,
            });
        }
        if descriptor.start > descriptor.end || descriptor.end > self.byte_len() {
            return Err(BindError::InvalidRange);
        }
        if !self.is_char_boundary(descriptor.start) || !self.is_char_boundary(descriptor.end) {
            return Err(BindError::NotCharBoundary);
        }
        Ok(BoundRange {
            snapshot: self,
            range: descriptor.start..descriptor.end,
        })
    }

    fn edit(&self, range: Range<usize>, replacement: &str) -> (Self, EditProvenance) {
        assert!(range.start <= range.end);
        assert!(range.end <= self.byte_len());
        assert!(self.is_char_boundary(range.start));
        assert!(self.is_char_boundary(range.end));

        let old_len = self.byte_len();
        let mut root = self.root.clone();
        root.replace(range.clone(), replacement);
        let next = Self {
            root,
            identity: RootIdentity::mint(),
        };
        let new_suffix_start = range.start + replacement.len();
        let provenance = EditProvenance {
            from: self.identity,
            to: next.identity,
            edited_old: range.clone(),
            inserted_bytes: replacement.len(),
            unchanged_prefix: UnchangedRegion {
                old: 0..range.start,
                new: 0..range.start,
            },
            unchanged_suffix: UnchangedRegion {
                old: range.end..old_len,
                new: new_suffix_start..next.byte_len(),
            },
        };
        (next, provenance)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RangeDescriptor {
    source: RootIdentity,
    start: usize,
    end: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum BindError {
    WrongRoot {
        descriptor: RootIdentity,
        snapshot: RootIdentity,
    },
    InvalidRange,
    NotCharBoundary,
}

struct BoundRange<'a> {
    snapshot: &'a CropSnapshot,
    range: Range<usize>,
}

impl BoundRange<'_> {
    fn owned_bytes(&self) -> Vec<u8> {
        owned_crop_range(&self.snapshot.root, self.range.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnchangedRegion {
    old: Range<usize>,
    new: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditProvenance {
    from: RootIdentity,
    to: RootIdentity,
    edited_old: Range<usize>,
    inserted_bytes: usize,
    unchanged_prefix: UnchangedRegion,
    unchanged_suffix: UnchangedRegion,
}

impl EditProvenance {
    fn map_unchanged_range(
        &self,
        source: RootIdentity,
        range: Range<usize>,
    ) -> Option<Range<usize>> {
        if source != self.from || range.start > range.end {
            return None;
        }
        if range.start >= self.unchanged_prefix.old.start
            && range.end <= self.unchanged_prefix.old.end
        {
            return Some(range);
        }
        if range.start >= self.unchanged_suffix.old.start
            && range.end <= self.unchanged_suffix.old.end
        {
            let relative_start = range.start - self.unchanged_suffix.old.start;
            let relative_end = range.end - self.unchanged_suffix.old.start;
            return Some(
                self.unchanged_suffix.new.start + relative_start
                    ..self.unchanged_suffix.new.start + relative_end,
            );
        }
        None
    }
}

fn main() {
    let mut arguments = env::args().skip(1);
    let backend = arguments.next().unwrap_or_else(|| "crop".to_owned());
    let mib = parse_or(arguments.next(), 10_usize, "MiB");
    let edits = parse_or(arguments.next(), 1_000_usize, "edit count");
    let poll_bytes = parse_or(arguments.next(), 4_096_usize, "poll bytes");
    assert!(mib > 0);
    assert!(poll_bytes > 0);

    let requested_bytes = mib.checked_mul(1024 * 1024).expect("requested size");
    match backend.as_str() {
        "crop-stream" => run_crop_streaming(requested_bytes, edits, poll_bytes),
        "crop" => run_crop(make_text(requested_bytes), edits, poll_bytes),
        #[cfg(feature = "custom")]
        "custom" => run_custom(make_text(requested_bytes), edits, poll_bytes),
        #[cfg(not(feature = "custom"))]
        "custom" => panic!("re-run with --features custom"),
        _ => panic!("backend must be crop, crop-stream, or custom"),
    }
}

fn parse_or<T>(value: Option<String>, default: T, label: &str) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    value.map_or(default, |value| value.parse().expect(label))
}

fn make_text(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes);
    while text.len() + TEXT_PATTERN.len() <= bytes {
        text.push_str(TEXT_PATTERN);
    }
    while text.len() < bytes {
        text.push('a');
    }
    text
}

fn run_crop(text: String, edits: usize, poll_bytes: usize) {
    let actual_bytes = text.len();
    let build_started = Instant::now();
    let current = CropSnapshot::from_text(text.as_str());
    let build = build_started.elapsed();
    drop(text);
    finish_crop(
        current,
        actual_bytes,
        build,
        edits,
        poll_bytes,
        "crop",
        "true",
    );
}

fn run_crop_streaming(requested_bytes: usize, edits: usize, poll_bytes: usize) {
    let build_started = Instant::now();
    let reusable = TEXT_PATTERN.repeat(1_024);
    let mut builder = RopeBuilder::new();
    let mut appended_bytes = 0;
    while appended_bytes + reusable.len() <= requested_bytes {
        builder.append(&reusable);
        appended_bytes += reusable.len();
    }
    while appended_bytes + TEXT_PATTERN.len() <= requested_bytes {
        builder.append(TEXT_PATTERN);
        appended_bytes += TEXT_PATTERN.len();
    }
    if appended_bytes < requested_bytes {
        builder.append("a".repeat(requested_bytes - appended_bytes));
    }
    let current = CropSnapshot::from_root(builder.build());
    let build = build_started.elapsed();
    finish_crop(
        current,
        requested_bytes,
        build,
        edits,
        poll_bytes,
        "crop-stream",
        "not_allocated",
    );
}

fn finish_crop(
    mut current: CropSnapshot,
    actual_bytes: usize,
    build: Duration,
    edits: usize,
    poll_bytes: usize,
    backend: &str,
    generator_dropped: &str,
) {
    let clone_started = Instant::now();
    let initial_snapshot = current.clone();
    let clone = clone_started.elapsed();
    assert_eq!(initial_snapshot.byte_len(), actual_bytes);
    assert_eq!(initial_snapshot.identity(), current.identity());

    let scan = scan_crop(&current, poll_bytes);
    let edit = edit_crop(&mut current, edits);
    let suffix_start = current.byte_len().saturating_sub(poll_bytes);
    let suffix_started = Instant::now();
    let suffix_descriptor = current.descriptor(suffix_start..current.byte_len());
    let suffix = current
        .bind(suffix_descriptor)
        .expect("same-root suffix descriptor")
        .owned_bytes();
    let suffix_read = suffix_started.elapsed();
    black_box(suffix);

    let drop_started = Instant::now();
    drop(initial_snapshot);
    drop(current);
    let final_drop = drop_started.elapsed();

    println!(
        "backend={backend} bytes={actual_bytes} generator_dropped={generator_dropped} build_us={} clone_ns={} scan={} edits={} suffix_owned_us={} final_two_root_drop_us={}",
        build.as_micros(),
        clone.as_nanos(),
        scan.summary(),
        edit.summary(),
        suffix_read.as_micros(),
        final_drop.as_micros(),
    );
}

fn scan_crop(source: &CropSnapshot, poll_bytes: usize) -> ScanReceipt {
    let mut timings = Timings::default();
    let mut digest = FNV_OFFSET;
    let mut offset = 0;
    while offset < source.byte_len() {
        let mut end = (offset + poll_bytes).min(source.byte_len());
        while end > offset && !source.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset {
            end = (offset + poll_bytes).min(source.byte_len());
            while end < source.byte_len() && !source.is_char_boundary(end) {
                end += 1;
            }
        }
        let started = Instant::now();
        for chunk in source.root.byte_slice(offset..end).chunks() {
            digest = fold(digest, chunk.as_bytes());
        }
        timings.push(started.elapsed());
        offset = end;
    }
    black_box(digest);
    ScanReceipt { timings, digest }
}

fn edit_crop(current: &mut CropSnapshot, edits: usize) -> EditReceipt {
    let mut history = VecDeque::with_capacity(HISTORY_ROOTS);
    let mut timings = Timings::default();
    for ordinal in 0..edits {
        let old = current.clone();
        let len = current.byte_len();
        let mut at = pseudo_position(ordinal, len.saturating_sub(1));
        while at > 0
            && (!current.is_char_boundary(at)
                || !current.is_char_boundary(at + 1)
                || !current.byte(at).is_ascii())
        {
            at -= 1;
        }
        let old_byte = current.byte(at);
        let replacement = if old_byte == b'x' { "y" } else { "x" };
        let started = Instant::now();
        let (next, provenance) = current.edit(at..at + 1, replacement);
        timings.push(started.elapsed());
        assert_eq!(old.byte(at), old_byte, "snapshot changed after COW edit");
        assert_eq!(provenance.from, old.identity());
        assert_eq!(provenance.to, next.identity());
        assert_eq!(
            provenance.map_unchanged_range(old.identity(), at + 1..old.byte_len()),
            Some(at + 1..next.byte_len())
        );
        *current = next;
        history.push_back(old);
        if history.len() > HISTORY_ROOTS {
            drop(history.pop_front());
        }
    }
    let retained_roots = history.len();
    let drop_started = Instant::now();
    drop(history);
    EditReceipt {
        timings,
        retained_roots,
        history_drop: drop_started.elapsed(),
    }
}

fn owned_crop_range(source: &Rope, range: std::ops::Range<usize>) -> Vec<u8> {
    let mut result = Vec::with_capacity(range.len());
    for chunk in source.byte_slice(range).chunks() {
        result.extend_from_slice(chunk.as_bytes());
    }
    result
}

#[cfg(feature = "custom")]
fn run_custom(text: String, edits: usize, poll_bytes: usize) {
    let actual_bytes = text.len();
    let build_started = Instant::now();
    let mut current = Arc::new(PersistentSource::from_text(&text));
    let build = build_started.elapsed();
    drop(text);

    let clone_started = Instant::now();
    let initial_snapshot = Arc::clone(&current);
    let clone = clone_started.elapsed();
    assert_eq!(initial_snapshot.len_bytes(), actual_bytes);

    let scan = scan_custom(&current, poll_bytes);
    let edit = edit_custom(&mut current, edits);
    let suffix_start = current.len_bytes().saturating_sub(poll_bytes);
    let suffix_started = Instant::now();
    let suffix = owned_custom_range(&current, suffix_start, current.len_bytes());
    let suffix_read = suffix_started.elapsed();
    black_box(suffix);

    let drop_started = Instant::now();
    drop(initial_snapshot);
    drop(current);
    let final_drop = drop_started.elapsed();

    println!(
        "backend=custom bytes={actual_bytes} generator_dropped=true build_us={} clone_ns={} scan={} edits={} suffix_owned_us={} final_two_root_drop_us={}",
        build.as_micros(),
        clone.as_nanos(),
        scan.summary(),
        edit.summary(),
        suffix_read.as_micros(),
        final_drop.as_micros(),
    );
}

#[cfg(feature = "custom")]
fn scan_custom(source: &PersistentSource, poll_bytes: usize) -> ScanReceipt {
    let mut timings = Timings::default();
    let mut digest = FNV_OFFSET;
    let mut offset = 0;
    while offset < source.len_bytes() {
        let end = (offset + poll_bytes).min(source.len_bytes());
        let started = Instant::now();
        let mut cursor = source.cursor_at(offset).expect("in-bounds cursor");
        for _ in offset..end {
            let byte = cursor.next().expect("cursor covers poll").byte;
            digest = fold_byte(digest, byte);
        }
        timings.push(started.elapsed());
        offset = end;
    }
    black_box(digest);
    ScanReceipt { timings, digest }
}

#[cfg(feature = "custom")]
fn edit_custom(current: &mut Arc<PersistentSource>, edits: usize) -> EditReceipt {
    let mut history = VecDeque::with_capacity(HISTORY_ROOTS);
    let mut timings = Timings::default();
    for ordinal in 0..edits {
        let old = Arc::clone(current);
        let len = current.len_bytes();
        let mut at = pseudo_position(ordinal, len.saturating_sub(1));
        while at > 0
            && (!current.is_char_boundary(at)
                || !current.is_char_boundary(at + 1)
                || !current.byte_at(at).expect("edit byte").byte.is_ascii())
        {
            at -= 1;
        }
        let old_byte = current.byte_at(at).expect("edit byte").byte;
        let replacement = if old_byte == b'x' { "y" } else { "x" };
        let started = Instant::now();
        let next = current
            .edit(at..at + 1, replacement)
            .expect("scalar-safe edit")
            .source;
        timings.push(started.elapsed());
        assert_eq!(old.byte_at(at).expect("old snapshot byte").byte, old_byte);
        *current = Arc::new(next);
        history.push_back(old);
        if history.len() > HISTORY_ROOTS {
            drop(history.pop_front());
        }
    }
    let retained_roots = history.len();
    let drop_started = Instant::now();
    drop(history);
    EditReceipt {
        timings,
        retained_roots,
        history_drop: drop_started.elapsed(),
    }
}

#[cfg(feature = "custom")]
fn owned_custom_range(source: &PersistentSource, start: usize, end: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(end - start);
    let mut cursor = source.cursor_at(start).expect("in-bounds cursor");
    for _ in start..end {
        result.push(cursor.next().expect("cursor covers range").byte);
    }
    result
}

fn pseudo_position(ordinal: usize, upper_exclusive: usize) -> usize {
    if upper_exclusive == 0 {
        return 0;
    }
    ordinal.wrapping_mul(1_103_515_245).wrapping_add(12_345) % upper_exclusive
}

fn fold(mut digest: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        digest = fold_byte(digest, *byte);
    }
    digest
}

const fn fold_byte(digest: u64, byte: u8) -> u64 {
    (digest ^ byte as u64).wrapping_mul(FNV_PRIME)
}

struct ScanReceipt {
    timings: Timings,
    digest: u64,
}

impl ScanReceipt {
    fn summary(&self) -> String {
        format!("{},digest={:016x}", self.timings.summary(), self.digest)
    }
}

struct EditReceipt {
    timings: Timings,
    retained_roots: usize,
    history_drop: Duration,
}

impl EditReceipt {
    fn summary(&self) -> String {
        format!(
            "{},retained_roots={},history_drop_us={}",
            self.timings.summary(),
            self.retained_roots,
            self.history_drop.as_micros()
        )
    }
}

#[derive(Default)]
struct Timings {
    values: Vec<u128>,
    total: Duration,
    max: Duration,
}

impl Timings {
    fn push(&mut self, value: Duration) {
        self.values.push(value.as_nanos());
        self.total += value;
        self.max = self.max.max(value);
    }

    fn summary(&self) -> String {
        let mut values = self.values.clone();
        values.sort_unstable();
        format!(
            "ops={},total_us={},p50_ns={},p95_ns={},p99_ns={},max_ns={}",
            values.len(),
            self.total.as_micros(),
            percentile(&values, 50),
            percentile(&values, 95),
            percentile(&values, 99),
            self.max.as_nanos()
        )
    }
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() - 1).saturating_mul(percentile) / 100;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn materialize_crop(source: &CropSnapshot) -> String {
        String::from_utf8(owned_crop_range(&source.root, 0..source.byte_len()))
            .expect("Crop stores UTF-8")
    }

    #[test]
    fn descriptors_are_bound_to_the_exact_snapshot_identity() {
        let old = CropSnapshot::from_text("alpha *beta* omega");
        let descriptor = old.descriptor(6..12);
        let old_lease = old.clone();
        let (next, _) = old.edit(0..0, "new ");

        assert_eq!(old_lease.bind(descriptor).unwrap().owned_bytes(), b"*beta*");
        assert_eq!(old_lease.identity(), descriptor.source);
        assert_eq!(old_lease.identity(), old.identity());
        assert_ne!(next.identity(), old.identity());
        let Err(wrong_root) = next.bind(descriptor) else {
            panic!("old descriptor bound to a different source root");
        };
        assert_eq!(
            wrong_root,
            BindError::WrongRoot {
                descriptor: old.identity(),
                snapshot: next.identity(),
            }
        );
    }

    #[test]
    fn edit_provenance_maps_only_exact_unchanged_prefix_and_suffix() {
        let old = CropSnapshot::from_text("prefix [replace me] suffix 🦀\r\nend");
        let old_text = materialize_crop(&old);
        let edit_start = old_text.find('[').unwrap();
        let edit_end = old_text.find(']').unwrap() + 1;
        let suffix_start = old_text.find("suffix").unwrap();
        let suffix_end = old.byte_len();
        let old_suffix = old.descriptor(suffix_start..suffix_end);

        let (next, provenance) = old.edit(edit_start..edit_end, "X");
        let mapped = provenance
            .map_unchanged_range(old.identity(), suffix_start..suffix_end)
            .expect("suffix is exact unchanged lineage");
        let next_suffix = next.descriptor(mapped.clone());

        assert_eq!(provenance.from, old.identity());
        assert_eq!(provenance.to, next.identity());
        assert_eq!(
            old.bind(old_suffix).unwrap().owned_bytes(),
            next.bind(next_suffix).unwrap().owned_bytes()
        );
        assert_eq!(&materialize_crop(&old), &old_text, "old lease mutated");
        assert_eq!(
            provenance.map_unchanged_range(old.identity(), edit_start..edit_end),
            None,
            "edited bytes must not inherit lineage"
        );
        assert_eq!(
            provenance.map_unchanged_range(next.identity(), suffix_start..suffix_end),
            None,
            "provenance is directional and root-specific"
        );
        assert_eq!(
            &materialize_crop(&next)[mapped],
            &old_text[suffix_start..suffix_end]
        );
    }

    #[test]
    fn suffix_mapping_handles_both_growth_and_shrinkage() {
        for replacement in ["a much longer replacement", "x"] {
            let old = CropSnapshot::from_text("before EDIT after");
            let (next, provenance) = old.edit(7..11, replacement);
            let mapped = provenance
                .map_unchanged_range(old.identity(), 12..17)
                .expect("unchanged suffix");
            assert_eq!(&materialize_crop(&next)[mapped], "after");
        }
    }

    #[test]
    fn utf16_conversion_and_crlf_spelling_survive_snapshots_and_edits() {
        let source = CropSnapshot::from_text("a\r\nβ🦀\r\nz");
        let exact = materialize_crop(&source);
        assert_eq!(exact, "a\r\nβ🦀\r\nz");
        assert_eq!(source.root.line_len(), 3);
        assert_eq!(source.root.utf16_len(), exact.encode_utf16().count());

        for byte in exact
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(exact.len()))
        {
            let utf16 = source.root.utf16_code_unit_of_byte(byte);
            assert_eq!(source.root.byte_of_utf16_code_unit(utf16), byte);
        }

        let crab = exact.find('🦀').unwrap();
        let crab_utf16 = source.root.utf16_code_unit_of_byte(crab);
        let (next, _) = source.edit(crab..crab + '🦀'.len_utf8(), "ok");
        assert_eq!(source.root.byte_of_utf16_code_unit(crab_utf16), crab);
        assert_eq!(
            materialize_crop(&source),
            exact,
            "old UTF-16 snapshot changed"
        );
        assert_eq!(materialize_crop(&next), "a\r\nβok\r\nz");
    }

    #[test]
    fn leaf_descriptors_do_not_capture_or_clone_the_source_root() {
        let snapshot = CropSnapshot::from_text(&"line\n".repeat(10_000));
        let descriptors: Vec<_> = (0..10_000)
            .map(|line| snapshot.descriptor(line * 5..line * 5 + 4))
            .collect();

        assert!(std::mem::size_of::<RangeDescriptor>() <= 3 * std::mem::size_of::<usize>());
        assert!(!std::mem::needs_drop::<RangeDescriptor>());
        assert!(descriptors
            .iter()
            .all(|item| item.source == snapshot.identity()));
        assert_eq!(
            snapshot.bind(descriptors[9_999]).unwrap().owned_bytes(),
            b"line"
        );
    }

    #[cfg(feature = "custom")]
    #[test]
    fn both_backends_match_hashes_and_deterministic_edits() {
        let text = make_text(1024 * 1024);
        let mut crop = CropSnapshot::from_text(&text);
        let mut custom = Arc::new(PersistentSource::from_text(&text));
        let crop_scan = scan_crop(&crop, 4_096);
        let custom_scan = scan_custom(&custom, 4_096);
        assert_eq!(crop_scan.digest, custom_scan.digest);

        for ordinal in 0..256 {
            let len = crop.byte_len();
            assert_eq!(len, custom.len_bytes());
            let mut at = pseudo_position(ordinal, len.saturating_sub(1));
            while at > 0
                && (!crop.is_char_boundary(at)
                    || !crop.is_char_boundary(at + 1)
                    || !crop.byte(at).is_ascii())
            {
                at -= 1;
            }
            let old_byte = crop.byte(at);
            assert_eq!(custom.byte_at(at).unwrap().byte, old_byte);
            let replacement = if old_byte == b'x' { "y" } else { "x" };
            crop = crop.edit(at..at + 1, replacement).0;
            custom = Arc::new(custom.edit(at..at + 1, replacement).unwrap().source);
        }

        assert_eq!(materialize_crop(&crop), custom.materialize());
        assert_eq!(
            scan_crop(&crop, 4_096).digest,
            scan_custom(&custom, 4_096).digest
        );
    }

    #[cfg(feature = "custom")]
    #[test]
    fn mixed_unicode_crlf_splices_match_a_string_oracle() {
        const REPLACEMENTS: [&str; 7] = ["", "x", "β", "🦀", "\r\n", "**", "`code`"];
        let mut oracle = make_text(256 * 1_024);
        let mut crop = CropSnapshot::from_text(&oracle);
        let mut custom = Arc::new(PersistentSource::from_text(&oracle));

        for ordinal in 0..1_024 {
            let mut at = pseudo_position(ordinal, oracle.len().saturating_add(1));
            while at > 0 && !oracle.is_char_boundary(at) {
                at -= 1;
            }
            let end = if ordinal % 3 == 0 || at == oracle.len() {
                at
            } else {
                at + oracle[at..].chars().next().unwrap().len_utf8()
            };
            let replacement = REPLACEMENTS[ordinal % REPLACEMENTS.len()];

            crop = crop.edit(at..end, replacement).0;
            custom = Arc::new(custom.edit(at..end, replacement).unwrap().source);
            oracle.replace_range(at..end, replacement);

            assert_eq!(crop.byte_len(), oracle.len());
            assert_eq!(custom.len_bytes(), oracle.len());
            assert_eq!(crop.root.utf16_len(), oracle.encode_utf16().count());
            if ordinal % 64 == 0 {
                crop.root.assert_invariants();
                custom.validate().unwrap();
                assert_eq!(materialize_crop(&crop), oracle);
                assert_eq!(custom.materialize(), oracle);
            }
        }

        assert_eq!(materialize_crop(&crop), oracle);
        assert_eq!(custom.materialize(), oracle);
        assert_eq!(
            scan_crop(&crop, 4_096).digest,
            scan_custom(&custom, 4_096).digest
        );
    }
}
