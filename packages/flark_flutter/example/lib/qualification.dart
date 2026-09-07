import 'dart:convert';
import 'package:flark/flark.dart';

// Candidate values stay explicit until a matching profile is sealed.
const candidateLiveBytes = 32 * 1024;
const candidateSourceBytes = 256 * 1024;
const candidateLiveLimits = FlarkLiveLimits(
  lines: 1024,
  lineCodeUnits: 4096,
  blocks: 512,
  runs: 2048,
  blockCodeUnits: 4096,
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
};

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
