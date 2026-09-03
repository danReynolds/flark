import 'dart:convert';
import 'dart:io';

import 'package:flark_example/native_canary_mode.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('explicit observation marks the mounted surface for paint', (
    tester,
  ) async {
    final key = GlobalKey();
    await tester.pumpWidget(
      ColoredBox(key: key, color: const Color(0xff000000)),
    );
    final surface = key.currentContext!.findRenderObject()!;
    expect(surface.debugNeedsPaint, isFalse);

    dogfoodRequestSurfacePaint(surface);

    expect(surface.debugNeedsPaint, isTrue);
    await tester.pump();
    expect(surface.debugNeedsPaint, isFalse);
  });

  test('frame clock anchor brackets one monotonic observation', () {
    final anchor = dogfoodFrameClockAnchor();

    expect(anchor.keys, {
      'epochBeforeMicros',
      'monotonicMicros',
      'epochAfterMicros',
    });
    expect(
      anchor['epochAfterMicros']!,
      greaterThanOrEqualTo(anchor['epochBeforeMicros']!),
    );
    expect(
      anchor['epochAfterMicros']! - anchor['epochBeforeMicros']!,
      lessThan(1000),
    );
    expect(anchor['monotonicMicros'], greaterThan(0));
  });

  test('paint identity distinguishes collapsed and ranged selections', () {
    expect(
      dogfoodPaintSelectionIdentityMatches(
        activeRowVisible: true,
        selectionBaseUtf16: 4,
        selectionExtentUtf16: 4,
        caretSourceUtf16: 4,
        caretDisplayUtf16: 3,
        selectionRectCount: 0,
      ),
      isTrue,
    );
    expect(
      dogfoodPaintSelectionIdentityMatches(
        activeRowVisible: true,
        selectionBaseUtf16: 0,
        selectionExtentUtf16: 5,
        caretSourceUtf16: null,
        caretDisplayUtf16: null,
        selectionRectCount: 1,
      ),
      isTrue,
    );
    expect(
      dogfoodPaintSelectionIdentityMatches(
        activeRowVisible: true,
        selectionBaseUtf16: 0,
        selectionExtentUtf16: 5,
        caretSourceUtf16: null,
        caretDisplayUtf16: null,
        selectionRectCount: 0,
      ),
      isFalse,
    );
    expect(
      dogfoodPaintSelectionIdentityMatches(
        activeRowVisible: false,
        selectionBaseUtf16: 2,
        selectionExtentUtf16: 2,
        caretSourceUtf16: null,
        caretDisplayUtf16: null,
        selectionRectCount: 0,
      ),
      isTrue,
    );
  });

  test('receipt writes are serialized and the final admission wins', () async {
    final directory = await Directory.systemTemp.createTemp(
      'flark-canary-receipt-',
    );
    addTearDown(() => directory.delete(recursive: true));
    final receipt = File('${directory.path}/receipt.json');
    final writer = DogfoodCanaryReceiptFileWriter(receipt.path);

    await Future.wait([
      for (var sequence = 0; sequence < 64; sequence += 1)
        writer.write({'sequence': sequence}),
    ]);

    expect(jsonDecode(await receipt.readAsString()), {'sequence': 63});
    expect(File('${receipt.path}.tmp').existsSync(), isFalse);
  });

  test('a failed receipt write does not poison the next admission', () async {
    final directory = await Directory.systemTemp.createTemp(
      'flark-canary-receipt-recovery-',
    );
    addTearDown(() => directory.delete(recursive: true));
    final parent = Directory('${directory.path}/later');
    final receipt = File('${parent.path}/receipt.json');
    final writer = DogfoodCanaryReceiptFileWriter(receipt.path);

    await expectLater(
      writer.write({'sequence': 0}),
      throwsA(isA<FileSystemException>()),
    );
    await parent.create();
    await writer.write({'sequence': 1});

    expect(jsonDecode(await receipt.readAsString()), {'sequence': 1});
  });
}
