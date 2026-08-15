// SPDX-License-Identifier: MIT

use std::{collections::BTreeMap, ops::Range};

use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};
use flark_parser::{
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
};
use serde::Deserialize;

const COMMONMARK_FIXTURES: &str =
    include_str!("../../../test/fixtures/commonmark/upstream/common_mark_tests.json");
const EXPECTED_EXAMPLES: usize = 652;
const POLL_FUEL: usize = 64;
const MAX_POLLS: usize = 1_000_000;

#[derive(Debug, Deserialize)]
struct Fixture {
    markdown: String,
    example: usize,
    section: String,
}

#[derive(Debug)]
enum AdoptionOutcome {
    Complete,
    CleanFallback,
    StartFailure,
    AdoptionPollFailure(String),
    BaseBuildFailure(String),
}

#[derive(Default)]
struct SectionCounts {
    total: usize,
    complete: usize,
    clean_fallback: usize,
    start_failure: usize,
    adoption_poll_failure: usize,
    base_build_failure: usize,
}

impl SectionCounts {
    fn record(&mut self, outcome: &AdoptionOutcome) {
        self.total += 1;
        match outcome {
            AdoptionOutcome::Complete => self.complete += 1,
            AdoptionOutcome::CleanFallback => self.clean_fallback += 1,
            AdoptionOutcome::StartFailure => self.start_failure += 1,
            AdoptionOutcome::AdoptionPollFailure(_) => self.adoption_poll_failure += 1,
            AdoptionOutcome::BaseBuildFailure(_) => self.base_build_failure += 1,
        }
    }
}

/// Wave-level diagnostic for how much of the pinned CommonMark corpus can use
/// the local recursive-Green transaction. This is intentionally ignored: it
/// is evidence for sequencing incremental-parser work, not routine CI policy.
#[test]
#[ignore = "wave-level 652-fixture incremental-adoption classifier"]
fn classify_commonmark_micro_edits_by_incremental_adoption_outcome() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(COMMONMARK_FIXTURES).expect("pinned CommonMark fixtures");
    assert_eq!(fixtures.len(), EXPECTED_EXAMPLES);

    let prefix = ordinary_padding("Prefix");
    let suffix = ordinary_padding("Suffix");
    assert!(prefix.len() > 4 * 1024);
    assert!(suffix.len() > 4 * 1024);

    let mut sections = BTreeMap::<String, SectionCounts>::new();
    let mut totals = SectionCounts::default();
    let mut digest_mismatches = Vec::new();
    let mut first_base_build_failure = None;
    let mut first_adoption_poll_failure = None;

    for (index, fixture) in fixtures.iter().enumerate() {
        assert_eq!(
            fixture.example,
            index + 1,
            "fixture inventory is contiguous"
        );
        let outcome = classify_fixture(fixture, &prefix, &suffix, &mut digest_mismatches);
        if let AdoptionOutcome::BaseBuildFailure(error) = &outcome {
            first_base_build_failure.get_or_insert_with(|| {
                format!("CM{} ({:?}): {error}", fixture.example, fixture.section)
            });
        }
        if let AdoptionOutcome::AdoptionPollFailure(error) = &outcome {
            first_adoption_poll_failure.get_or_insert_with(|| {
                format!("CM{} ({:?}): {error}", fixture.example, fixture.section)
            });
        }
        sections
            .entry(fixture.section.clone())
            .or_default()
            .record(&outcome);
        totals.record(&outcome);
    }

    for (section, counts) in &sections {
        println!(
            "commonmark_incrementality section={section:?} total={} complete={} clean_fallback={} start_failure={} adoption_poll_failure={} base_build_failure={}",
            counts.total,
            counts.complete,
            counts.clean_fallback,
            counts.start_failure,
            counts.adoption_poll_failure,
            counts.base_build_failure,
        );
    }
    println!(
        "commonmark_incrementality total={} complete={} clean_fallback={} start_failure={} adoption_poll_failure={} base_build_failure={}",
        totals.total,
        totals.complete,
        totals.clean_fallback,
        totals.start_failure,
        totals.adoption_poll_failure,
        totals.base_build_failure,
    );
    if let Some(failure) = first_base_build_failure {
        println!("commonmark_incrementality first_base_build_failure={failure}");
    }
    if let Some(failure) = first_adoption_poll_failure {
        println!("commonmark_incrementality first_adoption_poll_failure={failure}");
    }

    assert_eq!(totals.total, EXPECTED_EXAMPLES);
    assert!(
        digest_mismatches.is_empty(),
        "completed local adoptions diverged from clean target oracles: {}",
        digest_mismatches.join(", ")
    );
}

fn classify_fixture(
    fixture: &Fixture,
    prefix: &str,
    suffix: &str,
    digest_mismatches: &mut Vec<String>,
) -> AdoptionOutcome {
    let (local_edit, replacement) = select_ascii_micro_edit(&fixture.markdown);
    let mut source =
        String::with_capacity(prefix.len() + fixture.markdown.len() + 2 + suffix.len());
    source.push_str(prefix);
    let fixture_start = source.len();
    source.push_str(&fixture.markdown);
    source.push_str("\n\n");
    source.push_str(suffix);

    let edit = fixture_start + local_edit.start..fixture_start + local_edit.end;
    let mut target_source = source.clone();
    target_source.replace_range(edit.clone(), replacement);
    assert_eq!(source.len(), target_source.len(), "same-length micro-edit");

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .unwrap_or_else(|error| panic!("CM{} base runtime: {error}", fixture.example));
    let session = match try_build_session(&mut runtime, fixture.example, "base") {
        Ok(session) => session,
        Err(error) => {
            close_runtime(runtime, fixture.example, "base-build failure");
            return AdoptionOutcome::BaseBuildFailure(error);
        }
    };
    let base_source = session.source();
    runtime
        .apply_edit(base_source, edit.clone(), replacement)
        .unwrap_or_else(|error| panic!("CM{} apply micro-edit: {error}", fixture.example));
    let target_lease = runtime
        .snapshot_current_source()
        .unwrap_or_else(|error| panic!("CM{} target lease: {error}", fixture.example));

    let mut adoption = match session.begin_local_adoption(&runtime, target_lease, edit) {
        Ok(adoption) => adoption,
        Err(failure) => {
            let mut base = failure.into_base();
            release_session(
                &mut runtime,
                &mut base,
                fixture.example,
                "start-failure base",
            );
            close_runtime(runtime, fixture.example, "start-failure");
            return AdoptionOutcome::StartFailure;
        }
    };

    let mut status = None;
    for _ in 0..MAX_POLLS {
        let poll = match adoption.poll(&mut runtime, POLL_FUEL) {
            Ok(poll) => poll,
            Err(error) => {
                let failure = format!("poll adoption: {error}");
                adoption
                    .begin_cancel(&mut runtime)
                    .unwrap_or_else(|cancel_error| {
                        panic!(
                            "CM{} begin failed-adoption cancel: {cancel_error}",
                            fixture.example
                        )
                    });
                while !adoption
                    .poll_cancel(&mut runtime, POLL_FUEL)
                    .unwrap_or_else(|cancel_error| {
                        panic!(
                            "CM{} poll failed-adoption cancel: {cancel_error}",
                            fixture.example
                        )
                    })
                {}
                let mut base = adoption.take_base_after_cancel().unwrap_or_else(|| {
                    panic!("CM{} failed-adoption cancel omitted base", fixture.example)
                });
                release_session(
                    &mut runtime,
                    &mut base,
                    fixture.example,
                    "adoption-poll-failure base",
                );
                close_runtime(runtime, fixture.example, "adoption poll failure");
                return AdoptionOutcome::AdoptionPollFailure(failure);
            }
        };
        if poll.status() != M11PersistentRecursiveGreenAdoptionStatus::Pending {
            status = Some(poll.status());
            break;
        }
    }
    let status =
        status.unwrap_or_else(|| panic!("CM{} adoption exceeded poll guard", fixture.example));

    match status {
        M11PersistentRecursiveGreenAdoptionStatus::Complete => {
            let mut update = adoption
                .take_update()
                .unwrap_or_else(|| panic!("CM{} completed without update", fixture.example));
            let mut base = update
                .take_base()
                .unwrap_or_else(|| panic!("CM{} update omitted base", fixture.example));
            let mut target = update
                .take_target()
                .unwrap_or_else(|| panic!("CM{} update omitted target", fixture.example));
            let incremental_digest = target
                .semantic_digest_for_diagnostics(&runtime)
                .unwrap_or_else(|error| {
                    panic!("CM{} incremental semantic digest: {error}", fixture.example)
                });

            let mut clean_runtime =
                DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
                    .unwrap_or_else(|error| {
                        panic!("CM{} clean target runtime: {error}", fixture.example)
                    });
            match try_build_session(&mut clean_runtime, fixture.example, "clean target") {
                Ok(mut clean) => {
                    let clean_digest = clean
                        .semantic_digest_for_diagnostics(&clean_runtime)
                        .unwrap_or_else(|error| {
                            panic!("CM{} clean semantic digest: {error}", fixture.example)
                        });
                    if incremental_digest != clean_digest {
                        digest_mismatches
                            .push(format!("CM{} ({:?})", fixture.example, fixture.section));
                    }
                    release_session(
                        &mut clean_runtime,
                        &mut clean,
                        fixture.example,
                        "clean target",
                    );
                }
                Err(error) => digest_mismatches.push(format!(
                    "CM{} ({:?}) clean oracle unavailable: {error}",
                    fixture.example, fixture.section
                )),
            }
            close_runtime(clean_runtime, fixture.example, "clean target");
            release_session(&mut runtime, &mut base, fixture.example, "completed base");
            release_session(
                &mut runtime,
                &mut target,
                fixture.example,
                "completed target",
            );
            close_runtime(runtime, fixture.example, "completed adoption");
            AdoptionOutcome::Complete
        }
        M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
            adoption.begin_cancel(&mut runtime).unwrap_or_else(|error| {
                panic!("CM{} begin fallback cancel: {error}", fixture.example)
            });
            while !adoption
                .poll_cancel(&mut runtime, POLL_FUEL)
                .unwrap_or_else(|error| {
                    panic!("CM{} poll fallback cancel: {error}", fixture.example)
                })
            {}
            let mut base = adoption
                .take_base_after_cancel()
                .unwrap_or_else(|| panic!("CM{} fallback omitted base", fixture.example));
            release_session(&mut runtime, &mut base, fixture.example, "fallback base");
            close_runtime(runtime, fixture.example, "clean fallback");
            AdoptionOutcome::CleanFallback
        }
        M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
            panic!("CM{} adoption cancelled without request", fixture.example)
        }
        M11PersistentRecursiveGreenAdoptionStatus::Pending => unreachable!("terminal status"),
    }
}

fn select_ascii_micro_edit(markdown: &str) -> (Range<usize>, &'static str) {
    let bytes = markdown.as_bytes();
    let center = bytes.len() / 2;
    let index = nearest_index(bytes, center, u8::is_ascii_alphanumeric)
        .or_else(|| {
            nearest_index(bytes, center, |byte| {
                byte.is_ascii() && !matches!(*byte, b'\n' | b'\r')
            })
        })
        .expect("every pinned fixture has an editable ASCII byte");
    let replacement = match bytes[index] {
        b'a'..=b'z' => "A",
        b'A'..=b'Z' => "a",
        b'0'..=b'8' => "9",
        b'9' => "0",
        b' ' | b'\t' => "!",
        b'~' => "!",
        _ => "~",
    };
    (index..index + 1, replacement)
}

fn nearest_index(bytes: &[u8], center: usize, predicate: impl Fn(&u8) -> bool) -> Option<usize> {
    (0..bytes.len()).find_map(|distance| {
        let after = center
            .checked_add(distance)
            .filter(|index| *index < bytes.len());
        let before = center.checked_sub(distance);
        after
            .filter(|index| predicate(&bytes[*index]))
            .or_else(|| before.filter(|index| predicate(&bytes[*index])))
    })
}

fn ordinary_padding(label: &str) -> String {
    let mut padding = String::new();
    for ordinal in 0..96 {
        padding.push_str(&format!(
            "{label} ordinary paragraph {ordinal:03} keeps distant document structure stable.\n\n"
        ));
    }
    padding
}

fn try_build_session(
    runtime: &mut DocumentRuntime,
    example: usize,
    label: &str,
) -> Result<M11PersistentRecursiveGreenSession, String> {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime
            .snapshot_current_source()
            .unwrap_or_else(|error| panic!("CM{example} {label} scanner lease: {error}")),
        runtime
            .snapshot_current_source()
            .unwrap_or_else(|error| panic!("CM{example} {label} writer lease: {error}")),
        1,
    )
    .unwrap_or_else(|error| panic!("CM{example} {label} clean plan: {error}"));
    let mut build = plan
        .begin(runtime)
        .unwrap_or_else(|error| panic!("CM{example} {label} begin build: {error}"));
    for _ in 0..MAX_POLLS {
        let poll = match build.poll(runtime, POLL_FUEL) {
            Ok(poll) => poll,
            Err(error) => {
                let failure = format!("{label} poll build: {error}");
                build.begin_cancel(runtime).unwrap_or_else(|cancel_error| {
                    panic!("CM{example} {label} begin failed-build cancel: {cancel_error}")
                });
                loop {
                    let cancel =
                        build
                            .poll_cancel(runtime, POLL_FUEL)
                            .unwrap_or_else(|cancel_error| {
                                panic!(
                                    "CM{example} {label} poll failed-build cancel: {cancel_error}"
                                )
                            });
                    if cancel.status() == M11PersistentRecursiveGreenBuildStatus::Cancelled {
                        break;
                    }
                }
                return Err(failure);
            }
        };
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            return Ok(build
                .take_session()
                .unwrap_or_else(|| panic!("CM{example} {label} build omitted session")));
        }
    }
    build.begin_cancel(runtime).unwrap_or_else(|cancel_error| {
        panic!("CM{example} {label} begin guarded-build cancel: {cancel_error}")
    });
    while build
        .poll_cancel(runtime, POLL_FUEL)
        .unwrap_or_else(|cancel_error| {
            panic!("CM{example} {label} poll guarded-build cancel: {cancel_error}")
        })
        .status()
        != M11PersistentRecursiveGreenBuildStatus::Cancelled
    {}
    Err(format!("{label} build exceeded poll guard"))
}

fn release_session(
    runtime: &mut DocumentRuntime,
    session: &mut M11PersistentRecursiveGreenSession,
    example: usize,
    label: &str,
) {
    session
        .begin_release(runtime)
        .unwrap_or_else(|error| panic!("CM{example} {label} begin release: {error}"));
    while !session
        .poll_release(runtime, POLL_FUEL)
        .unwrap_or_else(|error| panic!("CM{example} {label} poll release: {error}"))
    {}
}

fn close_runtime(mut runtime: DocumentRuntime, example: usize, label: &str) {
    runtime
        .begin_close()
        .unwrap_or_else(|error| panic!("CM{example} {label} begin runtime close: {error}"));
    while !runtime
        .poll_close(POLL_FUEL)
        .unwrap_or_else(|error| panic!("CM{example} {label} poll runtime close: {error}"))
        .complete
    {}
    let metrics = runtime.arena_metrics();
    assert_eq!(
        metrics.resident_nodes, 0,
        "CM{example} {label} leaked nodes"
    );
    assert_eq!(
        metrics.live_payload_bytes, 0,
        "CM{example} {label} leaked payload"
    );
    assert_eq!(
        metrics.reserved_external_payload_bytes, 0,
        "CM{example} {label} leaked reserved payload"
    );
    assert_eq!(metrics.live_builds, 0, "CM{example} {label} leaked builds");
    assert_eq!(
        metrics.pending_build_aborts, 0,
        "CM{example} {label} leaked build aborts"
    );
    assert_eq!(
        metrics.pending_reclaims, 0,
        "CM{example} {label} leaked pending reclaims"
    );
}
