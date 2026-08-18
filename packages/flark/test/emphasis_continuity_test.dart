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

      // The next keystroke is ordinary plain text immediately after the
      // construct. The frame it produces must not reveal the delimiters.
      typeAt(controller, ' ');
      final immediate = activeProjection(controller);
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
    // ACCEPTANCE TEST for RFC 027 section 4.4.1 (literal-safe envelopes).
    // Fails today at the `immediate` assertion: the superseded contract
    // refuses any edit whose caret touches an inline fact's source range,
    // boundary-inclusive, so a space after a closing delimiter is refused
    // and the row reveals its raw source. Under envelopes the parser proves
    // that position harmless and the presentation is retained. Unskip when
    // envelopes land.
    skip: 'awaiting RFC 027 4.4.1 literal-safe envelopes',
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
