use flark_engine::parser_internal::M11RecursiveGreenRoot;
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    BlockCommand, BlockKind, M11BlockRestartCheckpoint, M11BlockStructuralAdoptionReceipt,
    M11BlockTerminalConvergenceCheckpoint, M11BlockWriter, M11BlockWriterOfferStatus,
    M11BlockWriterPollStatus, M11DirectBlockController, M11DirectBlockPollStatus,
};
use flark_parser::{
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource,
};

const FUEL: usize = 17;
const UNITS: usize = 4_096;

struct Fixture {
    source: String,
    edit: std::ops::Range<usize>,
    restart_parser_cut: usize,
    convergence_parser_cut: usize,
}

fn mixed_fixture() -> Fixture {
    let mut source = String::new();
    let target = UNITS / 2;
    let mut edit = None;
    let mut restart_parser_cut = 0;
    let mut convergence_parser_cut = 0;
    for ordinal in 0..UNITS {
        source.push_str(&format!("# section {ordinal}\n\n"));
        source.push_str(&format!(
            "> quote {ordinal}\n> continued **bold** {ordinal}\n\n"
        ));
        source.push_str(&format!(
            "- item {ordinal}\n  - nested {ordinal}\n- tail {ordinal}\n\n"
        ));
        source.push_str(&format!("```rust\nlet n = {ordinal};\n```\n\n"));
        source.push_str(&format!(
            "<div data-n=\"{ordinal}\">\nhtml {ordinal}\n</div>\n\n"
        ));
        source.push_str("---\n\n");
        if ordinal == target {
            restart_parser_cut = source.len();
            let start = source.len();
            source.push_str(&format!(
                "ordinary paragraph {ordinal}\ncontinued paragraph {ordinal}\n"
            ));
            let end = source.len();
            source.push('\n');
            convergence_parser_cut = source.len();
            edit = Some(start..end);
        } else {
            source.push_str(&format!(
                "ordinary paragraph {ordinal}\ncontinued paragraph {ordinal}\n\n"
            ));
        }
    }
    Fixture {
        source,
        edit: edit.expect("middle edit range"),
        restart_parser_cut,
        convergence_parser_cut,
    }
}

fn write_pending_command(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
) {
    let command = *controller
        .pending_command()
        .expect("one parser command is ready");
    match writer
        .offer_command(command)
        .expect("writer accepts command")
    {
        M11BlockWriterOfferStatus::Complete => {}
        M11BlockWriterOfferStatus::Pending => loop {
            let poll = writer.poll(runtime, FUEL).expect("writer poll");
            assert!(poll.transitions() <= FUEL);
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                break;
            }
        },
    }
    controller
        .acknowledge_command()
        .expect("parser command acknowledgement");
}

fn drive_line(
    mut scanner: SnapshotLineScanner,
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
) -> Option<(SnapshotLineScanner, usize)> {
    let line = loop {
        match scanner.poll(FUEL).expect("line discovery") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => break line,
            SnapshotLinePoll::Complete => return None,
        }
    };
    let facts = line.facts();
    let end = usize::try_from(facts.identity().end_byte()).expect("line end fits usize");
    let mut source = line.into_source().expect("source-backed line");
    let mut admission =
        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::begin_source_line(
            controller,
            facts.identity(),
        )
        .expect("line admission");
    loop {
        if source.access_budget() == 0 && source.position() < source.len() {
            source
                .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                .expect("bounded source grant");
        }
        let receipt =
            <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::poll_source_line(
                controller,
                &mut admission,
                &mut source,
                FUEL,
            )
            .expect("source poll");
        assert!(receipt.source_first_reads <= FUEL);
        if receipt.status == M11SourceLinePollStatus::Matched {
            break;
        }
    }
    <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
        controller, admission, facts,
    )
    .expect("source commit");
    let scanner = source.finish().expect("line source is exhausted");
    loop {
        let receipt = controller.poll_line(FUEL).expect("grammar poll");
        assert!(receipt.transitions <= FUEL);
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(controller, writer, runtime);
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                panic!("incremental fixture unexpectedly requires reference work");
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    Some((scanner, end))
}

fn capture(
    controller: &M11DirectBlockController,
    writer: &M11BlockWriter,
) -> M11BlockRestartCheckpoint {
    writer
        .capture_restart_checkpoint(
            controller
                .capture_restart()
                .expect("direct parser restart capture"),
        )
        .expect("joined parser/writer/Green checkpoint")
}

fn finish_document(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
) -> (M11RecursiveGreenRoot, M11BlockTerminalConvergenceCheckpoint) {
    controller.begin_finish().expect("parser finish begins");
    let mut terminal = None;
    loop {
        let receipt = controller.poll_finish(FUEL).expect("finish poll");
        assert!(receipt.transitions <= FUEL);
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                let command = *controller
                    .pending_command()
                    .expect("finish command is ready");
                if matches!(
                    command,
                    BlockCommand::Close {
                        kind: BlockKind::Document,
                        ..
                    }
                ) {
                    assert!(terminal.is_none(), "one terminal convergence boundary");
                    terminal = Some(
                        writer
                            .capture_terminal_convergence_checkpoint()
                            .expect("terminal convergence checkpoint"),
                    );
                }
                write_pending_command(controller, writer, runtime);
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                panic!("incremental fixture unexpectedly requires reference work");
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    (
        writer.take_root().expect("completed recursive Green root"),
        terminal.expect("terminal convergence boundary was observed"),
    )
}

fn clean_with_boundaries(
    runtime: &mut DocumentRuntime,
    restart_parser_cut: usize,
    convergence_parser_cut: usize,
) -> (
    M11RecursiveGreenRoot,
    M11BlockRestartCheckpoint,
    M11BlockRestartCheckpoint,
    M11BlockTerminalConvergenceCheckpoint,
) {
    let writer_lease = runtime.snapshot_current_source().expect("writer lease");
    let scanner_lease = runtime.snapshot_current_source().expect("scanner lease");
    let mut scanner = SnapshotLineScanner::new(scanner_lease).expect("line scanner");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut writer = M11BlockWriter::new(runtime, writer_lease).expect("block writer");
    write_pending_command(&mut controller, &mut writer, runtime);
    let mut restart = None;
    let mut convergence = None;
    while let Some((next, line_end)) = drive_line(scanner, &mut controller, &mut writer, runtime) {
        scanner = next;
        if line_end == restart_parser_cut {
            restart = Some(capture(&controller, &writer));
        }
        if line_end == convergence_parser_cut {
            convergence = Some(capture(&controller, &writer));
        }
    }
    let (root, terminal) = finish_document(&mut controller, &mut writer, runtime);
    (
        root,
        restart.expect("restart boundary was observed"),
        convergence.expect("convergence boundary was observed"),
        terminal,
    )
}

fn clean(runtime: &mut DocumentRuntime) -> M11RecursiveGreenRoot {
    let writer_lease = runtime.snapshot_current_source().expect("writer lease");
    let scanner_lease = runtime.snapshot_current_source().expect("scanner lease");
    let mut scanner = SnapshotLineScanner::new(scanner_lease).expect("line scanner");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut writer = M11BlockWriter::new(runtime, writer_lease).expect("block writer");
    write_pending_command(&mut controller, &mut writer, runtime);
    while let Some((next, _)) = drive_line(scanner, &mut controller, &mut writer, runtime) {
        scanner = next;
    }
    let (root, _terminal) = finish_document(&mut controller, &mut writer, runtime);
    root
}

fn prefix_witness(
    runtime: &DocumentRuntime,
    base: flark_engine::SourceVersion,
    bytes: u64,
    utf16: u64,
) -> ExactUnchangedPrefixWitness {
    runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            usize::try_from(bytes).expect("prefix bytes fit usize"),
            usize::try_from(utf16).expect("prefix UTF-16 fits usize"),
        )
        .expect("exact unchanged prefix")
}

fn suffix_witness(
    runtime: &DocumentRuntime,
    base: flark_engine::SourceVersion,
    bytes: u64,
    utf16: u64,
) -> ExactUnchangedSuffixWitness {
    runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            usize::try_from(bytes).expect("suffix bytes fit usize"),
            usize::try_from(utf16).expect("suffix UTF-16 fits usize"),
        )
        .expect("exact unchanged suffix")
}

fn local_edit(
    runtime: &mut DocumentRuntime,
    base_root: &M11RecursiveGreenRoot,
    restart: M11BlockRestartCheckpoint,
    convergence: M11BlockRestartCheckpoint,
    retained_terminal: M11BlockTerminalConvergenceCheckpoint,
    edit: std::ops::Range<usize>,
    replacement: &str,
) -> (
    M11RecursiveGreenRoot,
    M11BlockStructuralAdoptionReceipt,
    M11BlockRestartCheckpoint,
    M11BlockRestartCheckpoint,
    M11BlockTerminalConvergenceCheckpoint,
) {
    let base = base_root.source();
    let restart_parser = restart.parser_physical();
    let restart_green = restart.accepted_physical();
    let next_line_ordinal = u32::try_from(restart.next_line_ordinal()).expect("line ordinal fits");
    let convergence_green = convergence.accepted_physical();
    runtime
        .apply_edit(base, edit, replacement)
        .expect("local source edit");
    let parser_prefix = prefix_witness(
        runtime,
        base,
        restart_parser.bytes(),
        restart_parser.utf16(),
    );
    let green_prefix = prefix_witness(runtime, base, restart_green.bytes(), restart_green.utf16());
    let green_suffix = suffix_witness(
        runtime,
        base,
        convergence_green.bytes(),
        convergence_green.utf16(),
    );
    let target_lease = runtime
        .snapshot_current_source()
        .expect("target writer lease");
    let joined = restart
        .resume(runtime, base_root, target_lease, parser_prefix)
        .expect("source-bound local restart");
    let (mut controller, mut writer) = joined
        .into_local_fragment()
        .expect("local controller/writer fragment");
    let target_restart = capture(&controller, &writer);
    let scanner_lease = runtime
        .snapshot_current_source()
        .expect("target scanner lease");
    let mut scanner = SnapshotLineScanner::new_at(
        scanner_lease,
        usize::try_from(restart_parser.bytes()).expect("restart cut fits usize"),
        next_line_ordinal,
    )
    .expect("scanner restart");
    let target_suffix_start = green_suffix.target_byte_start();
    let target_parser_end = target_suffix_start
        .checked_add(1)
        .expect("fixture blank line end");
    loop {
        let (next, line_end) = drive_line(scanner, &mut controller, &mut writer, runtime)
            .expect("convergence lies before EOF");
        scanner = next;
        if line_end == target_parser_end {
            break;
        }
        assert!(
            line_end < target_parser_end,
            "local parse crossed convergence"
        );
    }
    let parser = controller
        .capture_restart()
        .expect("target convergence parser capture");
    let (root, receipt, checkpoints, terminal) = writer
        .adopt_converged_fragment(
            parser,
            target_restart,
            convergence,
            runtime,
            base_root,
            Some(green_prefix),
            Some(green_suffix),
            Vec::new(),
            Vec::new(),
            retained_terminal,
        )
        .expect("authenticated structural Green adoption");
    let mut checkpoints = checkpoints.into_iter();
    let restart = checkpoints
        .next()
        .expect("target restart checkpoint was retained");
    let convergence = checkpoints
        .next()
        .expect("target convergence checkpoint was retained");
    assert!(
        checkpoints.next().is_none(),
        "fixture retains only restart and convergence checkpoints"
    );
    (root, receipt, restart, convergence, terminal)
}

fn release(runtime: &mut DocumentRuntime, mut root: M11RecursiveGreenRoot) {
    root.begin_release(runtime).expect("begin Green release");
    while !root
        .poll_release(runtime, 64)
        .expect("poll Green release")
        .complete()
    {}
}

#[test]
fn large_mixed_document_two_structural_edits_restart_converge_and_reuse_suffix() {
    let fixture = mixed_fixture();
    let mut runtime =
        DocumentRuntime::new(&fixture.source, DocumentRuntimeConfig::default()).expect("runtime");
    let (base_root, restart0, convergence0, terminal0) = clean_with_boundaries(
        &mut runtime,
        fixture.restart_parser_cut,
        fixture.convergence_parser_cut,
    );
    assert!(base_root.source_byte_len() > 512 * 1024);
    let base_distant_byte = fixture.source.len() - 256;
    let base_distant_page = base_root
        .storage_page_identity_at_source_byte(&runtime, base_distant_byte)
        .expect("base distant page identity");

    const FIRST: &str = "- alpha\n- beta\n\n> gamma\n";
    let first_delta = isize::try_from(FIRST.len()).expect("replacement length")
        - isize::try_from(fixture.edit.len()).expect("base edit length");
    let (root1, receipt1, restart1, convergence1, terminal1) = local_edit(
        &mut runtime,
        &base_root,
        restart0,
        convergence0,
        terminal0,
        fixture.edit.clone(),
        FIRST,
    );
    let clean1 = clean(&mut runtime);
    assert_eq!(
        root1.semantic_digest(&runtime).expect("incremental digest"),
        clean1.semantic_digest(&runtime).expect("clean digest")
    );
    assert!(receipt1.green().replacement_events() > receipt1.green().deleted_events());
    assert!(receipt1.green().reused_storage_pages() > 100);
    assert!(receipt1.green().tree_nodes_visited() < 512);
    assert!(receipt1.fragment_source_bytes_read() < 256);
    assert!(receipt1.high_level_events() < 128);
    let target1_distant_byte = base_distant_byte
        .checked_add_signed(first_delta)
        .expect("shifted target-one suffix byte");
    assert_eq!(
        root1
            .storage_page_identity_at_source_byte(&runtime, target1_distant_byte)
            .expect("target-one distant page identity"),
        base_distant_page,
        "the exact distant suffix leaf survives the first path-copy splice",
    );
    release(&mut runtime, clean1);
    release(&mut runtime, base_root);

    let first_start = fixture.edit.start;
    let first_end = first_start + FIRST.len();
    const SECOND: &str = "```dart\nprint('x');\n```\n\n## after\n";
    let root1_distant_page = root1
        .storage_page_identity_at_source_byte(&runtime, target1_distant_byte)
        .expect("pre-second-edit distant page identity");
    let second_delta = isize::try_from(SECOND.len()).expect("replacement length")
        - isize::try_from(FIRST.len()).expect("first replacement length");
    let (root2, receipt2, _restart2, _convergence2, _terminal2) = local_edit(
        &mut runtime,
        &root1,
        restart1,
        convergence1,
        terminal1,
        first_start..first_end,
        SECOND,
    );
    let clean2 = clean(&mut runtime);
    assert_eq!(
        root2.semantic_digest(&runtime).expect("incremental digest"),
        clean2.semantic_digest(&runtime).expect("clean digest")
    );
    assert_ne!(
        receipt2.green().replacement_events(),
        receipt2.green().deleted_events()
    );
    assert!(receipt2.green().reused_storage_pages() > 100);
    assert!(receipt2.green().tree_nodes_visited() < 512);
    assert!(receipt2.fragment_source_bytes_read() < 256);
    assert!(receipt2.high_level_events() < 128);
    let target2_distant_byte = target1_distant_byte
        .checked_add_signed(second_delta)
        .expect("shifted target-two suffix byte");
    assert_eq!(
        root2
            .storage_page_identity_at_source_byte(&runtime, target2_distant_byte)
            .expect("target-two distant page identity"),
        root1_distant_page,
        "the exact distant suffix leaf survives the second path-copy splice",
    );

    release(&mut runtime, clean2);
    release(&mut runtime, root1);
    release(&mut runtime, root2);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}
