// Ported from @codemirror/legacy-modes 6.5.4 mode/rust.js, its `rust`
// export: a simple mode.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import 'simple_mode.dart';

final _rust = SimpleModeSpec(
  {
    'start': [
      // string and byte string
      SimpleRule(regex: r'b?"', token: 'string', next: 'string'),
      // raw string and raw byte string
      SimpleRule(regex: r'b?r"', token: 'string', next: 'string_raw'),
      SimpleRule(regex: r'b?r#+"', token: 'string', next: 'string_raw_hash'),
      // character
      SimpleRule(
        regex:
            r"""'(?:[^'\\]|\\(?:[nrt0'"]|x[\da-fA-F]{2}|u\{[\da-fA-F]{6}\}))'""",
        token: 'string.special',
      ),
      // byte
      SimpleRule(
        regex: r"b'(?:[^']|\\(?:['\\nrt0]|x[\da-fA-F]{2}))'",
        token: 'string.special',
      ),

      SimpleRule(
        regex:
            r'(?:(?:[0-9][0-9_]*)(?:(?:[Ee][+-]?[0-9_]+)|\.[0-9_]+'
            r'(?:[Ee][+-]?[0-9_]+)?)(?:f32|f64)?)|(?:0(?:b[01_]+|'
            r'(?:o[0-7_]+)|(?:x[0-9a-fA-F_]+))|(?:[0-9][0-9_]*))'
            r'(?:u8|u16|u32|u64|i8|i16|i32|i64|isize|usize)?',
        token: 'number',
      ),
      SimpleRule(
        regex:
            r'(let(?:\s+mut)?|fn|enum|mod|struct|type|union)(\s+)'
            r'([a-zA-Z_][a-zA-Z0-9_]*)',
        token: ['keyword', null, 'def'],
      ),
      SimpleRule(
        regex:
            r'(?:abstract|alignof|as|async|await|box|break|continue|const|'
            r'crate|do|dyn|else|enum|extern|fn|for|final|if|impl|in|loop|'
            r'macro|match|mod|move|offsetof|override|priv|proc|pub|pure|ref|'
            r'return|self|sizeof|static|struct|super|trait|type|typeof|union|'
            r'unsafe|unsized|use|virtual|where|while|yield)\b',
        token: 'keyword',
      ),
      SimpleRule(
        regex:
            r'\b(?:Self|isize|usize|char|bool|u8|u16|u32|u64|f16|f32|f64|i8|'
            r'i16|i32|i64|str|Option)\b',
        token: 'atom',
      ),
      SimpleRule(
        regex: r'\b(?:true|false|Some|None|Ok|Err)\b',
        token: 'builtin',
      ),
      SimpleRule(
        regex: r'\b(fn)(\s+)([a-zA-Z_][a-zA-Z0-9_]*)',
        token: ['keyword', null, 'def'],
      ),
      SimpleRule(regex: r'#!?\[.*\]', token: 'meta'),
      SimpleRule(regex: r'\/\/.*', token: 'comment'),
      SimpleRule(regex: r'\/\*', token: 'comment', next: 'comment'),
      SimpleRule(regex: r'[-+\/*=<>!]+', token: 'operator'),
      SimpleRule(regex: r'[a-zA-Z_]\w*!', token: 'macroName'),
      SimpleRule(regex: r'[a-zA-Z_]\w*', token: 'variable'),
      SimpleRule(regex: r'[\{\[\(]', indent: true),
      SimpleRule(regex: r'[\}\]\)]', dedent: true),
    ],
    'string': [
      SimpleRule(regex: '"', token: 'string', next: 'start'),
      SimpleRule(regex: r'(?:[^\\"]|\\(?:.|$))*', token: 'string'),
    ],
    'string_raw': [
      SimpleRule(regex: '"', token: 'string', next: 'start'),
      SimpleRule(regex: '[^"]*', token: 'string'),
    ],
    'string_raw_hash': [
      SimpleRule(regex: '"#+', token: 'string', next: 'start'),
      SimpleRule(regex: '(?:[^"]|"(?!#))*', token: 'string'),
    ],
    'comment': [
      SimpleRule(regex: r'.*?\*\/', token: 'comment', next: 'start'),
      SimpleRule(regex: '.*', token: 'comment'),
    ],
  },
  dontIndentStates: ['comment'],
  indentOnInput: RegExp(r'^\s*\}$'),
);

/// Rust: the upstream `rust` simple mode.
final class RustMode extends SimpleMode {
  RustMode([ModeConfig config = const ModeConfig()]) : super(_rust, config);
}
