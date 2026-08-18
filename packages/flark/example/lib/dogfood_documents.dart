// SPDX-License-Identifier: MIT

enum DogfoodDocumentPreset {
  productTour(
    label: 'Product tour',
    description: 'Rich GFM and Unicode editing cases',
  ),
  prose1MiB(
    label: 'Prose · 1 MiB',
    description: 'Ordinary mixed Markdown',
    targetBytes: 1 * 1024 * 1024,
  ),
  prose5MiB(
    label: 'Prose · 5 MiB',
    description: 'Large ordinary document',
    targetBytes: 5 * 1024 * 1024,
  ),
  prose10MiB(
    label: 'Prose · 10 MiB',
    description: 'Upper dogfood tier',
    targetBytes: 10 * 1024 * 1024,
  ),
  giantLine5MiB(
    label: 'Giant line · 5 MiB',
    description: 'Oversized-fragment stress shape',
    targetBytes: 5 * 1024 * 1024,
  ),
  denseBlocks1MiB(
    label: 'Dense blocks · 1 MiB',
    description: 'Many short headings and paragraphs',
    targetBytes: 1 * 1024 * 1024,
  ),
  streamed10MiB(
    label: 'Streamed · 10 MiB',
    description: 'Type in the head while the tail admits',
    targetBytes: 10 * 1024 * 1024,
    streamed: true,
  );

  const DogfoodDocumentPreset({
    required this.label,
    required this.description,
    this.targetBytes,
    this.streamed = false,
  });

  final String label;
  final String description;
  final int? targetBytes;

  /// Opens through the streamed admission path instead of the buffered one,
  /// so the certified head paints and accepts typing while the rest of the
  /// document is still being admitted. Requires a native library built with
  /// the `opening-session` cargo feature.
  final bool streamed;
}

String buildDogfoodDocument(DogfoodDocumentPreset preset) => switch (preset) {
  DogfoodDocumentPreset.productTour => _productTour,
  DogfoodDocumentPreset.prose1MiB ||
  DogfoodDocumentPreset.prose5MiB ||
  DogfoodDocumentPreset.prose10MiB ||
  DogfoodDocumentPreset.streamed10MiB => _buildSizedDocument(
    preset.targetBytes!,
    _ordinaryBlock,
  ),
  DogfoodDocumentPreset.giantLine5MiB => _buildGiantLine(preset.targetBytes!),
  DogfoodDocumentPreset.denseBlocks1MiB => _buildSizedDocument(
    preset.targetBytes!,
    _denseBlock,
  ),
};

String _buildSizedDocument(int targetBytes, String Function(int index) block) {
  const header = '# Flark large-document dogfood\n\n';
  final output = StringBuffer(header);
  var index = 1;
  while (true) {
    final next = block(index);
    if (output.length + next.length > targetBytes) break;
    output.write(next);
    index += 1;
  }
  _writeRepeated(output, 'dogfood-tail ', targetBytes - output.length);
  assert(output.length == targetBytes);
  return output.toString();
}

String _buildGiantLine(int targetBytes) {
  const header = '# Giant physical line\n\n';
  final output = StringBuffer(header);
  final contentBytes = targetBytes - header.length;
  if (contentBytes > 1) {
    _writeRepeated(output, 'giant-word ', contentBytes - 1);
  }
  if (contentBytes > 0) output.write('\n');
  assert(output.length == targetBytes);
  return output.toString();
}

void _writeRepeated(StringBuffer output, String pattern, int bytes) {
  var remaining = bytes;
  while (remaining >= pattern.length) {
    output.write(pattern);
    remaining -= pattern.length;
  }
  if (remaining > 0) output.write(pattern.substring(0, remaining));
}

String _ordinaryBlock(int index) {
  final id = index.toString().padLeft(6, '0');
  return '''
## Section $id

This is ordinary prose for **Flark** with *emphasis*, `inline code`, and a
[source-backed link](https://example.com/$id). Edit quickly, select across
blocks, undo, redo, and keep scrolling while the parser catches up.

- item $id-A
- item $id-B
- [ ] pending task $id
- [x] completed task $id

> A bounded incremental editor should keep this distant quote responsive.

```dart
final section$id = 'bounded';
```

| Name | Value | State |
| :--- | ---: | :---: |
| section | $id | ready |

''';
}

String _denseBlock(int index) {
  final id = index.toString().padLeft(6, '0');
  return '''
### Block $id

Short bounded paragraph $id.

''';
}

const _productTour = r'''# Flark dogfood

This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

## Start here

1. Click into this paragraph and type quickly.
2. Drag a selection across several blocks, then copy, cut, paste, undo, and redo.
3. Scroll away while an edit is settling, then return.
4. Switch to the large presets from the toolbar.

> Report anything that feels delayed, jumps unexpectedly, loses a selection,
> changes source incorrectly, or looks visually confusing.

## GFM projection

- [ ] An unchecked task
- [x] A checked task
- **strong**, *emphasis*, ~~strikethrough~~, and `inline code`
- <https://example.com> and <dogfood@example.com>

| Surface | Authority | State |
| :--- | :--- | ---: |
| editing surface | certified parser projection | 1 |
| reading surface | shared render plan | 2 |
| pending island | neutral exact source | 3 |

```dart
final editor = FlarkEditor(controller: controller);
```

## Unicode input

Try emoji and joined sequences: 👩‍💻 🧑🏽‍🚀 👨‍👩‍👧‍👦.

Try combining text: café, café, Å, Å.

Try bidirectional text: English العربية עברית English.

## Marker transitions

Turn the following ordinary line into a heading, quote, list, task, and fenced
code block. Watch the rendered construct and local incomplete transition:

change this line

## Incomplete syntax

Incomplete constructs must stay editable and source-exact while you finish or
remove them:

Start editing here: **unfinished

## Long paragraph

This intentionally longer paragraph provides wrapping and caret-navigation
feedback. Resize the window, select across wrapped lines, insert text near both
edges, and look for sudden reflow, clipped glyphs, incorrect hit testing, or a
caret that no longer follows the source position. The surface should remain
calm even while certified structure is temporarily unavailable.
''';
