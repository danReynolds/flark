import 'package:flutter_test/flutter_test.dart';

import 'support/live_render_gestures.dart';
import 'support/live_render_sequence.dart';

/// Gesture → **source-range** selection mapping for the live-rendered editor.
///
/// When a user long-presses, double-taps, or drag-selects, Flutter's
/// `TextSelectionGestureDetectorBuilder` produces a *display*-space selection
/// over the projected text (`**` markers hidden). Flark maps that back to
/// `controller.selection` in **source** offsets. The Flark-specific risk this
/// suite guards is the mapping through hidden inline markers — not Flutter's
/// handle widgets, which are exercised by Flutter's own tests.
///
/// The immutable claim: a gesture selection must never land a source endpoint
/// *inside* a hidden `*`/`_`/`~`/`` ` `` marker. Such an endpoint would corrupt
/// the marker pair on the next edit. [_expectEndpointsOffHiddenMarkers] encodes
/// that property and runs in every scenario. Per the harness authoring policy
/// it is never weakened to match reality; a violation is a defect to file, not
/// a boundary to move.
///
/// Word-selection boundaries themselves are pinned from the actual gesture
/// result (the double-tap/long-press word span Flutter chooses). Gestures are
/// real pointer streams routed through the production gesture detector — see
/// [LiveRenderGestures]. The default test target platform is Android, so a
/// long-press selects the word under it (an Apple platform would position the
/// caret instead); no platform override is needed for these scenarios.
void main() {
  group('gesture selection maps through hidden inline markers', () {
    testWidgets('double-tap selects a word inside a styled run', (
      tester,
    ) async {
      // 'a **bold** b': content 'bold' is source [4, 8); the hidden '**'
      // markers occupy [2, 4) and [8, 10). Display: 'a bold b'.
      final seq = await LiveRenderSequence.start(tester, 'a **bold** b');
      seq.expectRows(['a bold b']);

      await seq.doubleTapAtSource(6); // source 6 = 'l', inside 'bold'

      final selection = seq.controller.selection;
      // The selection is the *content* range of the run — endpoints land on
      // 'bold', flush against but never inside the hidden markers.
      expect(selection.start, 4, reason: 'start on content, not inside "**"');
      expect(selection.end, 8, reason: 'end on content, not inside "**"');
      _expectEndpointsOffHiddenMarkers(seq);
      // The projection of the source selection displays exactly 'bold'.
      expect(_selectionDisplayText(seq), 'bold');
      // A selection gesture never mutates the document.
      expect(seq.source, 'a **bold** b');
    });

    testWidgets('long-press selects a word adjacent to a run', (tester) async {
      // '**bold** word': content 'word' is source [9, 13); hidden markers at
      // [0, 2) and [6, 8). Display: 'bold word'.
      final seq = await LiveRenderSequence.start(tester, '**bold** word');
      seq.expectRows(['bold word']);

      await seq.longPressAtSource(11); // source 11 = 'r', inside 'word'

      final selection = seq.controller.selection;
      // 'word' exactly — the press sits past the run, so the word span stops at
      // the run's trailing marker boundary rather than swallowing it.
      expect(selection.start, 9, reason: 'word starts after the run + space');
      expect(selection.end, 13, reason: 'word ends at end of document');
      _expectEndpointsOffHiddenMarkers(seq);
      expect(_selectionDisplayText(seq), 'word');
      expect(seq.source, '**bold** word');
    });

    testWidgets('drag-select across a run edge keeps endpoints off markers', (
      tester,
    ) async {
      // 'x **bold** y': hidden markers at [2, 4) and [8, 10). Display:
      // 'x bold y'. Dragging from before 'x' to after 'y' sweeps across both
      // hidden markers.
      final seq = await LiveRenderSequence.start(tester, 'x **bold** y');
      seq.expectRows(['x bold y']);

      await seq.dragSelectSource(0, 12); // 'x' start → end of doc (after 'y')

      final selection = seq.controller.selection;
      // The sweep crosses two hidden marker pairs, yet both endpoints resolve
      // to clean source offsets (0 and 12) — normalized off the markers by the
      // downstream-start / upstream-end projection mapping.
      expect(selection.start, 0);
      expect(selection.end, 12);
      _expectEndpointsOffHiddenMarkers(seq);
      // The user sees the whole visible line selected, markers invisible.
      expect(_selectionDisplayText(seq), 'x bold y');
      expect(seq.source, 'x **bold** y');
    });

    testWidgets('double-tap in a plain paragraph maps 1:1 (control)', (
      tester,
    ) async {
      // No markers: source and display offsets coincide, so the gesture is a
      // straight pass-through with no projection to get wrong.
      final seq = await LiveRenderSequence.start(tester, 'the quick fox');
      seq.expectRows(['the quick fox']);

      await seq.doubleTapAtSource(5); // source 5 = 'u', inside 'quick'

      final selection = seq.controller.selection;
      expect(selection.start, 4);
      expect(selection.end, 9);
      // 1:1 mapping: with no hidden ranges the source selection projects to the
      // identical display offsets.
      expect(seq.controller.projection.hiddenRanges, isEmpty);
      final projected = seq.controller.projection.sourceSelectionToDisplay(
        selection,
      );
      expect(projected.start, selection.start);
      expect(projected.end, selection.end);
      expect(_selectionDisplayText(seq), 'quick');
      expect(seq.source, 'the quick fox');
    });
  });
}

/// The display text covered by the current source selection, obtained by
/// projecting `controller.selection` forward through the projection. This is
/// what the user sees highlighted — the read-side check that complements the
/// source-offset assertions.
String _selectionDisplayText(LiveRenderSequence seq) {
  final controller = seq.controller;
  final projected = controller.projection.sourceSelectionToDisplay(
    controller.selection,
  );
  return seq.display.substring(projected.start, projected.end);
}

/// Immutable marker-boundary claim: neither selection endpoint may sit strictly
/// *inside* a hidden range (a `*`/`_`/`~`/`` ` `` marker). An endpoint flush
/// against a marker boundary is fine; one between a marker's start and end
/// would split the marker pair on the next edit and corrupt the source.
void _expectEndpointsOffHiddenMarkers(LiveRenderSequence seq) {
  final controller = seq.controller;
  final selection = controller.selection;
  for (final offset in {selection.start, selection.end}) {
    for (final hidden in controller.projection.hiddenRanges) {
      final range = hidden.range;
      final bisectsMarker = offset > range.start && offset < range.end;
      expect(
        bisectsMarker,
        isFalse,
        reason:
            'source endpoint $offset bisects hidden marker '
            '[${range.start}, ${range.end}) in "${seq.source}" — a later edit '
            'would corrupt the marker pair',
      );
    }
  }
}
