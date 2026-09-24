import 'mode.dart';
import 'modes/javascript.dart';

/// Languages with a ported mode, by canonical name, with display labels.
const codeMirrorLanguages = {
  'javascript': 'JavaScript',
  'typescript': 'TypeScript',
  'json': 'JSON',
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
]) => switch (language) {
  'javascript' => JavaScriptMode(config),
  'typescript' => JavaScriptMode(
    config,
    const JavaScriptOptions(typescript: true),
  ),
  'json' => JavaScriptMode(config, const JavaScriptOptions(json: true)),
  _ => null,
};
