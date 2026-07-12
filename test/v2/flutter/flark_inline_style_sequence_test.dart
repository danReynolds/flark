import 'dart:math';

import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/inline_sequence_harness.dart';

/// User-level editing sequences gated by [InlineSequence]: after every step,
/// the display must equal what the user typed, and a fresh caret-free parse
/// of `controller.markdown` must render the identical display. See the
/// harness for the gate definitions; see flark_markdown_render_spec_test.dart
/// for the declarative one-string render specs.
void main() {
  const strong = FlarkMarkdownInlineStyle.strong;
  const emphasis = FlarkMarkdownInlineStyle.emphasis;
  const strikethrough = FlarkMarkdownInlineStyle.strikethrough;
  const inlineCode = FlarkMarkdownInlineStyle.inlineCode;

  group('armed typing with edge whitespace', () {
    for (final (style, open) in const [
      (strong, '**'),
      (emphasis, '*'),
      (strikethrough, '~~'),
    ]) {
      test('trailing space, toggle off, keep typing ($open)', () async {
        final seq = await InlineSequence.start('');
        addTearDown(seq.dispose);
        await seq.toggle(style);
        await seq.type('hello world ');
        seq.expectSource('${open}hello world$open ');
        seq.expectActive(style, active: true);
        await seq.toggle(style);
        seq.expectActive(style, active: false);
        await seq.type('x');
        seq.expectSource('${open}hello world$open x');
      });

      test('trailing space, keep typing styled ($open)', () async {
        final seq = await InlineSequence.start('');
        addTearDown(seq.dispose);
        await seq.toggle(style);
        await seq.type('hello ');
        seq.expectSource('${open}hello$open ');
        await seq.type('world');
        seq.expectSource('${open}hello world$open');
        await seq.toggle(style);
        await seq.type('!');
        seq.expectSource('${open}hello world$open!');
      });
    }

    test('leading space stays outside the run', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type(' h');
      seq.expectSource(' **h**');
      await seq.type('i');
      seq.expectSource(' **hi**');
    });

    test('whitespace-only typing keeps the style armed', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type(' ');
      seq.expectSource(' ');
      seq.expectActive(strong, active: true);
      await seq.type('h');
      seq.expectSource(' **h**');
    });

    test('whitespace-only typing then toggle off stays plain', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type(' ');
      await seq.toggle(strong);
      seq.expectActive(strong, active: false);
      await seq.type('x');
      seq.expectSource(' x');
    });

    test('spaces typed one keystroke at a time re-enter the run', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('a');
      seq.expectSource('**a**');
      await seq.type(' ');
      seq.expectSource('**a** ');
      await seq.type(' ');
      seq.expectSource('**a**  ');
      await seq.type('b');
      seq.expectSource('**a  b**');
    });

    test('stacked bold+italic with trailing space', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.toggle(emphasis);
      await seq.type('h ');
      seq.expectSource('***h*** ');
      await seq.type('x');
      seq.expectSource('***h x***');
    });

    test('inline code keeps edge whitespace inside its span', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(inlineCode);
      await seq.type('x ');
      seq.expectSource('`x `');
    });
  });

  group('typing at the edges of an existing run', () {
    test('space at the trailing edge commits outside and re-enters', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(5);
      await seq.type(' ');
      seq.expectSource('**hello** ');
      seq.expectActive(strong, active: true);
      await seq.type('world');
      seq.expectSource('**hello world**');
    });

    test('space at the leading edge commits outside', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(0);
      await seq.type(' ');
      seq.expectSource(' **hello**');
    });

    test('caret moves away and typing elsewhere never leaks markers', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('hello ');
      seq.expectSource('**hello** ');
      await seq.moveCaret(0);
      // A caret at the run's leading edge types into the run (caret affinity),
      // which is valid and display-faithful — the gates in settle() are the
      // real assertion here.
      await seq.type('y');
      seq.expectSource('**yhello** ');
    });
  });

  group('muted exits', () {
    test('middle split around straddled whitespace', () async {
      final seq = await InlineSequence.start('**foo bar**');
      addTearDown(seq.dispose);
      await seq.moveCaret(4);
      await seq.toggle(strong);
      await seq.type('x');
      seq.expectSource('**foo** x**bar**');
    });

    test('trailing-edge exit without whitespace is untouched', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(5);
      await seq.toggle(strong);
      await seq.type('x');
      seq.expectSource('**hello**x');
    });

    test('middle split without whitespace', () async {
      final seq = await InlineSequence.start('**bold**');
      addTearDown(seq.dispose);
      await seq.moveCaret(2);
      await seq.toggle(strong);
      await seq.type('x');
      seq.expectSource('**bo**x**ld**');
    });
  });

  group('deletions at run edges', () {
    test('deleting the last word-character relocates the close', () async {
      final seq = await InlineSequence.start('**foo x** bar');
      addTearDown(seq.dispose);
      await seq.moveCaret(5);
      await seq.backspace();
      seq.expectSource('**foo**  bar');
      await seq.type('y');
      seq.expectSource('**foo y** bar');
    });

    test('deleting the first word-character relocates the open', () async {
      final seq = await InlineSequence.start('**x foo**');
      addTearDown(seq.dispose);
      await seq.moveCaret(1);
      await seq.backspace();
      seq.expectSource(' **foo**');
    });

    test('deleting all content dissolves the run', () async {
      final seq = await InlineSequence.start('**ab**');
      addTearDown(seq.dispose);
      await seq.moveCaret(2);
      await seq.backspace();
      seq.expectSource('**a**');
      await seq.backspace();
      seq.expectSource('');
    });
  });

  group('Enter at run edges', () {
    test('Enter at the trailing edge lands after the run', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(5);
      await seq.pressEnter();
      seq.expectSource('**hello**\n');
    });

    test('Enter at the leading edge lands before the run', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(0);
      await seq.pressEnter();
      seq.expectSource('\n**hello**');
    });

    test('Enter mid-run is a legal softbreak', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(3);
      await seq.pressEnter();
      seq.expectSource('**hel\nlo**');
    });
  });

  group('selection wraps', () {
    test(
      'wrapping a selection with trailing whitespace hugs the core',
      () async {
        final seq = await InlineSequence.start('hello world');
        addTearDown(seq.dispose);
        await seq.select(0, 6);
        await seq.toggle(strong);
        seq.expectSource('**hello** world');
      },
    );

    test(
      'wrapping a selection with leading whitespace hugs the core',
      () async {
        final seq = await InlineSequence.start('hello world');
        addTearDown(seq.dispose);
        await seq.select(5, 11);
        await seq.toggle(strong);
        seq.expectSource('hello **world**');
      },
    );

    test('wrapping a whitespace-only selection is a no-op', () async {
      final seq = await InlineSequence.start('hello world');
      addTearDown(seq.dispose);
      await seq.select(5, 6);
      await seq.toggle(strong);
      seq.expectSource('hello world');
    });

    test('typing a delimiter over a whitespace-edged selection', () async {
      final seq = await InlineSequence.start('foo bar');
      addTearDown(seq.dispose);
      await seq.select(0, 4);
      final before = seq.display;
      expect(
        seq.controller.applyProjectedTextEdit(
          oldDisplayText: before,
          newDisplayText: '*foo* bar',
        ),
        isTrue,
      );
      await seq.settle('foo bar');
      seq.expectSource('*foo* bar');
    });
  });

  group('hand-typed literal markers are never rewritten', () {
    test('literal `**foo **` stays literal through styling actions', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.typeSource('**foo **');
      seq.expectSource('**foo **');
      await seq.toggle(strong);
      await seq.type('x');
      // The literal text is untouched; whatever the keystroke produced must
      // itself round-trip (the gates already enforced it).
      expect(seq.controller.markdown, contains('**foo **'));
    });
  });

  group('deletions beside hidden markers', () {
    test(
      'backspace before a nested close deletes content, not markers',
      () async {
        // Found by the randomized gate (seed 1): with the caret hugging the
        // hidden `~~` close of `*~~ff~~*`, backspace must remove the `f` — a
        // deletion mapping that swallows one half of a marker pair strands the
        // other half as literal text.
        final seq = await InlineSequence.start('');
        addTearDown(seq.dispose);
        await seq.toggle(emphasis);
        await seq.toggle(strikethrough);
        await seq.type('f');
        await seq.type('f');
        seq.expectSource('*~~ff~~*');
        await seq.backspace();
        seq.expectSource('*~~f~~*');
        await seq.backspace();
        seq.expectSource('');
      },
    );
  });

  group('inline code editing', () {
    // Code spans are exempt from every whitespace-relocation rule: their
    // backticks legally hug whitespace and their content is verbatim.
    test('muted code exit at the trailing edge', () async {
      final seq = await InlineSequence.start('`code`');
      addTearDown(seq.dispose);
      await seq.moveCaret(4);
      await seq.toggle(inlineCode);
      await seq.type('x');
      seq.expectSource('`code`x');
    });

    test('muted code exit mid-span keeps whitespace as content', () async {
      final seq = await InlineSequence.start('`foo bar`');
      addTearDown(seq.dispose);
      await seq.moveCaret(4);
      await seq.toggle(inlineCode);
      await seq.type('x');
      seq.expectSource('`foo `x`bar`');
    });

    test('typing a space at the code trailing edge stays inside', () async {
      final seq = await InlineSequence.start('`ab`');
      addTearDown(seq.dispose);
      await seq.moveCaret(2);
      await seq.type(' ');
      seq.expectSource('`ab `');
    });

    test(
      'deleting to a trailing space inside code does not relocate',
      () async {
        final seq = await InlineSequence.start('`a x`');
        addTearDown(seq.dispose);
        await seq.moveCaret(3);
        await seq.backspace();
        seq.expectSource('`a `');
      },
    );
  });

  group('links stay untouched by edge repair', () {
    test('typing a space at the end of a link label', () async {
      final seq = await InlineSequence.start('[docs](u) tail');
      addTearDown(seq.dispose);
      await seq.moveCaret(4);
      await seq.type(' ');
      // A caret at the label's trailing display edge maps outside the link
      // (unlike emphasis runs, links are not extended by adjacent typing),
      // and no edge repair touches the link's markers.
      seq.expectSource('[docs](u)  tail');
    });
  });

  group('multi-line editing', () {
    test('armed typing with a trailing space on a second paragraph', () async {
      final seq = await InlineSequence.start('first\n\n');
      addTearDown(seq.dispose);
      await seq.moveCaret(seq.display.length);
      await seq.toggle(strong);
      await seq.type('second ');
      seq.expectSource('first\n\n**second** ');
      await seq.type('x');
      seq.expectSource('first\n\n**second x**');
    });

    test('deletion repair on a later line', () async {
      final seq = await InlineSequence.start('a\n\n**foo x**');
      addTearDown(seq.dispose);
      await seq.moveCaret(seq.display.length);
      await seq.backspace();
      seq.expectSource('a\n\n**foo** ');
      await seq.type('y');
      seq.expectSource('a\n\n**foo y**');
    });
  });

  group('IME-style composition', () {
    test('composing inside an armed run commits canonically', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('a');
      seq.expectSource('**a**');
      await seq.compose(['w', 'wo', 'world ']);
      seq.expectSource('**aworld** ');
      await seq.type('x');
      seq.expectSource('**aworld x**');
    });

    test('composition converting across stages inside a run', () async {
      final seq = await InlineSequence.start('**hi**');
      addTearDown(seq.dispose);
      await seq.moveCaret(2);
      await seq.compose(['k', 'ka', 'かに']);
      seq.expectSource('**hiかに**');
    });
  });

  group('undo through repairs', () {
    test(
      'undo restores the pre-repair source after a deletion repair',
      () async {
        final seq = await InlineSequence.start('**foo x**');
        addTearDown(seq.dispose);
        await seq.moveCaret(5);
        await seq.backspace();
        seq.expectSource('**foo** ');
        await seq.undoExpecting('foo x');
        seq.expectSource('**foo x**');
        await seq.redoExpecting('foo ');
        seq.expectSource('**foo** ');
      },
    );

    test('undo restores a dissolved nested run', () async {
      final seq = await InlineSequence.start('*~~f~~*');
      addTearDown(seq.dispose);
      await seq.moveCaret(1);
      await seq.backspace();
      seq.expectSource('');
      await seq.undoExpecting('f');
      seq.expectSource('*~~f~~*');
    });
  });

  group('paste (markdown as one insertion)', () {
    test('pasting styled markdown converts like loading it', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.paste('**bold** and *em*');
      seq.expectSource('**bold** and *em*');
      expect(seq.display, 'bold and em');
    });

    test('pasting invalid markdown stays literal', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.paste('**foo **bar');
      seq.expectSource('**foo **bar');
      expect(seq.display, '**foo **bar');
    });

    test('pasting text with trailing space while a style is armed', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('a');
      await seq.paste(' tail ');
      seq.expectSource('**a tail** ');
      await seq.type('b');
      seq.expectSource('**a tail b**');
    });

    test('pasting into the middle of a run', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(3);
      await seq.paste(' mid ');
      seq.expectSource('**hel mid lo**');
      expect(seq.display, 'hel mid lo');
    });
  });

  group('selection replacement', () {
    test('typing over a selection inside a run repairs edges', () async {
      final seq = await InlineSequence.start('**hello world**');
      addTearDown(seq.dispose);
      await seq.select(6, 11);
      await seq.replaceSelection(' ');
      seq.expectSource('**hello**  ');
    });

    test(
      'typing over a selection spanning styled and plain text',
      // The selection's source range covers the run's hidden closing marker
      // but not its opening marker; the plain replacement would orphan the
      // open ('**boxin', literal '**' in the display). The crossing repair
      // relocates the close instead: the typed text joins the run (the
      // selection started inside it) and the pair stays balanced.
      () async {
        final seq = await InlineSequence.start('**bold** plain');
        addTearDown(seq.dispose);
        // Display [2, 8) is 'ld pla' — source [4, 12), covering the hidden
        // closing '**' at [6, 8) but not the opening one.
        await seq.select(2, 8);
        await seq.replaceSelection('x');
        expect(seq.display, 'boxin');
        seq.expectSource('**box**in');
      },
    );

    test("replacing a whole run's content keeps the style", () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.select(0, 5);
      await seq.replaceSelection('bye');
      seq.expectSource('**bye**');
    });

    test('typing over a selection entering a run from plain text', () async {
      final seq = await InlineSequence.start('plain **bold**');
      addTearDown(seq.dispose);
      // Display [3, 8) is 'in bo' — source [3, 10), covering the hidden
      // opening '**' at [6, 8) but not the closing one. The typed text stays
      // outside the run (the selection started outside) and the open
      // relocates past it.
      await seq.select(3, 8);
      await seq.replaceSelection('x');
      expect(seq.display, 'plaxld');
      seq.expectSource('plax**ld**');
    });

    test(
      'typing over a selection spanning two same-style runs merges them',
      () async {
        final seq = await InlineSequence.start('**bold** and **brave**');
        addTearDown(seq.dispose);
        // Display [2, 12) is 'ld and br' + — source [4, 18), covering A's
        // hidden close and B's hidden open; the runs merge around the typed
        // text.
        await seq.select(2, 12);
        await seq.replaceSelection('x');
        expect(seq.display, 'boxve');
        seq.expectSource('**boxve**');
      },
    );

    test('typing over a selection crossing a code span boundary', () async {
      final seq = await InlineSequence.start('`code` plain');
      addTearDown(seq.dispose);
      // Display [2, 7) is 'de pl' — source [3, 10), covering the hidden
      // closing backtick. An orphaned backtick would swallow the rest of
      // the document as a code span; the close relocates instead (with no
      // whitespace splitting — code whitespace is content).
      await seq.select(2, 7);
      await seq.replaceSelection('x');
      expect(seq.display, 'coxain');
      seq.expectSource('`cox`ain');
    });
  });

  group('deletions joining runs', () {
    test(
      'deleting the space between two same-style runs',
      // The plain deletion would produce '**a****b**' — the two runs' inner
      // delimiters fuse into a literal '****' that leaks into the display.
      // The joining repair merges the neighbors into one run instead.
      () async {
        final seq = await InlineSequence.start('**a** **b**');
        addTearDown(seq.dispose);
        await seq.moveCaret(2);
        await seq.backspace();
        seq.expectSource('**ab**');
      },
    );

    test(
      'deleting across a run boundary via selection',
      // Same root cause as the marker-crossing replacement — the deletion
      // covers the hidden close but not the open. The crossing repair
      // relocates the close to the deletion boundary ('**bo**ain') instead
      // of orphaning the open ('**boain').
      () async {
        final seq = await InlineSequence.start('**bold** plain');
        addTearDown(seq.dispose);
        await seq.select(2, 7);
        await seq.replaceSelection('', sourceSemantics: true);
        expect(seq.display, 'boain');
        seq.expectSource('**bo**ain');
      },
    );

    test(
      'deleting the space between same-character different-style runs',
      // '**a***b*' would fuse into a '***' delimiter run and comrak leaves
      // '*b*' literal, so the plain deletion leaks. The joining repair
      // rewrites the second run with its alternate marker character.
      () async {
        final seq = await InlineSequence.start('**a** *b*');
        addTearDown(seq.dispose);
        await seq.moveCaret(2);
        await seq.backspace();
        seq.expectSource('**a**_b_');
        expect(seq.display, 'ab');
      },
    );

    test(
      'deleting the space between different-character runs stays plain',
      // '**a**~~b~~' parses as two valid adjacent runs — different marker
      // characters never fuse, so no repair fires and the plain deletion
      // stands.
      () async {
        final seq = await InlineSequence.start('**a** ~~b~~');
        addTearDown(seq.dispose);
        await seq.moveCaret(2);
        await seq.backspace();
        seq.expectSource('**a**~~b~~');
        expect(seq.display, 'ab');
      },
    );

    test(
      'deleting the space between two stacked bold-italic runs',
      // Stacked neighbors merge cluster-chain-wise: dropping both inner
      // '***' clusters yields one nested run, never the fused literal
      // '***a******b***'.
      () async {
        final seq = await InlineSequence.start('***a*** ***b***');
        addTearDown(seq.dispose);
        await seq.moveCaret(2);
        await seq.backspace();
        seq.expectSource('***ab***');
        expect(seq.display, 'ab');
      },
    );
  });

  group('muted exits, remaining edges', () {
    test('muted exit at the leading edge', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(0);
      await seq.toggle(strong);
      await seq.type('x');
      seq.expectSource('x**hello**');
    });

    test('muted exit with whitespace text at the leading edge', () async {
      final seq = await InlineSequence.start('**hello**');
      addTearDown(seq.dispose);
      await seq.moveCaret(0);
      await seq.toggle(strong);
      await seq.type(' ');
      seq.expectSource(' **hello**');
      await seq.type('y');
      seq.expectSource(' y**hello**');
    });
  });

  group('style switching with whitespace', () {
    test('switch style after trailing space', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(emphasis);
      await seq.type('hello ');
      seq.expectSource('*hello* ');
      await seq.toggle(strong);
      await seq.type('x');
      // The em continuation unions with the newly armed strong: x lands in a
      // fresh sibling run carrying both styles.
      seq.expectSource('*hello* ***x***');
    });

    test(
      'switch inside a run at its trailing edge (last action wins)',
      () async {
        final seq = await InlineSequence.start('**bold**');
        addTearDown(seq.dispose);
        await seq.moveCaret(4);
        await seq.toggle(emphasis);
        await seq.type('x');
        seq.expectSource('**bold**_x_');
      },
    );

    test(
      'switch is symmetric: italic to bold uses the alternate marker',
      () async {
        final seq = await InlineSequence.start('*it*');
        addTearDown(seq.dispose);
        await seq.moveCaret(2);
        await seq.toggle(strong);
        await seq.type('z');
        seq.expectSource('*it*__z__');
      },
    );
  });

  group('Enter composition', () {
    test('Enter at the end of a styled list item continues the list outside '
        'the run', () async {
      final seq = await InlineSequence.start('- **item**');
      addTearDown(seq.dispose);
      await seq.moveCaret(seq.display.length);
      await seq.pressEnter(expectedDisplay: '${seq.display}\n');
      // The newline and the continuation marker must not land inside the run.
      expect(seq.controller.markdown, isNot(contains('item\n')));
      seq.expectSource('- **item**\n- ');
    });

    test('undo after a continuation then keep typing', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('a ');
      await seq.type('b');
      seq.expectSource('**a b**');
      await seq.undoExpecting('a ');
      seq.expectSource('**a** ');
      // Pending styles are not recorded in history, so typing after the undo
      // is plain text.
      await seq.type('c');
      seq.expectSource('**a** c');
    });
  });

  group('undo and redo preserve validity', () {
    test('undoing through a continuation sequence', () async {
      final seq = await InlineSequence.start('');
      addTearDown(seq.dispose);
      await seq.toggle(strong);
      await seq.type('hello ');
      await seq.type('x');
      seq.expectSource('**hello x**');
      await seq.undoExpecting('hello ');
      seq.expectSource('**hello** ');
      await seq.undoExpecting('');
      seq.expectSource('');
      await seq.redoExpecting('hello ');
      seq.expectSource('**hello** ');
      await seq.redoExpecting('hello x');
      seq.expectSource('**hello x**');
    });
  });

  group('randomized sequences', () {
    test('random raw markdown typing never breaks the round-trip', () async {
      // The "user speaking markdown" fuzz: an alphabet including the
      // delimiter characters, gated per keystroke by the export round-trip
      // (display fidelity does not apply — completing or breaking a marker
      // pair legitimately re-renders).
      const alphabet = 'ab *_~`';
      for (var seed = 0; seed < 6; seed += 1) {
        final random = Random(seed);
        final seq = await InlineSequence.start('');
        final journal = <String>[];
        for (var step = 0; step < 30; step += 1) {
          try {
            switch (random.nextInt(4)) {
              case 0 || 1:
                final char = alphabet[random.nextInt(alphabet.length)];
                journal.add("typeSource('$char')");
                await seq.typeSource(char);
              case 2:
                final length = seq.display.length;
                final offset = length == 0 ? 0 : random.nextInt(length + 1);
                journal.add('moveCaret($offset)');
                await seq.moveCaret(offset);
              case 3:
                if (seq.displayCaret > 0) {
                  journal.add('backspaceSource()');
                  await seq.backspaceSource();
                }
            }
          } catch (error) {
            fail(
              'seed $seed failed at step $step '
              '(source "${seq.controller.markdown}"):\n'
              '${journal.join('\n')}\n$error',
            );
          }
        }
        seq.dispose();
      }
    });

    test(
      'random typing, toggling, and caret movement never leak markers',
      () async {
        for (var seed = 0; seed < 8; seed += 1) {
          final random = Random(seed);
          final seq = await InlineSequence.start('');
          const styles = [strong, emphasis, strikethrough];
          const letters = 'abcdef';
          final journal = <String>[];
          for (var step = 0; step < 40; step += 1) {
            try {
              switch (random.nextInt(6)) {
                case 0 || 1:
                  final letter = letters[random.nextInt(letters.length)];
                  journal.add("type('$letter')");
                  await seq.type(letter);
                case 2:
                  journal.add("type(' ')");
                  await seq.type(' ');
                case 3:
                  final style = styles[random.nextInt(styles.length)];
                  journal.add('toggle($style)');
                  await seq.toggle(style);
                case 4:
                  final length = seq.display.length;
                  final offset = length == 0 ? 0 : random.nextInt(length + 1);
                  journal.add('moveCaret($offset)');
                  await seq.moveCaret(offset);
                case 5:
                  if (seq.displayCaret > 0) {
                    journal.add('backspace()');
                    await seq.backspace();
                  }
              }
            } catch (error) {
              fail(
                'seed $seed failed at step $step '
                '(source "${seq.controller.markdown}"):\n'
                '${journal.join('\n')}\n$error',
              );
            }
          }
          seq.dispose();
        }
      },
    );
  });
}
