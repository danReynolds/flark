import 'mode.dart';
import 'modes/clike.dart';
import 'modes/go.dart';
import 'modes/javascript.dart';
import 'modes/powershell.dart';
import 'modes/python.dart';
import 'modes/ruby.dart';
import 'modes/rust.dart';
import 'modes/shell.dart';
import 'modes/xml.dart';
import 'modes/yaml.dart';

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
  CodeMirrorLanguage('python', 'Python', PythonMode.new),
  CodeMirrorLanguage('c', 'C', ClikeMode.c),
  CodeMirrorLanguage('cpp', 'C++', ClikeMode.cpp),
  CodeMirrorLanguage('java', 'Java', ClikeMode.java),
  CodeMirrorLanguage('csharp', 'C#', ClikeMode.csharp),
  CodeMirrorLanguage('kotlin', 'Kotlin', ClikeMode.kotlin),
  CodeMirrorLanguage('dart', 'Dart', ClikeMode.dart),
  CodeMirrorLanguage('bash', 'Bash', ShellMode.new),
  CodeMirrorLanguage('yaml', 'YAML', YamlMode.new),
  CodeMirrorLanguage('go', 'Go', GoMode.new),
  CodeMirrorLanguage('ruby', 'Ruby', RubyMode.new),
  CodeMirrorLanguage('rust', 'Rust', RustMode.new),
  CodeMirrorLanguage('powershell', 'PowerShell', PowerShellMode.new),
  CodeMirrorLanguage('xml', 'XML', XmlMode.new),
  CodeMirrorLanguage(
    'html',
    'HTML',
    (config) => XmlMode(config, XmlOptions.html),
  ),
];

/// Languages with a ported mode, by canonical name, with display labels.
final codeMirrorLanguages = {
  for (final language in codeMirrorCatalog) language.name: language.label,
};

/// The canonical name for a fence's info string: its first word, lowercased,
/// through [codeMirrorAliases]. Empty for none or `auto`. Names without an
/// alias are returned as written.
String codeMirrorLanguageName(String info) {
  final name = info.trim().split(RegExp(r'\s+')).first.toLowerCase();
  return codeMirrorAliases[name] ?? name;
}

/// Other names for languages, as fences write them: common aliases and file
/// extensions, and close relatives a language's mode reads well (JSX as
/// JavaScript). `text` and its aliases name plain text.
const codeMirrorAliases = {
  'auto': '',
  'js': 'javascript',
  'mjs': 'javascript',
  'cjs': 'javascript',
  'jsx': 'javascript',
  'node': 'javascript',
  'ecmascript': 'javascript',
  'ts': 'typescript',
  'mts': 'typescript',
  'cts': 'typescript',
  'tsx': 'typescript',
  'jsonc': 'json',
  'json5': 'json',
  'jsonl': 'json',
  'ndjson': 'json',
  'webmanifest': 'json',
  'py': 'python',
  'py3': 'python',
  'python3': 'python',
  'pyi': 'python',
  'pyw': 'python',
  'starlark': 'python',
  'bzl': 'python',
  'h': 'c',
  'c++': 'cpp',
  'cc': 'cpp',
  'cxx': 'cpp',
  'hpp': 'cpp',
  'hh': 'cpp',
  'hxx': 'cpp',
  'ino': 'cpp',
  'cs': 'csharp',
  'c#': 'csharp',
  'kt': 'kotlin',
  'kts': 'kotlin',
  'sh': 'bash',
  'shell': 'bash',
  'zsh': 'bash',
  'ksh': 'bash',
  'shellscript': 'bash',
  'yml': 'yaml',
  'golang': 'go',
  'rb': 'ruby',
  'rake': 'ruby',
  'gemspec': 'ruby',
  'podspec': 'ruby',
  'rs': 'rust',
  'ps1': 'powershell',
  'psm1': 'powershell',
  'psd1': 'powershell',
  'pwsh': 'powershell',
  'posh': 'powershell',
  'xsd': 'xml',
  'xsl': 'xml',
  'xslt': 'xml',
  'svg': 'xml',
  'plist': 'xml',
  'rss': 'xml',
  'atom': 'xml',
  'wsdl': 'xml',
  'htm': 'html',
  'xhtml': 'html',
  'postgres': 'postgresql',
  'pgsql': 'postgresql',
  'psql': 'postgresql',
  'mariadb': 'mysql',
  'txt': 'text',
  'plaintext': 'text',
  'plain': 'text',
  'output': 'text',
  'log': 'text',
};

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
