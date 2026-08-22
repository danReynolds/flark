import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';

import '../support/flark_test_paths.dart';

/// RFC 022 Phase 1: commands whose transactions declare authored delimiter
/// ranges commit only when a synchronous parse confirms each range as a
/// hidden marker. These tests drive the judge against the real comrak bridge.
void main() {
  final libPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libPath.isEmpty || !File(libPath).existsSync()) {
    test('native bridge not built; parser judge suite skipped', () {
      expect(true, isTrue);
    });
    return;
  }

  FlarkFlutterController controller(String markdown) {
    final created = FlarkFlutterController.fromMarkdown(
      markdown,
      extensions: FlarkMarkdownEditingExtensions.standard(),
      parseBackend: FlarkNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      ),
    );
    addTearDown(created.dispose);
    return created;
  }

  test('a false authored claim is rejected and the document is untouched', () {
    final editor = controller('hello');
    // Claim that plain letters are a hidden delimiter — the parser must veto.
    final result = editor.applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: const FlarkSourceRange(0, 0),
          replacementText: 'x',
        ),
        selectionBefore: editor.selection,
        selectionAfter: const FlarkSelection(baseOffset: 1, extentOffset: 1),
        metadata: const FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.programmatic,
          authoredMarkerRanges: [FlarkSourceRange(0, 1)],
        ),
      ),
    );
    expect(result.commandResult.isRejected, isTrue);
    expect(result.commandResult.reason, contains('did not confirm'));
    expect(editor.markdown, 'hello', reason: 'a vetoed edit must not commit');
  });

  test(
    'a confirmed wrap commits and adopts authoritatively in the same turn',
    () {
      final editor = controller('hello world');
      expect(
        editor.applySelection(
          const FlarkSelection(baseOffset: 0, extentOffset: 5),
        ),
        isTrue,
      );
      final result = editor.commands.toggleStrong();
      expect(result.commandResult.isHandled, isTrue);
      expect(editor.markdown, '**hello** world');
      // The judge's parse is reused for adoption: no parseNow, no pumping — the
      // command lands authoritative synchronously.
      expect(editor.hasAuthoritativeRenderPlan, isTrue);
    },
  );

  test('a code wrap whose core terminates the span early is vetoed', () {
    // The verbatim code wrap writes `a`b` — comrak closes the span at the
    // interior backtick, so the authored closing marker is not a marker at
    // all. The proposer cannot see this (interior markers are "literal" for
    // code); the judge can.
    final editor = controller('a`b');
    expect(
      editor.applySelection(
        const FlarkSelection(baseOffset: 0, extentOffset: 3),
      ),
      isTrue,
    );
    final result = editor.commands.toggleInlineCode();
    expect(result.commandResult.isRejected, isTrue);
    expect(editor.markdown, 'a`b', reason: 'the invalid wrap must not commit');
  });

  test('nesting emphasis inside strong is confirmed, not false-rejected', () {
    // The authored `*` fuses with the existing `**` into a `***` cluster the
    // parser tokenizes with different sub-range bounds than declared
    // ([1,3)+[0,1), not [2,3)). Coverage-based confirmation must accept it:
    // every authored position is hidden, just under different cluster cuts.
    final editor = controller('**foobar**');
    expect(
      editor.applySelection(
        const FlarkSelection(baseOffset: 2, extentOffset: 8),
      ),
      isTrue,
    );
    final result = editor.commands.toggleEmphasis();
    expect(
      result.commandResult.isHandled,
      isTrue,
      reason:
          'italic-on-bold is the canonical nesting flow and must not '
          'be vetoed by exact-bounds tokenization differences',
    );
    expect(editor.markdown, '***foobar***');
    expect(editor.hasAuthoritativeRenderPlan, isTrue);
  });

  test('nesting emphasis on a strong SUFFIX commits (abutting closers)', () {
    // `**foo *bar***`: em nested in strong whose closers abut. The
    // partial-strong-intent heuristic used to misread the parsed adjacency
    // as mid-typing and keep the inner markers visible, so the judge vetoed
    // the wrap; the heuristic now defers to the parser (adjacency only
    // counts when the neighbor is unclaimed by a parsed marker range) and
    // the wrap commits and renders hidden.
    final editor = controller('**foo bar**');
    expect(
      editor.applySelection(
        const FlarkSelection(baseOffset: 6, extentOffset: 9),
      ),
      isTrue,
    );
    final result = editor.commands.toggleEmphasis();
    expect(result.commandResult.isHandled, isTrue);
    expect(editor.markdown, '**foo *bar***');
    expect(editor.hasAuthoritativeRenderPlan, isTrue);
  });

  test(
    'a judged command does not schedule a redundant follow-up parse',
    () async {
      final editor = controller('hello world');
      editor.ensureParsing();
      expect(
        editor.applySelection(
          const FlarkSelection(baseOffset: 0, extentOffset: 5),
        ),
        isTrue,
      );
      var adoptions = 0;
      final subscription = editor.events.listen((event) {
        if (event.kind == FlarkControllerEventKind.parseAdopted) adoptions += 1;
      });
      addTearDown(subscription.cancel);

      expect(editor.commands.toggleStrong().commandResult.isHandled, isTrue);
      expect(editor.hasAuthoritativeRenderPlan, isTrue);
      // Outlast the debounce window: the runtime adoption's notify scheduled a
      // parse before the judge's result was adopted; that schedule must have
      // been cancelled, not left to reparse the unchanged document and
      // re-notify every listener.
      await Future<void>.delayed(const Duration(milliseconds: 200));
      expect(
        adoptions,
        1,
        reason: 'the judged parse is the only adoption for one command',
      );
    },
  );

  test('multi-paragraph wraps are judged per paragraph and commit', () {
    final editor = controller('alpha\n\nbeta');
    expect(
      editor.applySelection(
        const FlarkSelection(baseOffset: 0, extentOffset: 11),
      ),
      isTrue,
    );
    final result = editor.commands.toggleStrong();
    expect(result.commandResult.isHandled, isTrue);
    expect(editor.markdown, '**alpha**\n\n**beta**');
    expect(editor.hasAuthoritativeRenderPlan, isTrue);
  });
}
