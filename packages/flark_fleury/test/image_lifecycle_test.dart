import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:image/image.dart' as pixels;
import 'package:test/test.dart';

class _Client extends MockClient {
  _Client(super.handler);
  bool closed = false;
  @override
  void close() {
    closed = true;
    super.close();
  }
}

void main() {
  final png = pixels.encodePng(pixels.Image(width: 4, height: 4));
  test(
    'loading, error and completion preserve caret geometry; stale loads are discarded',
    () async {
      final requests = <String, Completer<http.Response>>{};
      final clients = <_Client>[];
      Future<void> requestStarted(String path) async {
        for (var i = 0; i < 20 && !requests.containsKey(path); i++) {
          await Future<void>.delayed(Duration.zero);
        }
        expect(requests, contains(path));
      }

      await http.runWithClient(
        () async {
          final tester = FleuryTester(viewportSize: const CellSize(40, 18));
          final focus = FocusNode();
          final editor = FlarkEditor(
            createParseBackend(),
            text: '![photo](https://test/old)\n\nafter',
            caret: 0,
          );
          final controller = FlarkFleuryController(editor);
          addTearDown(() {
            tester.dispose();
            controller.dispose();
            focus.dispose();
          });
          tester.pumpWidget(
            Theme(
              data: const ThemeData(),
              child: FlarkEditorView(
                controller: controller,
                focusNode: focus,
                autofocus: true,
              ),
            ),
          );
          expect(tester.renderToString(), contains('Loading image'));
          await requestStarted('/old');
          editor.apply(SetSelection.caret(editor.source.indexOf('after')));
          tester.render();
          final caret = focus.caretRect;
          editor.apply(const SetSelection.caret(2));
          editor.apply(const SetImage('https://test/new', alt: 'photo'));
          tester.render();
          expect(clients.first.closed, isTrue);
          requests['/old']!.complete(http.Response.bytes(png, 200));
          await requestStarted('/new');
          requests['/new']!.complete(http.Response('no', 404));
          for (var i = 0; i < 10; i++) {
            await Future<void>.delayed(Duration.zero);
          }
          expect(tester.renderToString(), contains('Could not load image'));
          expect(tester.semantics().byRole(SemanticRole.image), isEmpty);
          editor.apply(const SetSelection.caret(2));
          editor.apply(const SetImage('https://test/ok', alt: 'photo'));
          tester.render();
          await requestStarted('/ok');
          requests['/ok']!.complete(http.Response.bytes(png, 200));
          for (var i = 0; i < 10; i++) {
            await Future<void>.delayed(Duration.zero);
          }
          tester.render();
          expect(
            tester.semantics().byRole(SemanticRole.image).single.label,
            'photo',
          );
          editor.apply(SetSelection.caret(editor.source.indexOf('after')));
          tester.render();
          expect(focus.caretRect, caret);
          // Edits before the same resource must not remount/reload the preview.
          final count = clients.length;
          editor.apply(
            ReplaceRange(0, editor.source.length, 'before\n\n${editor.source}'),
          );
          tester.render();
          expect(clients.length, count);
          editor.setSourceMode(true);
          tester.render();
          expect(clients.every((client) => client.closed), isTrue);
          expect(tester.semantics().byRole(SemanticRole.image), isEmpty);
        },
        () {
          final client = _Client((request) {
            final pending = requests[request.url.path] =
                Completer<http.Response>();
            return pending.future;
          });
          clients.add(client);
          return client;
        },
      );
    },
  );
}
