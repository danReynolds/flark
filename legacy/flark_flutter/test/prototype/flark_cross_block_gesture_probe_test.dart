import 'package:flutter_test/flutter_test.dart';

import '../v2/flutter/support/live_render_gestures.dart';
import '../v2/flutter/support/live_render_sequence.dart';

/// Disposable interaction probe for a seam that controller-level selection
/// tests cannot exercise: one physical mouse drag crossing two independent
/// EditableText widgets.
void main() {
  testWidgets('physical drag across live block editables', (tester) async {
    final sequence = await LiveRenderSequence.start(tester, '- one\n- two');
    sequence.expectRows(['one', 'two']);

    await sequence.dragSelectSource(2, 11);

    // ignore: avoid_print
    print(
      'flark_prototype cross_block_mouse_drag '
      'requested=2..11 actual=${sequence.controller.selection.start}..'
      '${sequence.controller.selection.end} rows=${sequence.rows.length}',
    );
    expect(sequence.source, '- one\n- two');
  });
}
