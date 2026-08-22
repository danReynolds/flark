import 'dart:io';

import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'close returns the process-global native registry to zero',
    () {
      expect(
        FlarkNativeDocument.inspectGlobalLiveState(
          libraryPath: libraryPath,
        ).isEmpty,
        isTrue,
        reason: 'the isolated lifecycle lane must begin with no native owners',
      );

      final document = FlarkNativeDocument.open(
        'first\n\nsecond\n\nthird\n',
        libraryPath: libraryPath,
      );
      addTearDown(document.close);
      document.pumpUntilReady();
      final edit = document.applyEditUtf16(2, 2, 'x');
      expect(edit.historyToken, isNotNull);
      document.pumpUntilReady();
      final viewport = document.queryViewport(maxRows: 1);
      expect(viewport.continuation, isNonZero);
      document.createAnchorUtf16(2, downstream: true);

      final live = FlarkNativeDocument.inspectGlobalLiveState(
        libraryPath: libraryPath,
      );
      expect(live.liveSessions, 1);
      expect(live.liveTransactions, 0);
      expect(live.liveContinuations, greaterThanOrEqualTo(1));
      expect(live.liveAnchors, 1);
      expect(live.liveHistoryTokens, greaterThanOrEqualTo(1));

      document.close(workUnits: 1);

      final closed = FlarkNativeDocument.inspectGlobalLiveState(
        libraryPath: libraryPath,
      );
      expect(closed.isEmpty, isTrue);
      expect(closed.liveSessions, 0);
      expect(closed.liveTransactions, 0);
      expect(closed.liveContinuations, 0);
      expect(closed.liveAnchors, 0);
      expect(closed.liveHistoryTokens, 0);
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to the built flark_abi library.'
        : false,
  );
}
