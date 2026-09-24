import 'mode.dart';
import 'modes/javascript.dart';

/// A ported language: its canonical name, display label and mode.
final class CodeMirrorLanguage {
  const CodeMirrorLanguage(
    this.name,
    this.label,
    this.mode, {
    this.valuesOnly = false,
  });
  final String name, label;
  final Mode<Object?> Function(ModeConfig config) mode;

  /// The language holds only values, as JSON does, so detection requires
  /// that shape.
  final bool valuesOnly;
}

/// The ported languages, in detection priority.
final codeMirrorCatalog = [
  CodeMirrorLanguage('javascript', 'JavaScript', JavaScriptMode.new),
  CodeMirrorLanguage(
    'typescript',
    'TypeScript',
    (config) =>
        JavaScriptMode(config, const JavaScriptOptions(typescript: true)),
  ),
  CodeMirrorLanguage(
    'json',
    'JSON',
    (config) => JavaScriptMode(config, const JavaScriptOptions(json: true)),
    valuesOnly: true,
  ),
];

/// Languages with a ported mode, by canonical name, with display labels.
final codeMirrorLanguages = {
  for (final language in codeMirrorCatalog) language.name: language.label,
};

/// The canonical name for a fence's info string: its first word, lowercased,
/// with the aliases Flark's language picker accepts. Empty for none or
/// `auto`. Names without a ported mode are returned as written.
String codeMirrorLanguageName(String info) {
  final name = info.trim().split(RegExp(r'\s+')).first.toLowerCase();
  return const {
        'auto': '',
        'js': 'javascript',
        'ts': 'typescript',
        'py': 'python',
        'rb': 'ruby',
        'rs': 'rust',
        'golang': 'go',
        'sh': 'bash',
        'shell': 'bash',
        'yml': 'yaml',
        'txt': 'text',
        'plaintext': 'text',
        'plain': 'text',
      }[name] ??
      name;
}

/// A mode for [language], or null when none is ported.
Mode<Object?>? codeMirrorMode(
  String language, [
  ModeConfig config = const ModeConfig(),
]) {
  for (final entry in codeMirrorCatalog) {
    if (entry.name == language) return entry.mode(config);
  }
  return null;
}
