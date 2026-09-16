/// Native host CPU diagnostic through input dispatch, layout and cell paint.
/// This excludes terminal/DOM presentation and OS input delivery.
/// Run with `dart run tool/perf_audit.dart` from this package.
library;

import 'dart:convert';
import 'dart:io';
import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';

String fixture(String shape, int bytes) {
  final out = StringBuffer();
  var i = 0;
  while (true) {
    final chunk = switch (shape) {
      'prose' => 'Ordinary prose with **bold** and a [link](/url).\n\n',
      'definitions' => '[ref${i++}]: /target "title"\n',
      'code' =>
        '```ruby\ndef hello${i++}\n  puts "${'hello ' * 30}"\nend\n```\n\n',
      'tables' =>
        '| A | B |\n| - | - |\n| value ${i++} ${'word ' * 20}| **bold** |\n\n',
      'images' => '![Photo ${i++}](demo.png)\n\nSome nearby text.\n\n',
      'source' => 'word\n',
      _ => throw ArgumentError(shape),
    };
    if (out.length + chunk.length + 6 > bytes) break;
    out.write(chunk);
  }
  out.write('\n\nTail');
  return out.toString();
}

void require(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Future<void> main(List<String> args) async {
  final backend = createParseBackend();
  final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  try {
    for (final shape
        in args.isNotEmpty
            ? args
            : ['prose', 'definitions', 'code', 'tables', 'images', 'source']) {
      for (final kib in shape == 'source' ? [256] : [16, 32]) {
        final source = fixture(shape, kib * 1024);
        final editor = FlarkEditor(
          backend,
          text: source,
          caret: source.length,
          syncLimit: shape == 'source'
              ? FlarkEditor.defaultSyncLimit
              : kib * 1024,
          codeEditing: code,
        );
        // Preserve actual product admission: a dense shape may enter source
        // mode below the byte cap. Do not expand shape limits to force a pass.
        if (shape == 'source') {
          require(editor.sourceMode, 'source ceiling admission');
        }
        final controller = FlarkFleuryController(
          editor,
          highlightWorker: await CodeHighlightWorker.start(),
        );
        final tester = FleuryTester(viewportSize: const CellSize(80, 24));
        final focus = FocusNode();
        final samples = <String, List<int>>{};
        final rssBefore = ProcessInfo.currentRss;
        final buildsBefore = CellDocumentLayout.builds;
        var measured = false;
        void measure(String name, void Function() action) {
          final timer = Stopwatch()..start();
          action();
          final frame = tester.render();
          timer.stop();
          if (measured) (samples[name] ??= []).add(timer.elapsedMicroseconds);
          require(frame.size == tester.viewportSize, 'wrong frame dimensions');
          if (name == 'insert') {
            require(editor.source == '${source}x', 'input source mismatch');
            require(
              editor.selection.extent == source.length + 1,
              'input caret mismatch',
            );
            final caret = focus.caretRect;
            require(caret != null && caret.left > 0, 'missing proving caret');
            require(
              frame.atColRow(caret!.left - 1, caret.top).grapheme == 'x',
              'first edited paint does not contain the inserted character',
            );
          }
          if (name == 'backspace') {
            require(editor.source == source, 'delete source mismatch');
          }
        }

        try {
          tester.pumpWidget(
            Theme(
              data: const ThemeData(),
              child: FlarkEditorView(
                controller: controller,
                autofocus: true,
                focusNode: focus,
                imagePreviewBuilder: (_, _, _) => const Text('IMAGE'),
              ),
            ),
          );
          tester.render();
          if (shape == 'code' && !editor.sourceMode) {
            final deadline = Stopwatch()..start();
            while (controller.colorRevision == 0 &&
                deadline.elapsed < const Duration(seconds: 5)) {
              await Future<void>.delayed(const Duration(milliseconds: 10));
              tester.render();
            }
            require(controller.colorRevision > 0, 'live code never colored');
          }
          for (var i = 0; i < 150; i++) {
            measured = i >= 50;
            measure('insert', () => tester.type('x'));
            measure(
              'backspace',
              () => tester.sendKey(const KeyEvent(KeyCode.backspace)),
            );
            measure(
              'selection',
              () => tester.sendKey(const KeyEvent(KeyCode.arrowLeft)),
            );
            tester.sendKey(const KeyEvent(KeyCode.arrowRight));
            tester.render();
            measure('resize', () {
              tester.viewportSize = CellSize(i.isEven ? 40 : 80, 24);
            });
            // Allow actual coloring-worker completions between input pairs.
            await Future<void>.delayed(Duration.zero);
          }
          require(controller.highlightError == null, 'coloring failed');
          stdout.writeln(
            jsonEncode({
              'evidence':
                  'native input-to-cell-buffer CPU; image placeholders; no presenter',
              'head': Process.runSync('git', [
                'rev-parse',
                'HEAD',
              ]).stdout.toString().trim(),
              'runtime': Platform.version,
              'shape': shape,
              'sourceBytes': utf8.encode(source).length,
              'sourceMode': editor.sourceMode,
              'colorRevision': controller.colorRevision,
              'rssDeltaBytes': ProcessInfo.currentRss - rssBefore,
              'layoutBuilds': CellDocumentLayout.builds - buildsBefore,
              'operations': {
                for (final e in samples.entries)
                  e.key: (() {
                    final sorted = e.value.toList()..sort();
                    return {
                      'samples_us': e.value,
                      'p50_us': sorted[(sorted.length * .5).ceil() - 1],
                      'p99_us': sorted[(sorted.length * .99).ceil() - 1],
                      'max_us': sorted.last,
                    };
                  })(),
              },
            }),
          );
        } finally {
          tester.dispose();
          controller.dispose();
          focus.dispose();
        }
      }
    }
  } finally {
    code.dispose();
  }
}
