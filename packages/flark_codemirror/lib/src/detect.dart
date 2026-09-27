import 'languages.dart';
import 'signs.dart';

/// Detection reads at most this many code units of a fence.
const detectionSample = 512;

/// Less evidence than this leaves a fence of three or more lines plain; a
/// shorter one needs half a point less a line (1.5 for one), since a line
/// can show only so much.
const detectionThreshold = 2.5;

/// The language of an untagged fence among [languages], such as
/// [CodeMirrorLanguages.all], or '' when nothing is recognized.
///
/// Each language's signs ([CodeMirrorLanguage.signs]) are patterns that its
/// code shows and other text rarely does, such as `def f():` for Python or
/// `err != nil` for Go, each weighted by how sure a sign it is, and some
/// counting against it. The language with the most evidence wins if it has
/// at least [detectionThreshold] (less for one or two lines); ties go to the
/// one listed first. A variant
/// (TypeScript, C++, a SQL dialect) carries its base language's signs and
/// its own, so it wins only on the latter. A values-only language (JSON)
/// has no signs: it is the answer when the sample holds only values. A
/// sample that reads as prose stays plain whatever its signs.
String detectCodeMirrorLanguage(
  String source,
  Iterable<CodeMirrorLanguage> languages,
) {
  final sample = detectionSampleOf(source);
  if (sample.trim().isEmpty || _looksLikeProse(sample)) return '';
  var lines = 0;
  for (final line in sample.split('\n')) {
    if (line.trim().isNotEmpty && ++lines == 3) break;
  }
  final threshold = detectionThreshold - 0.5 * (3 - lines);
  var best = '';
  var bestScore = threshold - 1e-9;
  // Variants share their base's signs: score each list once.
  final scores = Map<List<DetectionSign>, double>.identity();
  final read = DetectionSample(sample);
  double scoreOf(List<DetectionSign> signs) =>
      scores[signs] ??= signs.fold(0.0, (sum, sign) => sum + sign.score(read));
  for (final language in languages) {
    final double score;
    if (language.valuesOnly) {
      score = _holdsOnlyValues(sample) ? 8 : 0;
    } else {
      final base = scoreOf(language.baseSigns);
      if (language.dialect && base < threshold / 2) continue;
      score = base + scoreOf(language.signs);
    }
    if (score > bestScore) {
      best = language.name;
      bestScore = score;
    }
  }
  return best;
}

/// The part of [source] detection reads: its first [detectionSample] code
/// units, back to a line end, since a cut line would read as other tokens.
String detectionSampleOf(String source) {
  if (source.length <= detectionSample) return source;
  final sample = source.substring(0, detectionSample);
  final end = sample.lastIndexOf('\n');
  return end > 0 ? sample.substring(0, end) : sample;
}

final _valueToken = RegExp(
  r'\s+|"(?:[^"\\\n]|\\.)*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|'
  r'true\b|false\b|null\b|[{}\[\],:]|//[^\n]*|/\*[\s\S]*?\*/',
);

/// Whether [sample] holds only JSON values, perhaps excerpted and with
/// comments, and at least one key or two values.
bool _holdsOnlyValues(String sample) {
  final text = sample.trim();
  // An excerpt may start inside an object or array.
  if (text.isEmpty || !'{["}]'.contains(text[0])) return false;
  var pos = 0, strings = 0, values = 0, keys = 0;
  String? previous;
  while (pos < text.length) {
    final match = _valueToken.matchAsPrefix(text, pos);
    if (match == null) {
      // A string cut by the sample's end.
      return text.indexOf('\n', pos) < 0 && text[pos] == '"' && values > 0;
    }
    final token = match[0]!;
    pos = match.end;
    if (token.trim().isEmpty || token.startsWith('/')) continue;
    if (token == ':') {
      if (previous == null || !previous.startsWith('"')) return false;
      keys++;
    } else if (!'{}[],'.contains(token)) {
      values++;
      if (token.startsWith('"')) strings++;
    }
    previous = token;
  }
  return keys > 0 || values - strings >= 2 || values >= 2;
}

final _commentLine = RegExp(r'^\s*(?://|#|\*|/\*|--|<!--|;|%)');

// Four plain lowercase words in a row, and a word prose needs and code
// rarely names.
final _wordRun = RegExp(r'(?:\b[a-z]+[,;:]? ){3}[a-z]+\b');
final _functionWord = RegExp(
  r'\b(?:the|an|to|of|that|are|be|was|were|you|we|our|your|which|should|'
  r'would|will|than|its|there|their|these|those|been|has|have)\b',
);

/// Whether at least half the lines outside comments read as sentences.
bool _looksLikeProse(String sample) {
  var lines = 0, prose = 0;
  for (final line in sample.split('\n')) {
    if (line.trim().isEmpty || _commentLine.hasMatch(line)) continue;
    lines++;
    if (_wordRun.hasMatch(line) && _functionWord.hasMatch(line)) prose++;
  }
  return lines > 0 && prose * 2 >= lines;
}
