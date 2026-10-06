/// The workbench's documents. Pure Dart, so that tools on the Dart VM (the
/// browser fuzzer's oracle) start from the same text the page does.
library;

const tour = '''# A place to think

Flark is a live Markdown notebook. Write naturally, and your Markdown stays with you.

## Try the editing loop

Move through **bold**, *emphasis*, ~~strikethrough~~ and `inline code`. Delete a styled word, then keep typing. Use ⌘B or ⌘I to change formatting.

- Return continues a list
- Return again on an empty item leaves it
- [ ] A task to finish
- [x] A task completed

> A quote can hold **formatted words**.
> Return continues the quote.

## A small table

| Idea | State |
| --- | --- |
| Clear source | **Always** |
| Fast feedback | In progress |

```dart
final thought = 'Keep it simple';
```

---

[Markdown reference](https://commonmark.org/help/)

Your edits are saved locally. The Source button opens exact Markdown for edits that need it.
''';

String dense(int bytes) {
  final b = StringBuffer();
  for (var i = 0; b.length < bytes; i++) {
    b.write(
      '## Section $i\n\nSome **strong words** with *emphasis*, `code`, and a [link](https://example.com). A paragraph to write in.\n\n- first item\n- [x] another item\n\n> a short quote\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n',
    );
  }
  return b.toString().substring(0, bytes);
}

final presets = <String, String>{
  'Draft': '',
  'Tour': tour,
  'Dense 16 KiB': dense(16 * 1024),
  'Dense 32 KiB': dense(32 * 1024),
  'Long line': 'word ' * 1000,
};
