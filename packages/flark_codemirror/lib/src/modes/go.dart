// Ported from @codemirror/legacy-modes 6.5.4 mode/go.js, its `go` export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

Set<String> _words(String words) => words.split(' ').toSet();

final _keywords = _words(
  'break case chan const continue default defer else fallthrough for func '
  'go goto if import interface map package range return select struct '
  'switch type var bool byte complex64 complex128 float32 float64 int8 '
  'int16 int32 int64 string uint8 uint16 uint32 uint64 int uint uintptr '
  'error rune any comparable',
);

const _atoms = {
  'true', 'false', 'iota', 'nil', 'append', 'cap', 'close', 'complex', //
  'copy', 'delete', 'imag', 'len', 'make', 'new', 'panic', 'print',
  'println', 'real', 'recover',
};

final _isOperatorChar = RegExp(r'[+\-*&^%:=<>!|\/]');
final _numberStart = RegExp(r'[\d\.]');
final _fraction = RegExp(r'[0-9]+([eE][\-+]?[0-9]+)?');
final _hex = RegExp('[xX][0-9a-fA-F]+');
final _octal = RegExp('0[0-7]+');
final _decimal = RegExp(r'[0-9]*\.?[0-9]*([eE][\-+]?[0-9]+)?');
final _punctuation = RegExp(r'[\[\]{}\(\),;\:\.]');
final _wordChar = RegExp(r'[\w\$_\xa1-\uffff]');
final _caseLabel = RegExp(r'^(?:case|default)\b');

typedef _Tokenizer = String? Function(StringStream stream, GoState state);

final class _Context {
  _Context(this.indented, this.column, this.type, this.align, [this.prev]);
  final int indented, column;
  String type;
  bool? align;
  final _Context? prev;
}

final class GoState {
  GoState._(this._tokenize, this._context, this._indented, this._startOfLine);
  _Tokenizer? _tokenize;

  /// Changed in place, as upstream does, so copies share it.
  _Context _context;
  int _indented;
  bool _startOfLine;

  /// The upstream default copy: the context is shared.
  GoState copy() => GoState._(_tokenize, _context, _indented, _startOfLine);
}

void _pushContext(GoState state, int col, String type) {
  state._context = _Context(state._indented, col, type, null, state._context);
}

void _popContext(GoState state) {
  final prev = state._context.prev;
  if (prev == null) return;
  final t = state._context.type;
  if (t == ')' || t == ']' || t == '}') {
    state._indented = state._context.indented;
  }
  state._context = prev;
}

/// Go: the upstream `go` stream parser.
final class GoMode extends Mode<GoState> {
  GoMode([super.config = const ModeConfig()]);

  /// Upstream's module-level `curPunc`: the punctuation just read.
  String? _curPunc;

  late final _Tokenizer _tokenBaseT = _tokenBase;
  late final _Tokenizer _tokenCommentT = _tokenComment;

  String? _tokenBase(StringStream stream, GoState state) {
    final ch = stream.next()!;
    if (ch == '"' || ch == "'" || ch == '`') {
      final tokenize = state._tokenize = _tokenString(ch);
      return tokenize(stream, state);
    }
    if (_numberStart.hasMatch(ch)) {
      if (ch == '.') {
        stream.match(_fraction);
      } else if (ch == '0') {
        if (stream.match(_hex) == null) stream.match(_octal);
      } else {
        stream.match(_decimal);
      }
      return 'number';
    }
    if (_punctuation.hasMatch(ch)) {
      _curPunc = ch;
      return null;
    }
    if (ch == '/') {
      if (stream.eat('*') != null) {
        state._tokenize = _tokenCommentT;
        return _tokenComment(stream, state);
      }
      if (stream.eat('/') != null) {
        stream.skipToEnd();
        return 'comment';
      }
    }
    if (_isOperatorChar.hasMatch(ch)) {
      stream.eatWhile(_isOperatorChar);
      return 'operator';
    }
    stream.eatWhile(_wordChar);
    final cur = stream.current();
    if (_keywords.contains(cur)) {
      if (cur == 'case' || cur == 'default') _curPunc = 'case';
      return 'keyword';
    }
    if (_atoms.contains(cur)) return 'atom';
    return 'variable';
  }

  _Tokenizer _tokenString(String quote) => (stream, state) {
    var escaped = false, end = false;
    for (var next = stream.next(); next != null; next = stream.next()) {
      if (next == quote && !escaped) {
        end = true;
        break;
      }
      escaped = !escaped && quote != '`' && next == r'\';
    }
    if (end || !(escaped || quote == '`')) state._tokenize = _tokenBaseT;
    return 'string';
  };

  String? _tokenComment(StringStream stream, GoState state) {
    var maybeEnd = false;
    for (var ch = stream.next(); ch != null; ch = stream.next()) {
      if (ch == '/' && maybeEnd) {
        state._tokenize = _tokenBaseT;
        break;
      }
      maybeEnd = ch == '*';
    }
    return 'comment';
  }

  @override
  GoState startState([int baseColumn = 0]) =>
      GoState._(null, _Context(-config.indentUnit, 0, 'top', false), 0, true);

  @override
  GoState copyState(GoState state) => state.copy();

  @override
  String? token(StringStream stream, GoState state) {
    final ctx = state._context;
    if (stream.sol()) {
      ctx.align ??= false;
      state._indented = stream.indentation();
      state._startOfLine = true;
      if (ctx.type == 'case') ctx.type = '}';
    }
    if (stream.eatSpace()) return null;
    _curPunc = null;
    final style = (state._tokenize ?? _tokenBaseT)(stream, state);
    if (style == 'comment') return style;
    ctx.align ??= true;

    if (_curPunc == '{') {
      _pushContext(state, stream.column(), '}');
    } else if (_curPunc == '[') {
      _pushContext(state, stream.column(), ']');
    } else if (_curPunc == '(') {
      _pushContext(state, stream.column(), ')');
    } else if (_curPunc == 'case') {
      ctx.type = 'case';
    } else if (_curPunc == '}' && ctx.type == '}') {
      _popContext(state);
    } else if (_curPunc == ctx.type) {
      _popContext(state);
    }
    state._startOfLine = false;
    return style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(GoState state, String textAfter, String line) {
    if (!identical(state._tokenize, _tokenBaseT) && state._tokenize != null) {
      return null;
    }
    final ctx = state._context;
    final firstChar = textAfter.isEmpty ? '' : textAfter[0];
    if (ctx.type == 'case' && _caseLabel.hasMatch(textAfter)) {
      return ctx.indented;
    }
    final closing = firstChar == ctx.type;
    if (ctx.align == true) return ctx.column + (closing ? 0 : 1);
    return ctx.indented + (closing ? 0 : config.indentUnit);
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s([{}]|case |default\s*:)$');
}
