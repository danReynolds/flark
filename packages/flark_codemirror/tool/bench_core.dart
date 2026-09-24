import 'package:flark/code.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_codemirror/src/highlight.dart';
import 'package:flark_codemirror/src/languages.dart';

/// [source] repeated, then cut, to exactly [units] code units at a line end
/// when one is near.
String sized(String source, int units) {
  final buffer = StringBuffer();
  while (buffer.length < units) {
    buffer.write(source);
    if (!source.endsWith('\n')) buffer.write('\n');
  }
  final text = buffer.toString().substring(0, units);
  final end = text.lastIndexOf('\n');
  return end > units - 200 ? text.substring(0, end) : text;
}

/// Median microseconds of [run] over [rounds] rounds after a warm-up.
int median(void Function() run, {int rounds = 41, int warm = 20}) {
  for (var i = 0; i < warm; i++) {
    run();
  }
  final samples = <int>[];
  final watch = Stopwatch();
  for (var i = 0; i < rounds; i++) {
    watch
      ..reset()
      ..start();
    run();
    watch.stop();
    samples.add(watch.elapsedMicroseconds);
  }
  samples.sort();
  return samples[rounds ~/ 2];
}

/// Highlighting and edit proposals over [snippets] (language to source) at
/// each size in [sizes]. The delegate's cache is bypassed: every highlight
/// tokenizes.
List<Map<String, Object>> benchCodeMirror(
  Map<String, String> snippets, {
  List<int> sizes = const [2048, 8192, 32768],
}) {
  final results = <Map<String, Object>>[];
  final delegate = FlarkCodeMirror();
  for (final MapEntry(key: language, value: source) in snippets.entries) {
    final mode = codeMirrorMode(language)!;
    for (final units in sizes) {
      final text = sized(source, units);
      var tokens = 0;
      final highlightUs = median(
        () => tokens = codeMirrorTokens(mode, text).length,
      );
      // Enter and a typed closer at the end: the proposal tokenizes
      // everything before the caret.
      final enterUs = median(
        () => delegate.propose(
          text,
          language: language,
          base: text.length,
          extent: text.length,
          action: CodeEditingAction.newline,
          text: '',
          indentUnit: '  ',
        ),
      );
      final typedText = '$text\n  ';
      final typeUs = median(
        () => delegate.propose(
          typedText,
          language: language,
          base: typedText.length,
          extent: typedText.length,
          action: CodeEditingAction.insert,
          text: '}',
          indentUnit: '  ',
        ),
      );
      results.add({
        'language': language,
        'units': text.length,
        'lines': '\n'.allMatches(text).length + 1,
        'tokens': tokens,
        'highlightUs': highlightUs,
        'enterUs': enterUs,
        'typeCloserUs': typeUs,
      });
    }
  }
  return results;
}
