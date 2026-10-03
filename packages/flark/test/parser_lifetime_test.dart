import 'dart:isolate';

import 'package:flark/flark.dart';
import 'package:flark/src/parse/parser_memory.dart';
import 'package:test/test.dart';

void main() {
  test('a parser from createParseBackend disposes through the interface', () {
    final parser = createParseBackend();
    expect(parser.parse('# Title').blockCount, 2);
    parser.dispose();
    parser.dispose();
    expect(() => parser.parse('# Title'), throwsStateError);
  });

  test('a parser keeps one finalizer attachment until it is disposed', () {
    final before = ParserMemory.attachedBlocks;
    final parser = createParseBackend();
    expect(ParserMemory.attachedBlocks, before + 1);
    // Growing the input buffer replaces the attachment instead of adding one.
    expect(parser.parse('word ' * 4000).blockCount, 2);
    expect(parser.parse('short').blockCount, 2);
    expect(ParserMemory.attachedBlocks, before + 1);
    parser.dispose();
    parser.dispose();
    expect(ParserMemory.attachedBlocks, before);
    // A parser dropped without dispose stays attached, so the finalizer frees
    // its memory once the parser is collected.
    createParseBackend().parse('dropped');
    expect(ParserMemory.attachedBlocks, before + 1);
  });

  test(
    'a parser and an editor holding it cannot be copied to an isolate',
    () async {
      final parser = createParseBackend();
      addTearDown(parser.dispose);
      // The closures only read, so even an allowed copy would free nothing:
      // a regression fails these expectations rather than the test process.
      await expectLater(
        () => Isolate.run(() => parser.schemaVersion),
        throwsArgumentError,
      );
      final editor = FlarkEditor(parser, text: '# Title');
      await expectLater(
        () => Isolate.run(() => editor.source.length),
        throwsArgumentError,
      );
      expect(parser.parse('still usable').blockCount, 2);
    },
  );

  test(
    'the input buffer fits the worst case within the u32 the parser takes',
    () {
      expect(ParserMemory.inputCapacityFor(1000), 6000);
      // Headroom stops at the limit instead of overflowing it.
      expect(
        ParserMemory.inputCapacityFor(0x40000000),
        ParserMemory.maxInputBytes,
      );
      expect(
        ParserMemory.inputCapacityFor(0x55555555),
        ParserMemory.maxInputBytes,
      );
      // A source whose UTF-8 may not fit is refused, never truncated.
      expect(ParserMemory.inputCapacityFor(0x55555556), isNull);
    },
  );
}
