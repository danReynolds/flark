// Dogfood regression (2026-08-18): typing an emphasis run and then a space
// flashes the row's raw Markdown before settling back to the rendered
// presentation. RFC 027 forbids exactly this — "ordinary source edits inside
// rendered constructs must not flash an entire raw row", and incomplete or
// pending ranges are supposed to use local exact-source islands rather than a
// whole-row reveal.
//
// The existing continuity coverage never reached this case: the inline-typing
// profile workload types alphanumerics strictly inside an already-certified
// `**strong**` run, which is the one shape row continuity retains. No test
// typed a delimiter, created a construct, or edited at a construct boundary.

import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  Future<FlarkEditorController> open(String source) async {
    final controller = await FlarkEditorController.open(
      source,
      libraryPath: libraryPath,
    );
    await controller.continueParsing();
    return controller;
  }

  Future<void> settle(FlarkEditorController controller) async {
    final deadline = DateTime.now().add(const Duration(seconds: 10));
    while ((controller.pendingEdits != 0 ||
            controller.viewport?.revision != controller.revision) &&
        DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
    await controller.debugWaitForPresentationSettled();
  }

  /// The active row's projected text, or null when nothing is projected —
  /// which the frame harness also counts as a raw frame.
  String? activeProjection(FlarkEditorController controller) {
    for (final row in controller.rows) {
      final candidate = controller.surfaceRow(row);
      if (candidate.active) return candidate.text;
    }
    return null;
  }

  FlarkSurfaceRow activeSurface(FlarkEditorController controller) => controller
      .rows
      .map(controller.surfaceRow)
      .firstWhere((candidate) => candidate.active);

  void typeAt(FlarkEditorController controller, String text) {
    final before = controller.inputValue;
    final offset = before.selection.extentOffset;
    controller.applyDeltas([
      TextEditingDeltaInsertion(
        oldText: before.text,
        textInserted: text,
        insertionOffset: offset,
        selection: TextSelection.collapsed(offset: offset + text.length),
        composing: TextRange.empty,
      ),
    ]);
  }

  test(
    'a space after a freshly typed emphasis run never reveals raw markers',
    () async {
      final controller = await open('Plain paragraph line.\n');
      addTearDown(controller.close);
      final row = controller.rows.first;
      controller.activateRow(row, row.editableUtf16!.end);

      // Type " *test*" and let it certify: the settled row renders the
      // emphasis with its delimiters hidden.
      for (final key in [' ', '*', 't', 'e', 's', 't', '*']) {
        typeAt(controller, key);
        await settle(controller);
      }
      expect(activeProjection(controller), isNot(contains('*')));
      final authorityRow = controller.rows.first;
      expect(
        authorityRow.literalSafeEnvelopes.map(
          (envelope) => (
            envelope.editClass.name,
            envelope.sourceUtf16.start,
            envelope.sourceUtf16.end,
          ),
        ),
        contains((
          'singleAsciiSpaceInsertion',
          controller.globalCaretOffset,
          controller.globalCaretOffset,
        )),
      );

      // The next keystroke is ordinary plain text immediately after the
      // construct. The frame it produces must not reveal the delimiters.
      final boundary = controller.globalCaretOffset;
      typeAt(controller, ' ');
      final immediateSurface = activeSurface(controller);
      final immediate = immediateSurface.text;
      final immediateSelection = immediateSurface.selection!;

      expect(controller.globalCaretOffset, boundary + 1);
      expect(controller.inputValue.text, contains('*test* '));
      expect(
        immediateSurface.sourceOffsetForTextOffset(
          immediateSelection.extentOffset,
          affinity: immediateSelection.affinity,
        ),
        boundary + 1,
        reason: 'painted caret must resolve to the current source insertion',
      );
      await settle(controller);

      expect(
        immediate,
        isNotNull,
        reason: 'the plain-text keystroke dropped the active projection',
      );
      expect(
        immediate,
        isNot(contains('*')),
        reason:
            'a plain-text edit adjacent to a certified construct revealed '
            'raw Markdown before recertification',
      );
      expect(activeProjection(controller), isNot(contains('*')));
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'space after an opening delimiter has exact immediate source and caret',
    () async {
      final controller = await open('*test*\n');
      addTearDown(controller.close);
      final row = controller.rows.single;
      final openingBoundary = row.inlineFacts!.single.contentUtf16.start;
      controller.activateRow(row, openingBoundary);

      typeAt(controller, ' ');
      final immediate = activeSurface(controller);
      final selection = immediate.selection!;

      expect(immediate.text, contains('* test*'));
      expect(immediate.runs, hasLength(1));
      expect(immediate.runs.single.sourceExact, isTrue);
      expect(controller.inputValue.text, contains('* test*'));
      expect(controller.globalCaretOffset, openingBoundary + 1);
      expect(
        immediate.sourceOffsetForTextOffset(
          selection.extentOffset,
          affinity: selection.affinity,
        ),
        openingBoundary + 1,
      );

      await settle(controller);
      expect(await controller.readSource(), '* test*\n');
      expect(controller.globalCaretOffset, openingBoundary + 1);
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'nested delimiter boundary fails closed to exact immediate source',
    () async {
      const source = '*a _b_ c*\n';
      final controller = await open(source);
      addTearDown(controller.close);
      final row = controller.rows.single;
      final nestedOpening = source.indexOf('_');

      expect(
        row.literalSafeEnvelopes
            .where(
              (envelope) =>
                  envelope.editClass ==
                  FlarkLiteralEditClass.asciiWordInsertion,
            )
            .any(
              (envelope) =>
                  envelope.sourceUtf16.start <= nestedOpening &&
                  nestedOpening <= envelope.sourceUtf16.end,
            ),
        isFalse,
        reason: 'an outer fact cannot certify across nested inline syntax',
      );

      controller.activateRow(row, nestedOpening);
      typeAt(controller, 'x');
      final immediate = activeSurface(controller);
      final selection = immediate.selection!;

      expect(immediate.text, contains('*a x_b_ c*'));
      expect(immediate.runs, hasLength(1));
      expect(immediate.runs.single.sourceExact, isTrue);
      expect(controller.inputValue.text, contains('*a x_b_ c*'));
      expect(controller.globalCaretOffset, nestedOpening + 1);
      expect(
        immediate.sourceOffsetForTextOffset(
          selection.extentOffset,
          affinity: selection.affinity,
        ),
        nestedOpening + 1,
      );

      await settle(controller);
      expect(await controller.readSource(), '*a x_b_ c*\n');
      expect(controller.globalCaretOffset, nestedOpening + 1);
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
