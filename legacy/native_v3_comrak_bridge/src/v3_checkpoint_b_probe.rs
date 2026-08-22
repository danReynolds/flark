//! Private one-shot evidence probe for Feedback Checkpoint B.
//!
//! This is not a product protocol. It drives the production document actor,
//! exact source lineage, persistent SourceFacts sequence, cancellation, and
//! reclamation paths, then serializes bounded review evidence for the native
//! isolate and Web Worker demos.

use std::fmt::Write as _;
use std::ops::{Deref, DerefMut, Range};

use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState,
    IncrementalSourceFactsPlan, ParserProfileId, PersistentSourceFactsPageInfo,
    PersistentSourceFactsWork, RuntimeSourceFactsPoll, SourceFactsRootLimits,
    SourceFactsScanProfile,
};
use serde::Serialize;

const PROBE_SCHEMA: u32 = 1;
const CHECKPOINT_SPACING_UTF16: usize = 4;
const PARSER_PROFILE: u64 = 0x23;
const CLEAN_SOURCE_BYTES_PER_POLL: usize = 128;
const CLEAN_CHECKPOINTS_PER_POLL: usize = 64;
const INCREMENTAL_SOURCE_BYTES_PER_POLL: usize = 19;
const INCREMENTAL_CHECKPOINTS_PER_POLL: usize = 5;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeReceipt {
    schema: u32,
    platform: &'static str,
    parity_digest: String,
    fixture_bytes: usize,
    steps: Vec<EditStepReceipt>,
    lifecycle: LifecycleReceipt,
    closed_resident_nodes: usize,
    closed_live_builds: usize,
    closed_pending_reclaims: usize,
    all_checks_passed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditStepReceipt {
    label: &'static str,
    edit_start: usize,
    edit_end: usize,
    replacement_bytes: usize,
    base_revision: u64,
    target_revision: u64,
    base_pages: usize,
    target_pages: usize,
    clean_oracle_pages: usize,
    crop_byte_start: usize,
    crop_byte_end: usize,
    crop_bytes: usize,
    base_page_start: usize,
    base_page_end: usize,
    lineage_transitions: usize,
    summary_matches_clean: bool,
    absolute_terminal_matches_root: bool,
    root_fingerprint: String,
    work: WorkReceipt,
    before_pages: Vec<PageReceipt>,
    after_pages: Vec<PageReceipt>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkReceipt {
    scanned_bytes: usize,
    planning_node_headers: u64,
    planning_payload_bytes: u64,
    planning_summary_combinations: u64,
    promotion_node_headers: u64,
    promotion_payload_bytes: u64,
    promotion_checkpoints_hashed: u64,
    promotion_summary_combinations: u64,
    leaves_adopted: usize,
    branches_allocated: usize,
    nodes_visited: usize,
    leaves_reused: usize,
    committed_leaves_retained: usize,
    leaves_deleted: usize,
    maximum_atomic_height: u16,
    seal_transitions: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageReceipt {
    ordinal: usize,
    id: String,
    digest: String,
    checkpoint_count: usize,
    first_byte: u64,
    terminal_byte: u64,
    terminal_utf16: u64,
    terminal_lines: u64,
    classification: &'static str,
    base_ordinal: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleReceipt {
    cancellation_reached_promotion: bool,
    base_restored_after_cancellation: bool,
    cancelled_build_reclaimed: bool,
    nearby_rapid_edit_lineage_transitions: usize,
    nearby_rapid_edit_matches_clean: bool,
    distant_edit_reuse_rejected: bool,
    failed_reuse_preserved_base: bool,
    clean_fallback_reached_target: bool,
    closed_to_zero: bool,
}

struct RuntimeGuard(Option<DocumentRuntime>);

impl RuntimeGuard {
    fn new(source: &str) -> Result<Self, String> {
        DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .map(|runtime| Self(Some(runtime)))
            .map_err(|error| probe_error("create document runtime", error))
    }

    fn close(&mut self) -> Result<(), String> {
        let runtime = self.0.as_mut().ok_or("runtime already closed")?;
        close_runtime(runtime)?;
        Ok(())
    }
}

impl Deref for RuntimeGuard {
    type Target = DocumentRuntime;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("probe runtime remains present")
    }
}

impl DerefMut for RuntimeGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("probe runtime remains present")
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        let Some(runtime) = self.0.as_mut() else {
            return;
        };
        if close_runtime(runtime).is_err() {
            let leaked = self.0.take();
            std::mem::forget(leaked);
        }
    }
}

pub(crate) fn encode_probe_json() -> Result<Vec<u8>, String> {
    serde_json::to_vec(&run_probe()?).map_err(|error| probe_error("encode probe JSON", error))
}

fn run_probe() -> Result<ProbeReceipt, String> {
    let profile = SourceFactsScanProfile::new(CHECKPOINT_SPACING_UTF16)
        .map_err(|error| probe_error("create SourceFacts profile", error))?;
    let parser_profile =
        ParserProfileId::new(PARSER_PROFILE).ok_or("invalid parser profile identity")?;
    let limits = SourceFactsRootLimits::default();
    let mut text = "# heading\r\nalpha **bold** 😀 beta\n".repeat(140);
    let fixture_bytes = text.len();
    let mut runtime = RuntimeGuard::new(&text)?;
    complete_clean(&mut runtime, profile, parser_profile, limits)?;
    drop(
        runtime
            .take_certified_source()
            .ok_or("clean probe did not produce legacy certification")?,
    );

    let mut steps = Vec::new();
    for shape in 0..4 {
        let (label, range, replacement) = edit_shape(shape, &text)?;
        let before_info = runtime
            .persistent_source_facts()
            .ok_or("probe has no persistent SourceFacts base")?;
        let before_pages = page_receipts(&runtime, "base", None)?;
        let base_revision = before_info.source().revision().get();

        text.replace_range(range.clone(), replacement);
        let target = runtime
            .apply_edit(before_info.source(), range.clone(), replacement)
            .map_err(|error| probe_error("apply probe edit", error))?
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(profile, parser_profile, limits)
            .map_err(|error| probe_error("plan incremental SourceFacts", error))?;
        let (scanned_bytes, work, replacement_pages) = complete_incremental(&mut runtime)?;
        let after_info = runtime
            .persistent_source_facts()
            .ok_or("incremental probe produced no persistent root")?;
        if after_info.source() != target {
            return Err("incremental probe installed the wrong source authority".into());
        }

        let base_page_start = usize::try_from(plan.base_page_range().start)
            .map_err(|_| "base page start exceeds host representation")?;
        let base_page_end = usize::try_from(plan.base_page_range().end)
            .map_err(|_| "base page end exceeds host representation")?;
        let after_pages = classified_page_receipts(
            &runtime,
            &before_pages,
            base_page_start,
            base_page_end,
            replacement_pages,
        )?;

        let mut oracle = RuntimeGuard::new(&text)?;
        complete_clean(&mut oracle, profile, parser_profile, limits)?;
        let oracle_info = oracle
            .persistent_source_facts()
            .ok_or("clean oracle produced no persistent root")?;
        let summary_matches_clean = after_info.summary() == oracle_info.summary();
        let terminal = after_pages.last().ok_or("nonempty probe lost all pages")?;
        let summary = after_info.summary();
        let absolute_terminal_matches_root = terminal.terminal_byte == summary.byte_len()
            && terminal.terminal_utf16 == summary.utf16_len()
            && terminal.terminal_lines == summary.logical_line_breaks();
        let step = EditStepReceipt {
            label,
            edit_start: range.start,
            edit_end: range.end,
            replacement_bytes: replacement.len(),
            base_revision,
            target_revision: target.revision().get(),
            base_pages: usize::try_from(before_info.page_count())
                .map_err(|_| "base page count exceeds host representation")?,
            target_pages: usize::try_from(after_info.page_count())
                .map_err(|_| "target page count exceeds host representation")?,
            clean_oracle_pages: usize::try_from(oracle_info.page_count())
                .map_err(|_| "oracle page count exceeds host representation")?,
            crop_byte_start: plan.target_byte_range().start,
            crop_byte_end: plan.target_byte_range().end,
            crop_bytes: plan.target_byte_range().end - plan.target_byte_range().start,
            base_page_start,
            base_page_end,
            lineage_transitions: plan.lineage_transitions(),
            summary_matches_clean,
            absolute_terminal_matches_root,
            root_fingerprint: fingerprint(summary.rolling_hash().words()),
            work: work_receipt(plan, scanned_bytes, work),
            before_pages,
            after_pages,
        };
        if !step.summary_matches_clean || !step.absolute_terminal_matches_root {
            return Err(format!("{} failed its clean equality proof", step.label));
        }
        steps.push(step);
        oracle.close()?;
        drain_retirement(&mut runtime);
    }

    let lifecycle = lifecycle_receipt(profile, parser_profile, limits)?;
    runtime.close()?;
    let metrics = runtime.arena_metrics();
    let all_checks_passed = steps
        .iter()
        .all(|step| step.summary_matches_clean && step.absolute_terminal_matches_root)
        && lifecycle.cancellation_reached_promotion
        && lifecycle.base_restored_after_cancellation
        && lifecycle.cancelled_build_reclaimed
        && lifecycle.nearby_rapid_edit_matches_clean
        && lifecycle.distant_edit_reuse_rejected
        && lifecycle.failed_reuse_preserved_base
        && lifecycle.clean_fallback_reached_target
        && lifecycle.closed_to_zero
        && metrics.resident_nodes == 0
        && metrics.live_builds == 0
        && metrics.pending_reclaims == 0;
    let parity_digest = parity_digest(
        fixture_bytes,
        &steps,
        &lifecycle,
        metrics.resident_nodes,
        metrics.live_builds,
        metrics.pending_reclaims,
    )?;
    Ok(ProbeReceipt {
        schema: PROBE_SCHEMA,
        platform: if cfg!(target_arch = "wasm32") {
            "wasm"
        } else {
            "native"
        },
        parity_digest,
        fixture_bytes,
        steps,
        lifecycle,
        closed_resident_nodes: metrics.resident_nodes,
        closed_live_builds: metrics.live_builds,
        closed_pending_reclaims: metrics.pending_reclaims,
        all_checks_passed,
    })
}

fn lifecycle_receipt(
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    limits: SourceFactsRootLimits,
) -> Result<LifecycleReceipt, String> {
    let source = "alpha **bold** 😀\r\nbeta\n".repeat(120);
    let mut text = source.clone();
    let mut runtime = RuntimeGuard::new(&text)?;
    complete_clean(&mut runtime, profile, parser_profile, limits)?;
    let base = runtime
        .persistent_source_facts()
        .ok_or("lifecycle probe has no base")?;
    let baseline_nodes = runtime.arena_metrics().resident_nodes;
    drop(
        runtime
            .take_certified_source()
            .ok_or("lifecycle probe has no clean certification")?,
    );

    let first_start = text
        .match_indices("**bold**")
        .nth(60)
        .ok_or("lifecycle probe has no middle edit")?
        .0;
    let first_range = first_start..first_start + "**bold**".len();
    let first_replacement = "**stronger markdown**";
    text.replace_range(first_range.clone(), first_replacement);
    let first_target = runtime
        .apply_edit(base.source(), first_range, first_replacement)
        .map_err(|error| probe_error("apply lifecycle first edit", error))?
        .source()
        .current();
    runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .map_err(|error| probe_error("begin cancellable increment", error))?;
    let mut scan_complete = false;
    let cancellation_reached_promotion = loop {
        match runtime
            .poll_source_facts(
                INCREMENTAL_SOURCE_BYTES_PER_POLL,
                INCREMENTAL_CHECKPOINTS_PER_POLL,
            )
            .map_err(|error| probe_error("poll cancellable increment", error))?
        {
            RuntimeSourceFactsPoll::PromotionPending { transitions: 1 } if scan_complete => {
                break true;
            }
            RuntimeSourceFactsPoll::PromotionPending { .. } => {}
            RuntimeSourceFactsPoll::Pending(_) => {}
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => scan_complete = true,
            RuntimeSourceFactsPoll::IncrementalComplete { .. } => break false,
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                return Err("incremental lifecycle probe reported clean progress".into());
            }
        }
    };

    let second_start = text
        .find("stronger")
        .ok_or("lifecycle replacement disappeared")?
        + "strong".len();
    text.insert_str(second_start, "_live_");
    let second_target = runtime
        .apply_edit(first_target, second_start..second_start, "_live_")
        .map_err(|error| probe_error("apply cancelling edit", error))?
        .source()
        .current();
    let base_restored_after_cancellation = runtime
        .persistent_source_facts()
        .is_some_and(|info| info.source() == base.source());
    drain_retirement(&mut runtime);
    let cancelled_build_reclaimed = runtime.arena_metrics().resident_nodes == baseline_nodes
        && runtime.arena_metrics().live_builds == 0;

    let rapid_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .map_err(|error| probe_error("plan nearby rapid edit", error))?;
    let nearby_rapid_edit_lineage_transitions = rapid_plan.lineage_transitions();
    let _ = complete_incremental(&mut runtime)?;
    let mut rapid_oracle = RuntimeGuard::new(&text)?;
    complete_clean(&mut rapid_oracle, profile, parser_profile, limits)?;
    let nearby_rapid_edit_matches_clean = runtime
        .persistent_source_facts()
        .zip(rapid_oracle.persistent_source_facts())
        .is_some_and(|(incremental, clean)| incremental.summary() == clean.summary());
    rapid_oracle.close()?;

    let prefix_target = runtime
        .apply_edit(second_target, 0..0, "prefix ")
        .map_err(|error| probe_error("apply distant prefix edit", error))?
        .source()
        .current();
    text.insert_str(0, "prefix ");
    let tail = prefix_target.byte_len();
    let final_target = runtime
        .apply_edit(prefix_target, tail..tail, " suffix")
        .map_err(|error| probe_error("apply distant suffix edit", error))?
        .source()
        .current();
    text.push_str(" suffix");
    let preserved_base = runtime
        .persistent_source_facts()
        .ok_or("distant-edit probe lost its base")?
        .source();
    let distant_edit_reuse_rejected = matches!(
        runtime.begin_incremental_source_facts(profile, parser_profile, limits),
        Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)
    );
    let failed_reuse_preserved_base = runtime
        .persistent_source_facts()
        .is_some_and(|info| info.source() == preserved_base);
    complete_clean(&mut runtime, profile, parser_profile, limits)?;
    let clean_fallback_reached_target = runtime
        .persistent_source_facts()
        .is_some_and(|info| info.source() == final_target);
    runtime.close()?;
    let closed = runtime.arena_metrics();
    Ok(LifecycleReceipt {
        cancellation_reached_promotion,
        base_restored_after_cancellation,
        cancelled_build_reclaimed,
        nearby_rapid_edit_lineage_transitions,
        nearby_rapid_edit_matches_clean,
        distant_edit_reuse_rejected,
        failed_reuse_preserved_base,
        clean_fallback_reached_target,
        closed_to_zero: closed.resident_nodes == 0
            && closed.live_builds == 0
            && closed.pending_reclaims == 0,
    })
}

fn complete_clean(
    runtime: &mut DocumentRuntime,
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    limits: SourceFactsRootLimits,
) -> Result<(), String> {
    runtime
        .begin_source_facts(profile, parser_profile, limits)
        .map_err(|error| probe_error("begin clean SourceFacts", error))?;
    loop {
        match runtime
            .poll_source_facts(CLEAN_SOURCE_BYTES_PER_POLL, CLEAN_CHECKPOINTS_PER_POLL)
            .map_err(|error| probe_error("poll clean SourceFacts", error))?
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => return Ok(()),
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                return Err("clean probe reported incremental progress".into());
            }
        }
    }
}

fn complete_incremental(
    runtime: &mut DocumentRuntime,
) -> Result<(usize, PersistentSourceFactsWork, usize), String> {
    let mut scanned_bytes = 0_usize;
    loop {
        match runtime
            .poll_source_facts(
                INCREMENTAL_SOURCE_BYTES_PER_POLL,
                INCREMENTAL_CHECKPOINTS_PER_POLL,
            )
            .map_err(|error| probe_error("poll incremental SourceFacts", error))?
        {
            RuntimeSourceFactsPoll::Pending(work) => {
                scanned_bytes = scanned_bytes
                    .checked_add(work.source_bytes_examined())
                    .ok_or("incremental scan accounting overflow")?;
            }
            RuntimeSourceFactsPoll::PromotionPending { .. } => {}
            RuntimeSourceFactsPoll::IncrementalScanComplete { work, .. } => {
                scanned_bytes = scanned_bytes
                    .checked_add(work.source_bytes_examined())
                    .ok_or("incremental scan accounting overflow")?;
            }
            RuntimeSourceFactsPoll::IncrementalComplete { work, witness, .. } => {
                let witness = runtime
                    .take_persistent_source_facts_delta(witness)
                    .map_err(|error| probe_error("consume incremental witness", error))?;
                let target = witness.target();
                let replacement_pages = usize::try_from(
                    witness.target_page_range().end - witness.target_page_range().start,
                )
                .map_err(|_| "replacement page count exceeds host representation")?;
                drop(witness);
                if !runtime
                    .commit_persistent_source_facts_delta(target)
                    .map_err(|error| probe_error("commit acknowledged incremental target", error))?
                {
                    return Err(
                        "acknowledged incremental target had no pending SourceFacts transaction"
                            .into(),
                    );
                }
                return Ok((scanned_bytes, work, replacement_pages));
            }
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                return Err("incremental probe reported clean progress".into());
            }
        }
    }
}

fn edit_shape(
    shape: usize,
    text: &str,
) -> Result<(&'static str, Range<usize>, &'static str), String> {
    match shape {
        0 => Ok(("prefix insertion", 0..0, "<!-- prefix -->\n")),
        1 => {
            let start = text
                .match_indices('😀')
                .nth(70)
                .ok_or("probe fixture has no middle Unicode scalar")?
                .0;
            Ok((
                "middle Unicode replacement",
                start..start + '😀'.len_utf8(),
                "🌍✨",
            ))
        }
        2 => Ok(("tail insertion", text.len()..text.len(), "\n<!-- tail -->")),
        3 => {
            let split = text
                .match_indices("\r\n")
                .nth(90)
                .ok_or("probe fixture has no split CRLF boundary")?
                .0
                + 1;
            Ok(("split CRLF insertion", split..split, "X"))
        }
        _ => Err("unknown Checkpoint B edit shape".into()),
    }
}

fn page_receipts(
    runtime: &DocumentRuntime,
    classification: &'static str,
    base_ordinal: Option<usize>,
) -> Result<Vec<PageReceipt>, String> {
    let info = runtime
        .persistent_source_facts()
        .ok_or("persistent SourceFacts root is absent")?;
    let page_count = usize::try_from(info.page_count())
        .map_err(|_| "persistent page count exceeds host representation")?;
    let mut pages = Vec::new();
    pages
        .try_reserve_exact(page_count)
        .map_err(|_| "page receipt allocation failed")?;
    for ordinal in 0..page_count {
        let page = runtime
            .persistent_source_facts_page(
                u64::try_from(ordinal).map_err(|_| "page ordinal exceeds u64")?,
            )
            .map_err(|error| probe_error("project persistent page", error))?
            .ok_or("persistent page ordinal is missing")?;
        pages.push(page_receipt(
            page,
            classification,
            base_ordinal.or(Some(ordinal)),
        ));
    }
    Ok(pages)
}

fn classified_page_receipts(
    runtime: &DocumentRuntime,
    before: &[PageReceipt],
    old_start: usize,
    old_end: usize,
    replacement_pages: usize,
) -> Result<Vec<PageReceipt>, String> {
    let info = runtime
        .persistent_source_facts()
        .ok_or("updated persistent SourceFacts root is absent")?;
    let page_count =
        usize::try_from(info.page_count()).map_err(|_| "updated page count exceeds host")?;
    let new_suffix_start = old_start
        .checked_add(replacement_pages)
        .ok_or("updated suffix ordinal overflow")?;
    let mut pages = Vec::new();
    pages
        .try_reserve_exact(page_count)
        .map_err(|_| "updated page receipt allocation failed")?;
    for ordinal in 0..page_count {
        let (classification, base_ordinal) = if ordinal < old_start {
            ("retained prefix", Some(ordinal))
        } else if ordinal < new_suffix_start {
            ("rescanned replacement", None)
        } else {
            let old = old_end
                .checked_add(ordinal - new_suffix_start)
                .ok_or("retained suffix ordinal overflow")?;
            ("retained suffix", Some(old))
        };
        let page = runtime
            .persistent_source_facts_page(
                u64::try_from(ordinal).map_err(|_| "updated page ordinal exceeds u64")?,
            )
            .map_err(|error| probe_error("project updated persistent page", error))?
            .ok_or("updated persistent page ordinal is missing")?;
        let receipt = page_receipt(page, classification, base_ordinal);
        if let Some(base_ordinal) = base_ordinal {
            let base = before
                .get(base_ordinal)
                .ok_or("retained page points outside its base")?;
            if receipt.id != base.id || receipt.digest != base.digest {
                return Err("retained page identity or canonical bytes changed".into());
            }
        }
        pages.push(receipt);
    }
    Ok(pages)
}

fn page_receipt(
    page: PersistentSourceFactsPageInfo,
    classification: &'static str,
    base_ordinal: Option<usize>,
) -> PageReceipt {
    let id = page.id();
    let first = page.first_checkpoint();
    let terminal = page.terminal_checkpoint();
    PageReceipt {
        ordinal: usize::try_from(page.ordinal()).unwrap_or(usize::MAX),
        id: format!("{}:{}:{}", id.arena().get(), id.slot(), id.generation()),
        digest: hex(page.content_digest().bytes()),
        checkpoint_count: page.checkpoint_count(),
        first_byte: first.byte_offset(),
        terminal_byte: terminal.byte_offset(),
        terminal_utf16: terminal.utf16_offset(),
        terminal_lines: terminal.logical_line_breaks(),
        classification,
        base_ordinal,
    }
}

fn work_receipt(
    plan: IncrementalSourceFactsPlan,
    scanned_bytes: usize,
    work: PersistentSourceFactsWork,
) -> WorkReceipt {
    let planning = plan.planning_work();
    WorkReceipt {
        scanned_bytes,
        planning_node_headers: planning.node_headers_decoded(),
        planning_payload_bytes: planning.payload_bytes_inspected(),
        planning_summary_combinations: planning.summary_combinations(),
        promotion_node_headers: work.node_headers_decoded(),
        promotion_payload_bytes: work.payload_bytes_inspected(),
        promotion_checkpoints_hashed: work.checkpoints_hashed(),
        promotion_summary_combinations: work.summary_combinations(),
        leaves_adopted: work.leaves_adopted(),
        branches_allocated: work.branches_allocated(),
        nodes_visited: work.nodes_visited(),
        leaves_reused: work.leaves_reused(),
        committed_leaves_retained: work.committed_leaves_retained(),
        leaves_deleted: work.leaves_deleted(),
        maximum_atomic_height: work.maximum_atomic_height(),
        seal_transitions: work.seal_transitions(),
    }
}

fn fingerprint(words: [u32; 4]) -> String {
    format!(
        "{:08x}{:08x}{:08x}{:08x}",
        words[0], words[1], words[2], words[3]
    )
}

fn hex(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn parity_digest(
    fixture_bytes: usize,
    steps: &[EditStepReceipt],
    lifecycle: &LifecycleReceipt,
    closed_resident_nodes: usize,
    closed_live_builds: usize,
    closed_pending_reclaims: usize,
) -> Result<String, String> {
    // Arena identities are process-local witnesses. Everything else in this
    // projection must agree byte-for-byte between native and Wasm executions.
    let mut projection = serde_json::to_value((
        PROBE_SCHEMA,
        fixture_bytes,
        steps,
        lifecycle,
        closed_resident_nodes,
        closed_live_builds,
        closed_pending_reclaims,
    ))
    .map_err(|error| probe_error("encode Checkpoint B parity projection", error))?;
    remove_arena_identities(&mut projection);
    let encoded = serde_json::to_vec(&projection)
        .map_err(|error| probe_error("serialize Checkpoint B parity projection", error))?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

fn remove_arena_identities(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                remove_arena_identities(value);
            }
        }
        serde_json::Value::Object(fields) => {
            fields.remove("id");
            for value in fields.values_mut() {
                remove_arena_identities(value);
            }
        }
        _ => {}
    }
}

fn drain_retirement(runtime: &mut DocumentRuntime) {
    while !runtime.poll_retirement(1).complete {}
}

fn close_runtime(runtime: &mut DocumentRuntime) -> Result<(), String> {
    if runtime.state() == DocumentState::Open {
        runtime
            .begin_close()
            .map_err(|error| probe_error("begin probe close", error))?;
    }
    while runtime.state() != DocumentState::Closed {
        runtime
            .poll_close(8)
            .map_err(|error| probe_error("poll probe close", error))?;
    }
    Ok(())
}

fn probe_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_b_probe_proves_all_edit_and_lifecycle_rows() {
        let receipt = run_probe().expect("Checkpoint B probe");
        assert_eq!(receipt.steps.len(), 4);
        assert!(receipt.all_checks_passed);
        assert_eq!(receipt.parity_digest.len(), 64);
        assert_eq!(receipt.closed_resident_nodes, 0);
        assert_eq!(receipt.closed_live_builds, 0);
        assert_eq!(receipt.closed_pending_reclaims, 0);
        assert!(receipt
            .steps
            .iter()
            .all(|step| step.work.scanned_bytes == step.crop_bytes));
        assert!(receipt.steps.iter().all(|step| {
            step.work.leaves_reused == step.base_pages - (step.base_page_end - step.base_page_start)
        }));
    }

    #[test]
    fn encoded_probe_is_bounded_valid_json() {
        let encoded = encode_probe_json().expect("encode probe");
        assert!(encoded.len() < 64 * 1024);
        let decoded: serde_json::Value = serde_json::from_slice(&encoded).expect("probe JSON");
        assert_eq!(decoded["schema"], PROBE_SCHEMA);
        assert_eq!(decoded["allChecksPassed"], true);
    }
}
