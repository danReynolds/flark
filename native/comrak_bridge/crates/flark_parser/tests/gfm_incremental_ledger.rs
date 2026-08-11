// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use flark_engine::parser_internal::{M11ReferenceResolution, M11ReferenceResolver};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};
use flark_parser::{
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
};
use sha2::{Digest, Sha256};

const GFM_FIXTURES: &str =
    include_str!("../../../../../test/fixtures/commonmark/upstream/gfm_tests.json");
const EXPECTED_EXAMPLES: usize = 672;
const HISTORIES_PER_EXAMPLE: usize = 6;
const FUEL: usize = 64;

#[derive(Debug)]
struct Fixture {
    example: usize,
    markdown: String,
}

#[derive(Clone, Copy, Debug)]
enum HistoryKind {
    Type,
    Erase,
    Split,
    Merge,
    Paste,
    Incomplete,
}

impl HistoryKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Erase => "erase",
            Self::Split => "split",
            Self::Merge => "merge",
            Self::Paste => "paste",
            Self::Incomplete => "incomplete",
        }
    }
}

struct History {
    kind: HistoryKind,
    base: String,
    base_edit: std::ops::Range<usize>,
    replacement: String,
}

#[test]
fn production_gfm_incremental_history_parity_is_complete() {
    let fixtures = gfm_fixtures();
    assert_eq!(fixtures.len(), EXPECTED_EXAMPLES);
    let mut local = 0_usize;
    let mut fallback = 0_usize;
    let mut fallback_examples = Vec::new();
    let mut maximum_source_bytes_read = 0_usize;
    let mut receipt = String::new();

    for fixture in fixtures {
        for history in histories_for(&fixture) {
            let mut target_check = history.base.clone();
            target_check.replace_range(history.base_edit.clone(), &history.replacement);
            assert_eq!(target_check, fixture.markdown, "GFM-{}", fixture.example);

            let mut runtime = DocumentRuntime::new(&history.base, DocumentRuntimeConfig::default())
                .unwrap_or_else(|error| panic!("GFM-{} base runtime: {error:?}", fixture.example));
            let session = build_session(
                &mut runtime,
                &format!("GFM-{} {} base", fixture.example, history.kind.name()),
            );
            let base_source = session.source();
            runtime
                .apply_edit(base_source, history.base_edit.clone(), &history.replacement)
                .unwrap_or_else(|error| panic!("GFM-{} apply history: {error:?}", fixture.example));

            let target_lease = runtime
                .snapshot_current_source()
                .unwrap_or_else(|error| panic!("GFM-{} target lease: {error:?}", fixture.example));
            let adoption =
                session.begin_local_adoption(&runtime, target_lease, history.base_edit.clone());

            let (mut superseded, mut target, path, source_bytes_read) =
                match adoption {
                    Ok(mut adoption) => {
                        let mut complete = false;
                        for poll_index in 0..1_000_000 {
                            let fuel = [7, 3, FUEL][poll_index % 3];
                            let poll = adoption.poll(&mut runtime, fuel).unwrap_or_else(|error| {
                                panic!(
                                    "GFM-{} {} adoption poll: {error:?}",
                                    fixture.example,
                                    history.kind.name()
                                )
                            });
                            assert!(poll.transitions() <= fuel, "GFM-{}", fixture.example);
                            match poll.status() {
                        M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                        M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                            complete = true;
                            break;
                        }
                        M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => break,
                        M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                            panic!("GFM-{} adoption cancelled", fixture.example)
                        }
                    }
                        }
                        if complete {
                            let mut update = adoption
                                .take_update()
                                .unwrap_or_else(|| panic!("GFM-{} update", fixture.example));
                            let work = update.work();
                            (
                                update.take_base().unwrap_or_else(|| {
                                    panic!("GFM-{} superseded base", fixture.example)
                                }),
                                update
                                    .take_target()
                                    .unwrap_or_else(|| panic!("GFM-{} target", fixture.example)),
                                "local",
                                work.source_bytes_read(),
                            )
                        } else {
                            adoption.begin_cancel(&mut runtime).unwrap_or_else(|error| {
                                panic!("GFM-{} fallback cancel: {error}", fixture.example)
                            });
                            while !adoption.poll_cancel(&mut runtime, FUEL).unwrap_or_else(
                                |error| panic!("GFM-{} fallback poll: {error}", fixture.example),
                            ) {}
                            let base = adoption
                                .take_base_after_cancel()
                                .unwrap_or_else(|| panic!("GFM-{} fallback base", fixture.example));
                            let clean_target = build_session(
                                &mut runtime,
                                &format!(
                                    "GFM-{} {} fallback target",
                                    fixture.example,
                                    history.kind.name()
                                ),
                            );
                            (base, clean_target, "clean-fallback", 0)
                        }
                    }
                    Err(failure) => {
                        let base = failure.into_base();
                        let clean_target = build_session(
                            &mut runtime,
                            &format!(
                                "GFM-{} {} start fallback target",
                                fixture.example,
                                history.kind.name()
                            ),
                        );
                        (base, clean_target, "clean-fallback", 0)
                    }
                };

            match path {
                "local" => local += 1,
                "clean-fallback" => {
                    fallback += 1;
                    fallback_examples.push((fixture.example, history.kind.name()));
                }
                _ => unreachable!(),
            }
            maximum_source_bytes_read = maximum_source_bytes_read.max(source_bytes_read);

            let incremental_digest = target
                .semantic_digest_for_diagnostics(&runtime)
                .unwrap_or_else(|error| {
                    panic!("GFM-{} incremental digest: {error}", fixture.example)
                });
            let incremental_references =
                target.reference_resolver(&runtime).unwrap_or_else(|error| {
                    panic!("GFM-{} incremental references: {error}", fixture.example)
                });

            let mut clean_runtime =
                DocumentRuntime::new(&fixture.markdown, DocumentRuntimeConfig::default())
                    .unwrap_or_else(|error| {
                        panic!("GFM-{} clean runtime: {error:?}", fixture.example)
                    });
            let mut clean = build_session(
                &mut clean_runtime,
                &format!("GFM-{} {} oracle", fixture.example, history.kind.name()),
            );
            let clean_digest = clean
                .semantic_digest_for_diagnostics(&clean_runtime)
                .unwrap_or_else(|error| panic!("GFM-{} clean digest: {error}", fixture.example));
            assert_eq!(incremental_digest, clean_digest, "GFM-{}", fixture.example);
            let clean_references =
                clean
                    .reference_resolver(&clean_runtime)
                    .unwrap_or_else(|error| {
                        panic!("GFM-{} clean references: {error}", fixture.example)
                    });
            assert_reference_parity(
                fixture.example,
                &fixture.markdown,
                &runtime,
                &incremental_references,
                &clean_runtime,
                &clean_references,
            );

            receipt.push_str(&format!(
                "GFM-{}\t{}\t{}\t{}\n",
                fixture.example,
                history.kind.name(),
                path,
                hex_digest(incremental_digest),
            ));

            drop(incremental_references);
            drop(clean_references);
            release_session(&mut clean_runtime, &mut clean);
            close_runtime(clean_runtime);
            release_session(&mut runtime, &mut superseded);
            release_session(&mut runtime, &mut target);
            close_runtime(runtime);
        }
    }

    let receipt_sha256 = sha256(receipt.as_bytes());
    eprintln!(
        "GFM incremental receipt: exact={} local={local} clean_fallback={fallback} fallback_examples={fallback_examples:?} max_local_source_bytes_read={maximum_source_bytes_read} receipt_sha256={receipt_sha256}",
        local + fallback,
    );
    assert_eq!(local + fallback, EXPECTED_EXAMPLES * HISTORIES_PER_EXAMPLE);
    assert_eq!(
        fallback_examples,
        [
            (168, "paste"),
            (545, "paste"),
            (553, "paste"),
            (560, "paste"),
            (568, "paste"),
        ],
        "only the pinned edit-before-reference-coverage cases may use clean fallback"
    );
    assert_eq!(
        receipt_sha256, "40b6b939da32781f413b9ed55960d4f23894139d4770503f5dd07eb4d490aa7f",
        "review and pin the complete deterministic incremental receipt",
    );
}

fn histories_for(fixture: &Fixture) -> Vec<History> {
    [
        HistoryKind::Type,
        HistoryKind::Erase,
        HistoryKind::Split,
        HistoryKind::Merge,
        HistoryKind::Paste,
        HistoryKind::Incomplete,
    ]
    .into_iter()
    .map(|kind| history_for(fixture, kind))
    .collect()
}

fn history_for(fixture: &Fixture, selected: HistoryKind) -> History {
    match selected {
        HistoryKind::Type => missing_span_history(fixture, HistoryKind::Type, 1),
        HistoryKind::Erase => inserted_history(fixture, HistoryKind::Erase, "x"),
        HistoryKind::Split => fixture
            .markdown
            .find('\n')
            .map(|offset| History {
                kind: HistoryKind::Split,
                base: format!(
                    "{}{}",
                    &fixture.markdown[..offset],
                    &fixture.markdown[offset + 1..]
                ),
                base_edit: offset..offset,
                replacement: "\n".into(),
            })
            .unwrap_or_else(|| missing_span_history(fixture, HistoryKind::Type, 1)),
        HistoryKind::Merge => inserted_history(fixture, HistoryKind::Merge, "\n"),
        HistoryKind::Paste => missing_span_history(fixture, HistoryKind::Paste, 16),
        HistoryKind::Incomplete => inserted_history(fixture, HistoryKind::Incomplete, "`"),
    }
}

fn missing_span_history(fixture: &Fixture, kind: HistoryKind, maximum_bytes: usize) -> History {
    let (start, end) = scalar_aligned_span(&fixture.markdown, maximum_bytes);
    if start == end {
        return inserted_history(fixture, HistoryKind::Incomplete, "`");
    }
    let mut base = fixture.markdown.clone();
    let replacement = base[start..end].to_owned();
    base.replace_range(start..end, "");
    History {
        kind,
        base,
        base_edit: start..start,
        replacement,
    }
}

fn inserted_history(fixture: &Fixture, kind: HistoryKind, insertion: &str) -> History {
    let offset = scalar_boundary_near_middle(&fixture.markdown);
    let mut base = fixture.markdown.clone();
    base.insert_str(offset, insertion);
    History {
        kind,
        base,
        base_edit: offset..offset + insertion.len(),
        replacement: String::new(),
    }
}

fn scalar_aligned_span(source: &str, maximum_bytes: usize) -> (usize, usize) {
    if source.is_empty() {
        return (0, 0);
    }
    let start = scalar_boundary_near_middle(source);
    let mut end = start;
    for (relative, character) in source[start..].char_indices() {
        let candidate = start + relative + character.len_utf8();
        if candidate - start > maximum_bytes && end != start {
            break;
        }
        end = candidate;
        if end - start >= maximum_bytes {
            break;
        }
    }
    if start == end {
        let mut previous = 0;
        for (offset, _) in source.char_indices() {
            if offset >= start {
                break;
            }
            previous = offset;
        }
        (previous, start)
    } else {
        (start, end)
    }
}

fn scalar_boundary_near_middle(source: &str) -> usize {
    let middle = source.len() / 2;
    source
        .char_indices()
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= middle)
        .last()
        .unwrap_or(0)
}

fn build_session(
    runtime: &mut DocumentRuntime,
    context: &str,
) -> M11PersistentRecursiveGreenSession {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime.snapshot_current_source().expect("scanner lease"),
        runtime.snapshot_current_source().expect("writer lease"),
        1,
    )
    .unwrap_or_else(|error| panic!("{context} clean plan: {error}"));
    let mut build = plan
        .begin(runtime)
        .unwrap_or_else(|error| panic!("{context} clean build start: {error}"));
    loop {
        let poll = build
            .poll(runtime, FUEL)
            .unwrap_or_else(|error| panic!("{context} clean build poll: {error:?}"));
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            return build.take_session().expect("GFM session");
        }
    }
}

fn release_session(
    runtime: &mut DocumentRuntime,
    session: &mut M11PersistentRecursiveGreenSession,
) {
    session
        .begin_release(runtime)
        .expect("begin session release");
    while !session
        .poll_release(runtime, FUEL)
        .expect("poll session release")
    {}
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime
        .poll_close(FUEL)
        .expect("poll runtime close")
        .complete
    {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_payload_bytes, 0);
}

fn assert_reference_parity(
    example: usize,
    source: &str,
    incremental_runtime: &DocumentRuntime,
    incremental: &M11ReferenceResolver,
    clean_runtime: &DocumentRuntime,
    clean: &M11ReferenceResolver,
) {
    for label in candidate_reference_labels(source) {
        let incremental = incremental
            .resolve(incremental_runtime, &label, 8 * 1024)
            .unwrap_or_else(|error| {
                panic!("GFM-{example} incremental reference {label:?}: {error}")
            });
        let clean = clean
            .resolve(clean_runtime, &label, 8 * 1024)
            .unwrap_or_else(|error| panic!("GFM-{example} clean reference {label:?}: {error}"));
        assert_eq!(incremental, clean, "GFM-{example} reference {label:?}");
        assert!(matches!(
            incremental,
            M11ReferenceResolution::Missing
                | M11ReferenceResolution::ValueTooLarge
                | M11ReferenceResolution::Resolved(_)
        ));
    }
}

fn candidate_reference_labels(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut labels = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'[' && !byte_is_escaped(bytes, index) {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && end - start <= 999 {
                if bytes[end] == b']' && !byte_is_escaped(bytes, end) {
                    if let Some(label) = source.get(start..end) {
                        let normalized =
                            comrak::block_spine_facade::normalize_reference_label(label);
                        if !normalized.is_empty() {
                            labels.insert(normalized);
                        }
                    }
                    break;
                }
                end += 1;
            }
        }
        index += 1;
    }
    labels.into_iter().collect()
}

fn byte_is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut count = 0;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        count += 1;
        cursor -= 1;
    }
    count % 2 != 0
}

fn gfm_fixtures() -> Vec<Fixture> {
    let mut fixtures = load_fixtures(GFM_FIXTURES);
    fixtures.extend([
        Fixture {
            example: 279,
            markdown: "- [ ] foo\n- [x] bar\n".into(),
        },
        Fixture {
            example: 280,
            markdown: "- [x] foo\n  - [ ] bar\n  - [x] baz\n- [ ] bim\n".into(),
        },
    ]);
    fixtures.sort_by_key(|fixture| fixture.example);
    fixtures
}

fn load_fixtures(json: &str) -> Vec<Fixture> {
    const MARKDOWN: &str = "    \"markdown\": ";
    const EXAMPLE: &str = "\n    \"example\": ";
    let mut fixtures = Vec::new();
    let mut cursor = 0;
    while let Some(markdown_field) = json[cursor..].find(MARKDOWN) {
        cursor += markdown_field + MARKDOWN.len();
        let markdown = json_string(json, &mut cursor);
        cursor += json[cursor..].find(EXAMPLE).expect("fixture example") + EXAMPLE.len();
        let end = cursor
            + json[cursor..]
                .find(|character: char| !character.is_ascii_digit())
                .expect("fixture example delimiter");
        let example = json[cursor..end].parse().expect("fixture example number");
        cursor = end;
        fixtures.push(Fixture { example, markdown });
    }
    fixtures
}

fn json_string(json: &str, cursor: &mut usize) -> String {
    let bytes = json.as_bytes();
    assert_eq!(bytes.get(*cursor), Some(&b'\"'));
    *cursor += 1;
    let mut output = String::new();
    let mut literal_start = *cursor;
    loop {
        match bytes.get(*cursor).copied().expect("terminated JSON string") {
            b'\"' => {
                output.push_str(&json[literal_start..*cursor]);
                *cursor += 1;
                return output;
            }
            b'\\' => {
                output.push_str(&json[literal_start..*cursor]);
                *cursor += 1;
                let escaped = bytes[*cursor];
                *cursor += 1;
                match escaped {
                    b'\"' => output.push('\"'),
                    b'\\' => output.push('\\'),
                    b'/' => output.push('/'),
                    b'b' => output.push('\u{0008}'),
                    b'f' => output.push('\u{000c}'),
                    b'n' => output.push('\n'),
                    b'r' => output.push('\r'),
                    b't' => output.push('\t'),
                    b'u' => output.push(decode_json_scalar(bytes, cursor)),
                    _ => panic!("invalid JSON escape"),
                }
                literal_start = *cursor;
            }
            _ => *cursor += 1,
        }
    }
}

fn decode_json_scalar(bytes: &[u8], cursor: &mut usize) -> char {
    let first = decode_hex_quad(bytes, cursor);
    let scalar = if (0xd800..=0xdbff).contains(&first) {
        assert_eq!(bytes.get(*cursor..*cursor + 2), Some(&b"\\u"[..]));
        *cursor += 2;
        let second = decode_hex_quad(bytes, cursor);
        0x1_0000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00)
    } else {
        u32::from(first)
    };
    char::from_u32(scalar).expect("valid JSON scalar")
}

fn decode_hex_quad(bytes: &[u8], cursor: &mut usize) -> u16 {
    let end = *cursor + 4;
    let digits = std::str::from_utf8(&bytes[*cursor..end]).expect("hex UTF-8");
    *cursor = end;
    u16::from_str_radix(digits, 16).expect("hex digits")
}

fn hex_digest(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
