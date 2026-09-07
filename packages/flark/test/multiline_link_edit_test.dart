import 'package:flark/flark.dart';
import 'package:test/test.dart';
import 'support/invariants.dart';

void main() {
  final backend = createParseBackend();
  for (final prefix in ['', '> ']) {
    test(
      'punctuation after a multiline link stays outside its owner: $prefix',
      () {
        final source = '$prefix[link](   /uri\n$prefix  "title"  )';
        final e = FlarkEditor(backend, text: source, caret: source.length);
        for (final punctuation in [')', '!']) {
          expect(e.apply(InsertText(punctuation)), isTrue);
          checkInvariants(
            e.source,
            e.document.model,
            e.projection,
            'multiline link',
          );
          expect(
            e.projection.rows.single.text,
            punctuation == ')' ? 'link)' : 'link)!',
          );
        }
        e.apply(const Undo());
        expect(e.source, source);
        expect(e.selection.extent, source.length);
        e.apply(const Redo());
        expect(e.projection.rows.single.text, 'link)!');
      },
    );
  }
}
