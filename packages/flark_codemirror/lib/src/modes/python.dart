// Ported from @codemirror/legacy-modes 6.5.4 mode/python.js, its `python`
// export: Python 3 with the default parser configuration. `cython` and the
// configuration options are not ported.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

/// Upstream's `wordRegexp`, `((w1)|(w2)|...)\\b` matched at the stream,
/// succeeds exactly where the ASCII word there is one of the words, which a
/// set tells without trying each.
Set<String> _wordSet(List<String> words) => words.toSet();

/// The run of ASCII word characters at [stream]'s position.
String _asciiWordAt(StringStream stream) {
  final s = stream.string;
  var end = stream.pos;
  while (end < s.length && _isAsciiWordUnit(s.codeUnitAt(end))) {
    end++;
  }
  return s.substring(stream.pos, end);
}

bool _isAsciiWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

const _commonKeywords = [
  'as', 'assert', 'break', 'class', 'continue', //
  'def', 'del', 'elif', 'else', 'except', 'finally',
  'for', 'from', 'global', 'if', 'import',
  'lambda', 'pass', 'raise', 'return',
  'try', 'while', 'with', 'yield', 'in', 'False', 'True',
];
const _commonBuiltins = [
  'abs', 'all', 'any', 'bin', 'bool', 'bytearray', 'callable', 'chr', //
  'classmethod', 'compile', 'complex', 'delattr', 'dict', 'dir', 'divmod',
  'enumerate', 'eval', 'filter', 'float', 'format', 'frozenset',
  'getattr', 'globals', 'hasattr', 'hash', 'help', 'hex', 'id',
  'input', 'int', 'isinstance', 'issubclass', 'iter', 'len',
  'list', 'locals', 'map', 'max', 'memoryview', 'min', 'next',
  'object', 'oct', 'open', 'ord', 'pow', 'property', 'range',
  'repr', 'reversed', 'round', 'set', 'setattr', 'slice',
  'sorted', 'staticmethod', 'str', 'sum', 'super', 'tuple',
  'type', 'vars', 'zip', '__import__', 'NotImplemented',
  'Ellipsis', '__debug__',
];

const _error = 'error';

final _wordOperators = _wordSet(['and', 'or', 'not', 'is']);
final _keywords = _wordSet([
  ..._commonKeywords,
  'nonlocal', 'None', 'aiter', 'anext', 'async', 'await', 'breakpoint', //
  'match', 'case',
]);
final _builtins = _wordSet([
  ..._commonBuiltins,
  'ascii', 'bytes', 'exec', 'print', //
]);
final _delimiters = RegExp(r'[\(\)\[\]\{\}@,:`=;\.\\]');
final _operators = RegExp(
  r'([-+*/%\/&|^]=?|[<>=]+|\/\/=?|\*\*=?|!=|[~!@]|\.\.\.)',
);
final _identifiers = RegExp(
  r'[_A-Za-z\u00A1-\uFFFF][_A-Za-z0-9\u00A1-\uFFFF]*',
);
final _stringPrefixes = RegExp(
  r'''(([rbuf]|(br)|(rb)|(fr)|(rf))?('{3}|"{3}|['"]))''',
  caseSensitive: false,
);
final _comment = RegExp('#.*');
final _numberStart = RegExp(r'[0-9\.]');
final _float = RegExp(r'[\d_]*\.\d+(e[\+\-]?\d+)?', caseSensitive: false);
final _pointFloat = RegExp(r'[\d_]+\.\d*');
final _fraction = RegExp(r'\.\d+');
final _imaginary = RegExp('J', caseSensitive: false);
final _hex = RegExp('0x[0-9a-f_]+', caseSensitive: false);
final _binary = RegExp('0b[01_]+', caseSensitive: false);
final _octal = RegExp('0o[0-7_]+', caseSensitive: false);
final _decimal = RegExp(r'[1-9][\d_]*(e[\+\-]?[\d_]+)?');
final _zero = RegExp(r'0(?![\dx])', caseSensitive: false);
final _long = RegExp('L', caseSensitive: false);
final _formatBody = RegExp(r'''[^'"\{\}\\]''');
final _stringBody = RegExp(r'''[^'"\\]''');
final _quote = RegExp(r'''['"]''');
final _bracketTail = RegExp(r'[\s\[\{\(]*(?:#|$)');
final _lineTail = RegExp(r'\s*(?:#|$)');
final _nonSpace = RegExp(r'\S');
final _stringOrComment = RegExp('string|comment');
final _branch = RegExp(r'^(else:|elif |except |finally:)');

typedef _Run = String? Function(StringStream stream, PythonState state);

/// A tokenizer; string tokenizers are marked as upstream marks its
/// functions.
final class _Tokenizer {
  const _Tokenizer(this.run, {this.isString = false});
  final _Run run;
  final bool isString;
}

final class _Scope {
  const _Scope(this.offset, this.type, this.align);
  final int offset;
  final String type;
  final int? align;
}

final class PythonState {
  PythonState._(
    this._tokenize,
    this._scopes,
    this._indent,
    this._lastToken,
    this._lambda,
    this._dedent,
    this._errorToken,
    this._beginningOfLine,
  );
  _Tokenizer _tokenize;
  final List<_Scope> _scopes;
  int _indent;
  String? _lastToken;
  bool _lambda, _dedent, _errorToken, _beginningOfLine;

  /// The upstream default copy: the scope list is copied, scopes shared.
  PythonState copy() => PythonState._(
    _tokenize,
    List.of(_scopes),
    _indent,
    _lastToken,
    _lambda,
    _dedent,
    _errorToken,
    _beginningOfLine,
  );
}

_Scope _top(PythonState state) => state._scopes.last;

/// Python: the upstream `python` stream parser.
final class PythonMode extends Mode<PythonState> {
  PythonMode([super.config = const ModeConfig()]);

  late final _Tokenizer _tokenBaseT = _Tokenizer(_tokenBase);

  String? _tokenBase(StringStream stream, PythonState state) {
    final sol = stream.sol() && state._lastToken != r'\';
    if (sol) state._indent = stream.indentation();
    // Handle scope changes.
    if (sol && _top(state).type == 'py') {
      final scopeOffset = _top(state).offset;
      if (stream.eatSpace()) {
        final lineOffset = stream.indentation();
        if (lineOffset > scopeOffset) {
          _pushPyScope(stream, state);
        } else if (lineOffset < scopeOffset &&
            _dedentScopes(stream, state) &&
            stream.peek() != '#') {
          state._errorToken = true;
        }
        return null;
      } else {
        var style = _tokenBaseInner(stream, state);
        if (scopeOffset > 0 && _dedentScopes(stream, state)) {
          // Upstream appends to a null style as JavaScript does.
          style = '$style $_error';
        }
        return style;
      }
    }
    return _tokenBaseInner(stream, state);
  }

  String? _tokenBaseInner(
    StringStream stream,
    PythonState state, [
    bool inFormat = false,
  ]) {
    if (stream.eatSpace()) return null;

    // Comments
    if (!inFormat && stream.match(_comment) != null) return 'comment';

    // Number literals
    if (stream.match(_numberStart, consume: false) != null) {
      var floatLiteral = false;
      if (stream.match(_float) != null) floatLiteral = true;
      if (stream.match(_pointFloat) != null) floatLiteral = true;
      if (stream.match(_fraction) != null) floatLiteral = true;
      if (floatLiteral) {
        // Float literals may be imaginary.
        stream.eat(_imaginary);
        return 'number';
      }
      var intLiteral = false;
      if (stream.match(_hex) != null) intLiteral = true;
      if (stream.match(_binary) != null) intLiteral = true;
      if (stream.match(_octal) != null) intLiteral = true;
      if (stream.match(_decimal) != null) {
        // Decimal literals may be imaginary.
        stream.eat(_imaginary);
        intLiteral = true;
      }
      // Zero by itself with no other piece of number.
      if (stream.match(_zero) != null) intLiteral = true;
      if (intLiteral) {
        // Integer literals may be long.
        stream.eat(_long);
        return 'number';
      }
    }

    // Strings
    if (stream.match(_stringPrefixes) != null) {
      final isFmtString = stream.current().toLowerCase().contains('f');
      state._tokenize = isFmtString
          ? _formatStringFactory(stream.current(), state._tokenize)
          : _tokenStringFactory(stream.current(), state._tokenize);
      return state._tokenize.run(stream, state);
    }

    if (stream.match(_operators) != null) return 'operator';
    if (stream.match(_delimiters) != null) return 'punctuation';
    if (state._lastToken == '.' && stream.match(_identifiers) != null) {
      return 'property';
    }
    final word = _asciiWordAt(stream);
    if (_keywords.contains(word) || _wordOperators.contains(word)) {
      stream.pos += word.length;
      return 'keyword';
    }
    if (_builtins.contains(word)) {
      stream.pos += word.length;
      return 'builtin';
    }
    if (word == 'self' || word == 'cls') {
      stream.pos += word.length;
      return 'self';
    }
    if (stream.match(_identifiers) != null) {
      if (state._lastToken == 'def' || state._lastToken == 'class') {
        return 'def';
      }
      return 'variable';
    }

    // Anything else
    stream.next();
    return inFormat ? null : _error;
  }

  _Tokenizer _formatStringFactory(String delimiter, _Tokenizer tokenOuter) {
    while ('rubf'.contains(delimiter[0].toLowerCase())) {
      delimiter = delimiter.substring(1);
    }
    final singleline = delimiter.length == 1;

    late final _Tokenizer tokenString;
    _Tokenizer tokenNestedExpr(int depth) => _Tokenizer((stream, state) {
      final inner = _tokenBaseInner(stream, state, true);
      if (inner == 'punctuation') {
        if (stream.current() == '{') {
          state._tokenize = tokenNestedExpr(depth + 1);
        } else if (stream.current() == '}') {
          state._tokenize = depth > 1
              ? tokenNestedExpr(depth - 1)
              : tokenString;
        }
      }
      return inner;
    });

    tokenString = _Tokenizer((stream, state) {
      while (!stream.eol()) {
        stream.eatWhile(_formatBody);
        if (stream.eat(r'\') != null) {
          stream.next();
          if (singleline && stream.eol()) return 'string';
        } else if (stream.matchString(delimiter)) {
          state._tokenize = tokenOuter;
          return 'string';
        } else if (stream.matchString('{{')) {
          // An escaped brace.
          return 'string';
        } else if (stream.matchString('{', consume: false)) {
          // Into an interpolation.
          state._tokenize = tokenNestedExpr(0);
          if (stream.current().isNotEmpty) return 'string';
          return state._tokenize.run(stream, state);
        } else if (stream.matchString('}}')) {
          return 'string';
        } else if (stream.matchString('}')) {
          // A single closing brace is an error in an f-string.
          return _error;
        } else {
          stream.eat(_quote);
        }
      }
      if (singleline) state._tokenize = tokenOuter;
      return 'string';
    }, isString: true);
    return tokenString;
  }

  _Tokenizer _tokenStringFactory(String delimiter, _Tokenizer tokenOuter) {
    while ('rubf'.contains(delimiter[0].toLowerCase())) {
      delimiter = delimiter.substring(1);
    }
    final singleline = delimiter.length == 1;
    return _Tokenizer((stream, state) {
      while (!stream.eol()) {
        stream.eatWhile(_stringBody);
        if (stream.eat(r'\') != null) {
          stream.next();
          if (singleline && stream.eol()) return 'string';
        } else if (stream.matchString(delimiter)) {
          state._tokenize = tokenOuter;
          return 'string';
        } else {
          stream.eat(_quote);
        }
      }
      if (singleline) state._tokenize = tokenOuter;
      return 'string';
    }, isString: true);
  }

  void _pushPyScope(StringStream stream, PythonState state) {
    while (_top(state).type != 'py') {
      state._scopes.removeLast();
    }
    state._scopes.add(
      _Scope(_top(state).offset + stream.indentUnit, 'py', null),
    );
  }

  void _pushBracketScope(StringStream stream, PythonState state, String type) {
    final align = stream.match(_bracketTail, consume: false) != null
        ? null
        : stream.column() + 1;
    state._scopes.add(_Scope(state._indent + stream.indentUnit, type, align));
  }

  bool _dedentScopes(StringStream stream, PythonState state) {
    final indented = stream.indentation();
    while (state._scopes.length > 1 && _top(state).offset > indented) {
      if (_top(state).type != 'py') return true;
      state._scopes.removeLast();
    }
    return _top(state).offset != indented;
  }

  String? _tokenLexer(StringStream stream, PythonState state) {
    if (stream.sol()) {
      state._beginningOfLine = true;
      state._dedent = false;
    }

    var style = state._tokenize.run(stream, state);
    final current = stream.current();

    // Decorators
    if (state._beginningOfLine && current == '@') {
      return stream.match(_identifiers, consume: false) != null
          ? 'meta'
          : 'operator';
    }

    if (_nonSpace.hasMatch(current)) state._beginningOfLine = false;

    if ((style == 'variable' || style == 'builtin') &&
        state._lastToken == 'meta') {
      style = 'meta';
    }

    // Scope changes
    if (current == 'pass' || current == 'return') state._dedent = true;

    if (current == 'lambda') state._lambda = true;
    if (current == ':' &&
        !state._lambda &&
        _top(state).type == 'py' &&
        stream.match(_lineTail, consume: false) != null) {
      _pushPyScope(stream, state);
    }

    if (current.length == 1 && !_stringOrComment.hasMatch(style ?? 'null')) {
      var index = '[({'.indexOf(current);
      if (index != -1) {
        _pushBracketScope(stream, state, '])}'.substring(index, index + 1));
      }
      index = '])}'.indexOf(current);
      if (index != -1) {
        if (_top(state).type == current) {
          state._indent = state._scopes.removeLast().offset - stream.indentUnit;
        } else {
          return _error;
        }
      }
    }
    if (state._dedent &&
        stream.eol() &&
        _top(state).type == 'py' &&
        state._scopes.length > 1) {
      state._scopes.removeLast();
    }

    return style;
  }

  @override
  PythonState startState([int baseColumn = 0]) => PythonState._(
    _tokenBaseT,
    [const _Scope(0, 'py', null)],
    0,
    null,
    false,
    false,
    false,
    false,
  );

  @override
  PythonState copyState(PythonState state) => state.copy();

  @override
  String? token(StringStream stream, PythonState state) {
    final addErr = state._errorToken;
    if (addErr) state._errorToken = false;
    var style = _tokenLexer(stream, state);

    if (style != null && style.isNotEmpty && style != 'comment') {
      state._lastToken = style == 'keyword' || style == 'punctuation'
          ? stream.current()
          : style;
    }
    if (style == 'punctuation') style = null;

    if (stream.eol() && state._lambda) state._lambda = false;
    return addErr ? _error : style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(PythonState state, String textAfter, String line) {
    if (!identical(state._tokenize, _tokenBaseT)) {
      return state._tokenize.isString ? null : 0;
    }
    final scope = _top(state);
    final closing =
        scope.type == (textAfter.isEmpty ? '' : textAfter[0]) ||
        scope.type == 'py' && !state._dedent && _branch.hasMatch(textAfter);
    if (scope.align != null) return scope.align! - (closing ? 1 : 0);
    return scope.offset - (closing ? config.indentUnit : 0);
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(
    r'^\s*([\}\]\)]|else:|elif |except |finally:)$',
  );
}
