import 'model.dart';

/// One catalog for manual choice, aliases, detection and grammar IDs.
const codeLanguages = {
  'dart': 'Dart',
  'javascript': 'JavaScript',
  'python': 'Python',
  'yaml': 'YAML',
  'ruby': 'Ruby',
  'typescript': 'TypeScript',
  'rust': 'Rust',
  'go': 'Go',
  'json': 'JSON',
  'css': 'CSS',
  'bash': 'Shell',
  'html': 'HTML',
  'xml': 'XML',
  'sql': 'SQL',
  'text': 'Plain text',
};
String codeLanguageName(String info) {
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

CodeLanguage codeLanguage(String name) => name == 'text'
    ? CodeLanguage.plain
    : CodeLanguage.values.firstWhere(
        (l) => l.name == name,
        orElse: () => CodeLanguage.plain,
      );
