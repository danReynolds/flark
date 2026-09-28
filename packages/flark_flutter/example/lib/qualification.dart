import 'dart:convert';
import 'package:flark/flark.dart';

// Candidate values stay explicit until a matching profile is sealed. The
// count caps are the kernel's defaults (2026-09-23); the live limits section
// of docs/architecture/v5/performance_review_2026_09_22.md records why.
const candidateLiveBytes = 32 * 1024;
const candidateSourceBytes = 256 * 1024;
const candidateLiveLimits = FlarkLiveLimits(
  lines: 2048,
  lineCodeUnits: 4096,
  blocks: 2048,
  runs: 8192,
  blockCodeUnits: 16384,
  containerDepth: 8,
);

const profileCycles = <String, String>{
  'prose':
      'A paragraph with **strong words** and enough ordinary prose to wrap across a realistic viewport. Keep writing and moving through the text.\n\n',
  'dense':
      '## Section\n\nSome **strong words** with *emphasis*, `code`, and a [link](https://example.com).\n\n- first item\n- [x] another item\n\n> a short quote\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n',
  'list': '- an item with *emphasis* and a short note\n',
  'table': '| a | b |\n| - | - |\n| cell | **value** |\n\n',
  'nested': '> > > > > > > > Nested **words** with room to wrap.\n\n',
  'reference':
      '[note__index__]: https://example.com/note/__index__\n\nA [reference][note__index__] and another [reference][note__index__].\n\n',
  'unicode': 'Café 👩‍👩‍👧‍👦 你好 **words** and a little more text.\n\n',
  'code':
      '```dart\nfinal answer = 42;\nprint(answer);\n```\n\n'
      '```ruby\ndef hello\n  puts "hello"\nend\n```\n\n'
      '```json\n{"ready": true, "count": 3}\n```\n\n',
};

/// Text the frame profile pastes at the document start.
const profilePaste = 'Pasted line one\n\nPasted paragraph two';

/// Bytes the start-site fixture leaves below the live limit, so Enter and the
/// paste stay live. The other sites still type at the byte boundary itself.
const profileStartHeadroom = 64;

/// Edits the frame profile measures at [site], in order. Every site types and
/// deletes a character. The start site also inserts rows before most of the
/// document, with Enter and a multi-line paste, and removes each with Undo.
/// A round restores the original source. The harness delivers `insert` as a
/// platform text delta rather than as a command.
List<(String, FlarkCommand)> profileOperations(String site) => [
  ('insert', const InsertText('x')),
  ('delete', const DeleteBackward()),
  if (site == 'start') ...[
    ('enter', const Newline(paragraph: true)),
    ('undo enter', const Undo()),
    ('paste', const Paste(profilePaste)),
    ('undo paste', const Undo()),
  ],
];

/// Where the frame profile places the caret for [site].
int profileCaret(Projection projection, String site, String source) {
  if (site == 'end') return source.length;
  final rows = projection.rows;
  final row = site == 'start'
      ? rows.firstWhere((r) => r.text.isNotEmpty)
      : rows.reduce((a, b) => a.text.length > b.text.length ? a : b);
  return row.sourceForDisplay(row.text.length ~/ 2);
}

/// Reach the byte boundary while retaining the densest repetition admitted by
/// the declared count budget. Remaining bytes are paragraphs of up to 4 KiB.
/// The original unconstrained stress workloads remain separate diagnostics.
String boundedProfileSource(
  FlarkParseBackend backend,
  String shape,
  int bytes,
) {
  final cycle = profileCycles[shape]!;
  var text = '';
  var index = 0;
  while (true) {
    final next = text + cycle.replaceAll('__index__', '${index++}');
    if (utf8.encode(next).length >= bytes - 4) break;
    final model = backend.parse('$next\n\nz');
    final reserve = ((bytes - utf8.encode(next).length) / 4000).ceil() + 4;
    if (model.blockCount + reserve > candidateLiveLimits.blocks ||
        model.runCount + reserve > candidateLiveLimits.runs ||
        model.lineCount + reserve * 2 > candidateLiveLimits.lines) {
      break;
    }
    text = next;
  }
  if (!text.endsWith('\n\n')) text += '\n';
  var remaining = bytes - 4 - utf8.encode(text).length;
  while (remaining > 0) {
    if (remaining > 4002) {
      text += '${'word ' * 800}\n\n';
      remaining -= 4002;
    } else {
      text += ('word ' * ((remaining + 4) ~/ 5)).substring(0, remaining);
      remaining = 0;
    }
  }
  return '$text\n\nz';
}
