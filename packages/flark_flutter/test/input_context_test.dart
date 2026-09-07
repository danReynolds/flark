import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/input_context.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

TextEditingValue at(String text, int caret) => TextEditingValue(
  text: text,
  selection: TextSelection.collapsed(offset: caret),
);

void main() {
  test('platform input reports the source ceiling and then recovers', () {
    final c = FlarkController(
      FlarkEditor(
        createParseBackend(),
        text: 'abcd',
        caret: 4,
        syncLimit: 2,
        sourceLimit: 4,
      ),
    );
    expect(c.receive(at('abcde', 5)), isFalse);
    expect(c.text, 'abcd');
    expect(c.notice, 'This document exceeds the writable source limit.');
    c.command(const SetSelection(0, 4));
    expect(c.receive(at('ab', 2)), isTrue);
    expect(c.text, 'ab');
    expect(c.notice, isNull);
    expect(c.receive(at('abc', 3)), isTrue);
    expect(c.text, 'abc');
    c.dispose();
  });
  test('surrounding input maps an edit back into exact full source', () {
    final source = '${'before ' * 300}${'after ' * 300}';
    final input = InputContext.of(at(source, 2100));
    expect(input.value.text.length, lessThanOrEqualTo(514));
    final local = input.value;
    final caret = local.selection.extentOffset;
    final next = at(
      local.text.replaceRange(caret, caret, 'two  words'),
      caret + 10,
    );
    final full = input.expand(next)!;
    expect(full.text, source.replaceRange(2100, 2100, 'two  words'));
    expect(full.selection.extentOffset, 2110);
    final kept = InputContext.of(full, previous: input);
    expect(kept.rebased, isFalse);
    expect(kept.value, next);
  });

  test('context edges preserve complete Unicode graphemes and CRLF', () {
    final source = 'a👩🏽‍💻e\u0301\r\n' * 500;
    final boundaries = <int>{0};
    var offset = 0;
    for (final grapheme in source.characters) {
      offset += grapheme.length;
      boundaries.add(offset);
    }
    for (final caret in boundaries.where((p) => p > 1000 && p < 1200)) {
      final input = InputContext.of(at(source, caret));
      expect(boundaries, contains(input.start));
      expect(boundaries, contains(input.end));
      expect(input.expand(input.value), at(source, caret));
    }
  });

  test('wide reverse selection and active composition are never clipped', () {
    final source = 'a' * 5000;
    final selected = TextEditingValue(
      text: source,
      selection: const TextSelection(baseOffset: 4000, extentOffset: 1000),
    );
    final input = InputContext.of(selected);
    expect(input.expand(input.value), selected);
    final local = input.value;
    final replaced = local.copyWith(
      text: local.text.replaceRange(
        local.selection.start,
        local.selection.end,
        'x',
      ),
      selection: TextSelection.collapsed(offset: local.selection.start + 1),
    );
    expect(input.expand(replaced)!.text, '${'a' * 1000}x${'a' * 1000}');
    expect(input.expand(replaced)!.selection.extentOffset, 1001);

    var full = at(
      source,
      2501,
    ).copyWith(composing: const TextRange(start: 2500, end: 2501));
    var composing = InputContext.of(full);
    final start = composing.start;
    for (var i = 0; i < 1100; i++) {
      full = full.copyWith(
        text: full.text.replaceRange(2501 + i, 2501 + i, 'x'),
        selection: TextSelection.collapsed(offset: 2502 + i),
        composing: TextRange(start: 2500, end: 2502 + i),
      );
      composing = InputContext.of(full, previous: composing);
      expect(composing.start, start);
      expect(composing.rebased, isFalse);
      expect(composing.expand(composing.value), full);
    }
  });

  test('invalid local ranges cannot edit neighboring source', () {
    final input = InputContext.of(at('a' * 5000, 2500));
    expect(input.expand(at('x', 2)), isNull);
    expect(
      input.expand(
        at('x', 1).copyWith(composing: const TextRange(start: 0, end: 2)),
      ),
      isNull,
    );
  });

  final backend = createParseBackend();
  Future<void> mount(WidgetTester tester, FlarkController c) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
  }

  TextEditingValue remote(WidgetTester tester) =>
      TextEditingValue.fromJSON(tester.testTextInput.editingState!);
  int client(WidgetTester tester) =>
      (tester.testTextInput.log
                      .lastWhere((call) => call.method == 'TextInput.setClient')
                      .arguments
                  as List)
              .first
          as int;
  Future<void> delta(
    WidgetTester tester,
    int clientId,
    TextEditingValue before,
    String inserted,
  ) async {
    final offset = before.selection.extentOffset;
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      SystemChannels.textInput.name,
      SystemChannels.textInput.codec.encodeMethodCall(
        MethodCall('TextInputClient.updateEditingStateWithDeltas', [
          clientId,
          {
            'deltas': [
              {
                'oldText': before.text,
                'deltaText': inserted,
                'deltaStart': offset,
                'deltaEnd': offset,
                'selectionBase': offset + inserted.length,
                'selectionExtent': offset + inserted.length,
                'selectionAffinity': 'TextAffinity.downstream',
                'selectionIsDirectional': false,
                'composingBase': -1,
                'composingExtent': -1,
              },
            ],
          },
        ]),
      ),
      (_) {},
    );
  }

  testWidgets(
    'platform deltas edit full source without echoing accepted input',
    (tester) async {
      final source = 'a' * 4000;
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 2000),
      );
      await mount(tester, c);
      final before = remote(tester), id = client(tester);
      expect(before.text.length, lessThan(1024));
      tester.testTextInput.log.clear();
      await delta(tester, id, before, 'x');
      expect(c.text, source.replaceRange(2000, 2000, 'x'));
      expect(c.editor.selection.extent, 2001);
      await tester.pump();
      expect(
        tester.testTextInput.log.where(
          (call) => call.method == 'TextInput.setEditingState',
        ),
        isEmpty,
      );
      await delta(tester, id, before, 'stale');
      expect(c.text, source.replaceRange(2000, 2000, 'x'));
      expect(c.editor.selection.extent, 2001);
      expect(
        remote(tester).text,
        before.text.replaceRange(
          before.selection.extentOffset,
          before.selection.extentOffset,
          'x',
        ),
      );
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'windowed input resynchronizes a formatting correction before the next key',
    (tester) async {
      final prefix = '${'p' * 1500}\n\n';
      final suffix = '\n\n${'q' * 1500}';
      final c = FlarkController(
        FlarkEditor(
          backend,
          text: '${prefix}a *x* b$suffix',
          caret: prefix.length + 4,
        ),
      );
      await mount(tester, c);
      final before = remote(tester),
          caret = remote(tester).selection.extentOffset;
      tester.testTextInput.updateEditingValue(
        at(before.text.replaceRange(caret - 1, caret, ''), caret - 1),
      );
      expect(c.text, '${prefix}a  b$suffix');
      final corrected = remote(tester);
      tester.testTextInput.updateEditingValue(
        at(
          corrected.text.replaceRange(
            corrected.selection.extentOffset,
            corrected.selection.extentOffset,
            'y',
          ),
          corrected.selection.extentOffset + 1,
        ),
      );
      expect(c.text, '${prefix}a *y* b$suffix');
      expect(c.editor.selection.extent, prefix.length + 4);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'rebasing invalidates queued input from the old local coordinates',
    (tester) async {
      final source = '${'a' * 2000}\n\n${'b' * 2000}';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 1000),
      );
      await mount(tester, c);
      final before = remote(tester), oldClient = client(tester);
      c.command(const SetSelection.caret(3000));
      await tester.pump();
      expect(client(tester), isNot(oldClient));
      await delta(tester, oldClient, before, 'stale');
      expect(c.text, source);
      await delta(tester, client(tester), remote(tester), 'next');
      expect(c.text, source.replaceRange(3000, 3000, 'next'));
      expect(c.editor.selection.extent, 3004);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'continued Unicode typing crosses input contexts without losing the next key',
    (tester) async {
      final source = '${'a' * 1500}\n\n${'b' * 1500}';
      final c = FlarkController(FlarkEditor(backend, text: source, caret: 750));
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      var expected = source, caret = 750;
      var platform = remote(tester);
      final firstClient = client(tester);
      for (final character in (' delta  👩🏽‍💻' * 50).characters) {
        final before = platform, offset = platform.selection.extentOffset;
        paints.clear();
        platform = at(
          before.text.replaceRange(offset, offset, character),
          offset + character.length,
        );
        final logStart = tester.testTextInput.log.length;
        tester.testTextInput.updateEditingValue(platform);
        // The stub records outgoing corrections only. A real platform already
        // holds the value it sent, even when the client correctly avoids echo.
        for (final call in tester.testTextInput.log.skip(logStart)) {
          if (call.method == 'TextInput.setEditingState') {
            platform = TextEditingValue.fromJSON(
              Map<String, dynamic>.from(call.arguments as Map),
            );
          }
        }
        expected = expected.replaceRange(caret, caret, character);
        caret += character.length;
        expect(c.text, expected);
        expect(c.editor.selection.extent, caret);
        expect(c.editor.sourceMode, isFalse);
        await tester.pump();
        expect(paints, isNotEmpty);
        expect(paints.last.caretSource, caret);
        expect(paints.last.snapshot.source, expected);
      }
      expect(client(tester), isNot(firstClient));
      c.command(const Undo());
      expect(c.text, source);
      expect(c.editor.selection.extent, 750);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'windowed composition commits and undoes as one exact source edit',
    (tester) async {
      final source = 'a' * 4000;
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: 2000),
      );
      await mount(tester, c);
      final before = remote(tester),
          caret = remote(tester).selection.extentOffset;
      final composing = before.copyWith(
        text: before.text.replaceRange(caret, caret, 'é'),
        selection: TextSelection.collapsed(offset: caret + 1),
        composing: TextRange(start: caret, end: caret + 1),
      );
      tester.testTextInput.updateEditingValue(composing);
      expect(c.value.composing, const TextRange(start: 2000, end: 2001));
      tester.testTextInput.updateEditingValue(
        composing.copyWith(composing: TextRange.empty),
      );
      expect(c.text, source.replaceRange(2000, 2000, 'é'));
      expect(c.value.composing, TextRange.empty);
      c.command(const Undo());
      expect(c.text, source);
      expect(c.editor.selection.extent, 2000);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
