import 'package:flark/code.dart';

import 'mode.dart';
import 'modes/blocks.dart';
import 'modes/clike.dart';
import 'modes/css.dart';
import 'modes/go.dart';
import 'modes/htmlmixed.dart';
import 'modes/javascript.dart';
import 'modes/php.dart';
import 'modes/powershell.dart';
import 'modes/python.dart';
import 'modes/ruby.dart';
import 'modes/rust.dart';
import 'modes/shell.dart';
import 'modes/sql.dart';
import 'modes/xml.dart';
import 'modes/yaml.dart';
import 'signs.dart';

/// A ported language: its canonical name, display label and mode.
final class CodeMirrorLanguage {
  const CodeMirrorLanguage(
    this.name,
    this.label,
    this.mode, {
    this.signs = const [],
    this.baseSigns = const [],
    this.dialect = false,
    this.valuesOnly = false,
  });
  final String name, label;
  final Mode<Object?> Function(ModeConfig config) mode;

  /// What an untagged fence in this language looks like; without any, it is
  /// never detected.
  final List<DetectionSign> signs;

  /// The signs of the language this one extends, which count for it too.
  final List<DetectionSign> baseSigns;

  /// A dialect is detected only where [baseSigns] alone reach half the
  /// threshold, since its own signs (a LESS mixin call, MySQL's quoted names)
  /// also appear in other code.
  final bool dialect;

  /// The language holds only values, as JSON does, so detection requires
  /// that shape.
  final bool valuesOnly;
}

/// The ported languages. An app that highlights only some names them,
/// `FlarkCodeMirror.only([CodeMirrorLanguages.python, ...])`, and the others'
/// modes stay out of its build.
abstract final class CodeMirrorLanguages {
  static final javascript = CodeMirrorLanguage(
    'javascript',
    'JavaScript',
    JavaScriptMode.new,
    signs: javascriptSigns,
  );
  static final typescript = CodeMirrorLanguage(
    'typescript',
    'TypeScript',
    _typescript,
    signs: typescriptSigns,
    baseSigns: javascriptSigns,
  );
  static final json = CodeMirrorLanguage(
    'json',
    'JSON',
    _json,
    valuesOnly: true,
  );
  static final python = CodeMirrorLanguage(
    'python',
    'Python',
    PythonMode.new,
    signs: pythonSigns,
  );
  static final c = CodeMirrorLanguage('c', 'C', ClikeMode.c, signs: cSigns);
  static final cpp = CodeMirrorLanguage(
    'cpp',
    'C++',
    ClikeMode.cpp,
    signs: cppSigns,
    baseSigns: cSigns,
  );
  static final java = CodeMirrorLanguage(
    'java',
    'Java',
    ClikeMode.java,
    signs: javaSigns,
  );
  static final csharp = CodeMirrorLanguage(
    'csharp',
    'C#',
    ClikeMode.csharp,
    signs: csharpSigns,
  );
  static final kotlin = CodeMirrorLanguage(
    'kotlin',
    'Kotlin',
    ClikeMode.kotlin,
    signs: kotlinSigns,
  );
  static final dart = CodeMirrorLanguage(
    'dart',
    'Dart',
    ClikeMode.dart,
    signs: dartSigns,
  );
  static final bash = CodeMirrorLanguage(
    'bash',
    'Bash',
    _bash,
    signs: bashSigns,
  );
  static final yaml = CodeMirrorLanguage(
    'yaml',
    'YAML',
    YamlMode.new,
    signs: yamlSigns,
  );
  static final go = CodeMirrorLanguage('go', 'Go', GoMode.new, signs: goSigns);
  static final ruby = CodeMirrorLanguage(
    'ruby',
    'Ruby',
    RubyMode.new,
    signs: rubySigns,
  );
  static final rust = CodeMirrorLanguage(
    'rust',
    'Rust',
    RustMode.new,
    signs: rustSigns,
  );
  static final powershell = CodeMirrorLanguage(
    'powershell',
    'PowerShell',
    PowerShellMode.new,
    signs: powershellSigns,
  );
  static final xml = CodeMirrorLanguage(
    'xml',
    'XML',
    XmlMode.new,
    signs: xmlSigns,
  );
  static final html = CodeMirrorLanguage(
    'html',
    'HTML',
    HtmlMixedMode.new,
    signs: htmlSigns,
  );
  static final sql = CodeMirrorLanguage(
    'sql',
    'SQL',
    SqlMode.standardSql,
    signs: sqlSigns,
  );
  static final postgresql = CodeMirrorLanguage(
    'postgresql',
    'PostgreSQL',
    SqlMode.pgSql,
    signs: postgresqlSigns,
    baseSigns: sqlSigns,
    dialect: true,
  );
  static final mysql = CodeMirrorLanguage(
    'mysql',
    'MySQL',
    SqlMode.mySql,
    signs: mysqlSigns,
    baseSigns: sqlSigns,
    dialect: true,
  );
  static final css = CodeMirrorLanguage(
    'css',
    'CSS',
    CssMode.css,
    signs: cssSigns,
  );
  static final scss = CodeMirrorLanguage(
    'scss',
    'SCSS',
    CssMode.scss,
    signs: scssSigns,
    baseSigns: cssSigns,
    dialect: true,
  );
  static final less = CodeMirrorLanguage(
    'less',
    'LESS',
    CssMode.less,
    signs: lessSigns,
    baseSigns: cssSigns,
    dialect: true,
  );
  static final php = CodeMirrorLanguage(
    'php',
    'PHP',
    PhpMode.new,
    signs: phpSigns,
    baseSigns: htmlSigns,
  );

  /// Every ported language, in detection priority: the earlier of two with
  /// equal evidence wins.
  static final all = [
    javascript,
    typescript,
    json,
    python,
    c,
    cpp,
    java,
    csharp,
    kotlin,
    dart,
    bash,
    yaml,
    go,
    ruby,
    rust,
    powershell,
    xml,
    html,
    sql,
    postgresql,
    mysql,
    css,
    scss,
    less,
    php,
  ];
}

/// CodeMirror's shell mode has no indentation; Flark indents its blocks.
Mode<Object?> _bash(ModeConfig config) => BlockIndentMode(
  ShellMode(config),
  words: const {
    'if': 'fi',
    'for': 'done',
    'while': 'done',
    'until': 'done',
    'select': 'done',
    'case': 'esac',
  },
  brackets: const {'{': '}', '(': ')'},
  continuations: const {'then', 'do', 'else', 'elif'},
  electricInput: _bashElectric,
);

final _bashElectric = RegExp(r'^\s*(?:fi|done|esac|then|do|else|elif|\}|\))$');

Mode<Object?> _typescript(ModeConfig config) =>
    JavaScriptMode(config, const JavaScriptOptions(typescript: true));

Mode<Object?> _json(ModeConfig config) =>
    JavaScriptMode(config, const JavaScriptOptions(json: true));

/// The canonical name for a fence's info string: its first word
/// ([codeInfoLanguage]), lowercased, through [codeMirrorAliases]. Empty for
/// none or [codeAutoLanguage]. Names without an alias are returned as written.
String codeMirrorLanguageName(String info) {
  final name = codeInfoLanguage(info.trim()).toLowerCase();
  return codeMirrorAliases[name] ?? name;
}

/// Other names for languages, as fences write them: common aliases and file
/// extensions, and close relatives a language's mode reads well (JSX as
/// JavaScript). `text` and its aliases name plain text.
const codeMirrorAliases = {
  codeAutoLanguage: '',
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

/// A mode for [language] among every ported one, or null when none is.
Mode<Object?>? codeMirrorMode(
  String language, [
  ModeConfig config = const ModeConfig(),
]) {
  for (final entry in CodeMirrorLanguages.all) {
    if (entry.name == language) return entry.mode(config);
  }
  return null;
}
