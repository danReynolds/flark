import 'languages.dart';
import 'mode.dart';

/// Detection reads at most this many code units of a fence.
const detectionSample = 1024;

/// The language of an untagged fence, or '' when nothing is recognized.
///
/// Each ported language tokenizes a sample with its own mode. What a mode
/// recognizes is evidence for it: keywords, builtins and atoms by text, type
/// annotations (a type after `:`; TypeScript also reads `extends X` as a
/// type, which says nothing), comment markers, declarations and
/// multi-character operators. Each
/// piece is weighted by how few languages recognize it, so a word only one
/// language knows decides more than one most share. Declarations count
/// once, since modes differ in what they call one; comment markers count
/// half, since `#` also opens JavaScript's private names; and each token a
/// mode rejects as an error counts half against it. A language whose reading
/// of the sample looks like prose (no code punctuation outside strings and
/// comments, and mostly plain names or a sentence's end) is out, as is a
/// values-only language (JSON)
/// when the sample is not shaped like one; when it is, that shape counts.
/// Ties go to the language listed first in [codeMirrorCatalog].
String detectCodeMirrorLanguage(String source) {
  final sample = detectionSampleOf(source);
  if (sample.trim().isEmpty) return '';
  final evidence = <String, _Evidence>{};
  for (final language in codeMirrorCatalog) {
    final found = _evidence(language, sample);
    if (found != null) evidence[language.name] = found;
  }
  final shared = <String, int>{};
  for (final found in evidence.values) {
    for (final key in found.words.keys) {
      shared[key] = (shared[key] ?? 0) + 1;
    }
  }
  var best = '';
  var bestScore = 0.0;
  for (final MapEntry(key: language, value: found) in evidence.entries) {
    var score = -0.5 * found.errors;
    for (final MapEntry(:key, value: count) in found.words.entries) {
      final weight = key.startsWith('comment ') ? 0.5 : 1.0;
      score += weight * count / shared[key]!;
    }
    if (score > bestScore) {
      best = language;
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

final _name = RegExp(r'^[A-Za-z_$][\w$]*$');
final _codePunctuation = RegExp(r'[{}()\[\];=<>]');
final _sentenceEnd = RegExp(r'[.!?]\s*$');

typedef _Evidence = ({Map<String, int> words, int errors});

/// What [language]'s mode recognizes in [sample], or null when the language
/// rules itself out.
_Evidence? _evidence(CodeMirrorLanguage language, String sample) {
  final values = language.valuesOnly;
  if (values && !_opensValues(sample)) return null;
  final words = <String, int>{};
  var names = 0, plainNames = 0, errors = 0, punctuation = false;
  final mode = language.mode(const ModeConfig());
  final state = mode.startState();
  final lines = splitLines(sample).lines;
  final oracle = LineList(lines);
  void count(String key) => words[key] = (words[key] ?? 0) + 1;
  var previous = '';
  for (var i = 0; i < lines.length; i++) {
    final line = lines[i];
    oracle.line = i;
    final stream = streamFor(mode, line, oracle);
    if (line.isEmpty) mode.blankLine(state);
    while (!stream.eol()) {
      final style = readToken(mode, stream, state);
      final text = line.substring(stream.start, stream.pos);
      final before = previous;
      if (text.trim().isNotEmpty) previous = text;
      if (_name.hasMatch(text)) {
        names++;
        if (style == null || style == 'variable') plainNames++;
      }
      if (style == null || style == 'operator' || style == 'bracket') {
        if (_codePunctuation.hasMatch(text)) punctuation = true;
      }
      if (style == null) continue;
      if (style.contains('error') || style.contains('invalid')) {
        errors++;
      } else if (style.contains('comment')) {
        count('comment ${text.length < 2 ? text : text.substring(0, 2)}');
      } else if (style == 'string property') {
        // Shapes count once; how often a mode sees them says little.
        words[style] = 1;
      } else if (style.contains('type')) {
        if (before == ':') count('$style $text');
      } else if (style.contains('keyword') ||
          style.contains('builtin') ||
          style.contains('atom')) {
        if (values && style.contains('keyword')) return null;
        count('$style $text');
      } else if (style == 'def') {
        if (values) return null;
        words[style] = 1;
      } else if (style == 'operator' && text.length > 1) {
        count('$style $text');
      } else if (values &&
          (style.contains('variable') || style.contains('property'))) {
        // Values hold no names.
        return null;
      }
    }
  }
  // Prose: no code punctuation, and mostly plain names or a sentence's end.
  if (!punctuation &&
      (plainNames * 10 >= names * 7 || _sentenceEnd.hasMatch(sample))) {
    return null;
  }
  // A value-shaped sample is itself evidence for a values-only language.
  if (values) count('values');
  return (words: words, errors: errors);
}

bool _opensValues(String sample) {
  final text = sample.trimLeft();
  return text.startsWith('{') || text.startsWith('[');
}
