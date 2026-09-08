import 'dart:async';
import 'dart:ui' as ui;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class PendingImage extends ImageProvider<int> {
  PendingImage(this.id);
  final int id;
  final result = Completer<ImageInfo>();
  int loads = 0;
  @override
  Future<int> obtainKey(ImageConfiguration configuration) =>
      SynchronousFuture(id);
  @override
  ImageStreamCompleter loadImage(int key, ImageDecoderCallback decode) {
    loads++;
    return OneFrameImageStreamCompleter(result.future);
  }
}

void main() {
  final backend = createParseBackend();
  for (final readOnly in [true, false]) {
    testWidgets(
      'link activation uses glyph bounds and preserves source: readonly=$readOnly',
      (tester) async {
        final c = FlarkController(
          FlarkEditor(backend, text: '[guide](/guide)', caret: 3),
        );
        final opened = <Uri>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                readOnly: readOnly,
                showToolbar: false,
                baseUri: Uri.parse('https://example.com/'),
                onOpenLink: opened.add,
              ),
            ),
          ),
        );
        await tester.pump();
        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkSurface),
        );
        if (!readOnly) {
          await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
        }
        await tester.tapAt(surface.localToGlobal(surface.caretRect.center));
        expect(c.editor.selection.extent, 3);
        await tester.tapAt(surface.localToGlobal(const Offset(500, 28)));
        if (!readOnly) await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
        expect(opened, [Uri.parse('https://example.com/guide')]);
        expect(c.text, '[guide](/guide)');
        await tester.pump(const Duration(milliseconds: 350));
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      },
    );
  }
  testWidgets(
    'offscreen images do not load or paint and scrolling does not edit',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(
          backend,
          text: List.generate(24, (i) => '![image $i](/$i.png)').join('\n\n'),
        ),
      );
      final requests = <String>[];
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              height: 360,
              child: FlarkEditorWidget(
                controller: c,
                onPaint: paints.add,
                imageProvider: (uri) {
                  requests.add(uri.path);
                  return null;
                },
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      expect(requests.length, inInclusiveRange(1, 2));
      expect(paints.last.images.length, inInclusiveRange(1, 2));
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final point = surface.localToGlobal(paints.last.images.first.rect.center);
      await tester.dragFrom(point, const Offset(0, -200));
      await tester.pump();
      expect(find.text('Edit image'), findsNothing);
      expect(c.editor.history.canUndo, isFalse);
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets(
    'link dialog preserves formatting, commits one paint and restores input',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: 'before **read** after'),
      );
      final paints = <FlarkPaintObservation>[];
      final opened = <Uri>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
              onOpenLink: opened.add,
            ),
          ),
        ),
      );
      await tester.pump();
      c.command(const SetSelection(7, 15));
      await tester.pump();
      await tester.tap(find.byTooltip('Link'));
      await tester.pumpAndSettle();
      expect(find.text('Insert link'), findsOneWidget);
      await tester.enterText(
        find.widgetWithText(TextField, 'Destination'),
        'https://example.com',
      );
      paints.clear();
      await tester.tap(find.text('Save'));
      await tester.pump();
      const expected = 'before [**read**](<https://example.com>) after';
      expect(c.text, expected);
      expect(paints, isNotEmpty);
      for (final paint in paints) {
        expect(paint.snapshot.source, expected);
        expect(paint.caretSource, 16);
        expect(paint.styles.single, contains(Style.strong | Style.link));
      }
      await tester.pumpAndSettle();
      c.command(const InsertText('!'));
      await tester.pump();
      expect(c.text, 'before [**read**!](<https://example.com>) after');
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpAndSettle();
      expect(find.text('Edit link'), findsOneWidget);
      await tester.tap(find.text('Open link'));
      expect(opened.single, Uri.parse('https://example.com'));
      await tester.tap(find.text('Remove link'));
      await tester.pump();
      expect(c.text, 'before **read**! after');
      c.command(const Undo());
      await tester.pump();
      expect(c.text, 'before [**read**!](<https://example.com>) after');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'dialog cancellation and stale submissions preserve newer source',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: 'hello'));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkEditorWidget(controller: c)),
        ),
      );
      await tester.tap(find.byTooltip('Link'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.widgetWithText(TextField, 'Destination'),
        '/guide',
      );
      c.command(const InsertText('new '));
      await tester.tap(find.text('Save'));
      await tester.pump();
      expect(c.text, 'new hello');
      expect(find.textContaining('could not be applied'), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(c.text, 'new hello');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  for (final failure in [false, true]) {
    testWidgets(
      'image ${failure ? 'failure' : 'completion'} keeps source and geometry stable',
      (tester) async {
        final image = PendingImage(failure ? 8002 : 8001);
        final c = FlarkController(
          FlarkEditor(
            backend,
            text: '![cat](/cat.png)\n\nfollowing',
            caret: 27,
          ),
        );
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
                imageProvider: (_) => image,
                baseUri: Uri.parse('https://example.com/'),
              ),
            ),
          ),
        );
        await tester.pump();
        final before = paints.last;
        expect(before.images.single.state, 'loading');
        final rect = before.images.single.rect;
        expect(rect.height, 180);
        expect(before.caret!.top, greaterThan(rect.bottom));
        paints.clear();
        if (failure) {
          image.result.completeError(StateError('missing image'));
        } else {
          final recorder = ui.PictureRecorder();
          Canvas(recorder).drawRect(
            const Rect.fromLTWH(0, 0, 20, 10),
            Paint()..color = Colors.blue,
          );
          final picture = recorder.endRecording();
          final decoded = await tester.runAsync(() => picture.toImage(20, 10));
          picture.dispose();
          image.result.complete(ImageInfo(image: decoded!));
        }
        await tester.idle();
        await tester.pump();
        expect(paints.first.images.single.state, failure ? 'failed' : 'loaded');
        expect(paints.last.images.single.rect, rect);
        expect(paints.last.caret, before.caret);
        expect(c.text, '![cat](/cat.png)\n\nfollowing');
        c.command(const InsertText('!'));
        paints.clear();
        await tester.pump();
        expect(paints.first.snapshot.source, '![cat](/cat.png)\n\nfollowing!');
        expect(paints.first.images.single.rect, rect);
        expect(image.loads, 1);
        await tester.pumpWidget(const SizedBox());
        c.dispose();
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'image preview opens edit dialog; URL changes cannot paint an old request',
    (tester) async {
      final old = PendingImage(8003), next = PendingImage(8004);
      final c = FlarkController(
        FlarkEditor(backend, text: '![cat](/old.png)', caret: 4),
      );
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              onPaint: paints.add,
              imageProvider: (uri) => uri.path == '/old.png' ? old : next,
            ),
          ),
        ),
      );
      await tester.pump();
      final surface =
          tester.getTopLeft(find.byType(FlarkEditorWidget)) +
          const Offset(0, 48);
      await tester.tapAt(surface + paints.last.images.single.rect.center);
      await tester.pumpAndSettle();
      expect(find.text('Edit image'), findsOneWidget);
      await tester.enterText(
        find.widgetWithText(TextField, 'Image URL'),
        '/new.png',
      );
      await tester.tap(find.text('Save'));
      await tester.pump();
      expect(c.text, '![cat](</new.png>)');
      expect(paints.last.images.single.destination, '/new.png');
      old.result.completeError(StateError('obsolete'));
      await tester.pump();
      expect(paints.last.images.single.state, 'loading');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      next.result.completeError(StateError('disposed'));
      await tester.pump();
      expect(tester.takeException(), isNull);
    },
  );
}
