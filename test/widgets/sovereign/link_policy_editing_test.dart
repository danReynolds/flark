import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Link policy editing', () {
    test('Space at markdown link label boundary exits link tail', () {
      const initial = '[test](https://google.com)';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, ' '),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '[test](https://google.com) ');
      expect(
        controller.selection,
        const TextSelection.collapsed(
          offset: '[test](https://google.com) '.length,
        ),
      );
    });

    test('Enter at markdown link label boundary exits link tail', () {
      const initial = '[test](https://google.com)';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, '\n'),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '[test](https://google.com)\n');
      expect(
        controller.selection,
        const TextSelection.collapsed(
          offset: '[test](https://google.com)\n'.length,
        ),
      );
    });

    test('Space at markdown image alt boundary exits image link tail', () {
      const initial = '![hero](https://cdn.example/hero.png)';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, ' '),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '![hero](https://cdn.example/hero.png) ');
      expect(
        controller.selection,
        const TextSelection.collapsed(
          offset: '![hero](https://cdn.example/hero.png) '.length,
        ),
      );
    });

    test('Space insertion outside markdown tail is unchanged', () {
      const initial = '[test] (https://google.com)';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.indexOf(' ');
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, ' '),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '[test]  (https://google.com)');
      expect(controller.selection, TextSelection.collapsed(offset: caret + 1));
    });

    test('Space at reference-link label boundary is unchanged', () {
      const initial = '[test][ref]';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('][');
      controller.selection = TextSelection.collapsed(offset: boundary + 1);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary + 1, boundary + 1, ' '),
        selection: TextSelection.collapsed(offset: boundary + 2),
      );

      expect(controller.text, '[test] [ref]');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: boundary + 2),
      );
    });

    test('Space exits escaped-parenthesis markdown link tail', () {
      const initial = r'[test](https://example.com/\))';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, ' '),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '$initial ');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: ('$initial ').length),
      );
    });

    test('Space exits nested-parenthesis markdown link tail', () {
      const initial = '[test](https://example.com/path_(v1))';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.indexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, ' '),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '$initial ');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: ('$initial ').length),
      );
    });

    test('Space exits nested-label markdown link with title tail', () {
      const initial = '[a [b]](https://example.com/path_(v1) "Title (v1)")';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final boundary = initial.lastIndexOf('](');
      controller.selection = TextSelection.collapsed(offset: boundary);
      controller.value = TextEditingValue(
        text: initial.replaceRange(boundary, boundary, ' '),
        selection: TextSelection.collapsed(offset: boundary + 1),
      );

      expect(controller.text, '$initial ');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: ('$initial ').length),
      );
    });
  });
}
