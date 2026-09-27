// Ported from @codemirror/legacy-modes 6.5.4 mode/clike.js: the shared
// `clike` stream parser and its `c`, `cpp`, `java`, `csharp`, `kotlin` and
// `dart` exports, with the options, hooks and helpers they use. The other
// exports (scala, shader, nesC, objectiveC, objectiveCpp, squirrel, ceylon)
// are not ported, nor are options none of these six sets.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

typedef _Tokenizer = String? Function(StringStream stream, ClikeState state);

/// A character hook: a style, or `false` to let the base tokenizer go on.
typedef _Hook = Object? Function(StringStream stream, ClikeState state);

/// The `token` hook: a replacement style, or null for upstream's undefined.
typedef _TokenHook =
    String? Function(StringStream stream, ClikeState state, String? style);

/// The `indent` hook: a column, or null to fall through.
typedef _IndentHook =
    int? Function(
      ClikeState state,
      _Context ctx,
      String textAfter,
      int indentUnit,
    );

/// A word list, or a predicate in its place, as upstream's `contains` reads.
typedef _Words = bool Function(String word);

final class _Context {
  _Context(
    this.indented,
    this.column,
    this.type,
    this.info,
    this.align,
    this.prev,
  );
  final int indented, column;
  final String type;
  final String? info;

  /// Null until a line decides it. Tokens set it in place, so state copies
  /// see the change, as they share upstream's objects.
  bool? align;
  final _Context? prev;
}

_Context _pushContext(ClikeState state, int col, String type, [String? info]) {
  var indent = state._indented;
  if (state._context.type == 'statement' && type != 'statement') {
    indent = state._context.indented;
  }
  return state._context = _Context(
    indent,
    col,
    type,
    info,
    null,
    state._context,
  );
}

_Context _popContext(ClikeState state) {
  final t = state._context.type;
  if (t == ')' || t == ']' || t == '}') {
    state._indented = state._context.indented;
  }
  return state._context = state._context.prev!;
}

final _typeEnd = RegExp(r'\S(?:[^- ]>|[*\]])\s*$|\*$');

bool _typeBefore(StringStream stream, ClikeState state, int pos) {
  if (state._prevToken == 'variable' || state._prevToken == 'type') {
    return true;
  }
  if (_typeEnd.hasMatch(stream.string.substring(0, pos))) return true;
  if (state._typeAtEndOfLine && stream.column() == stream.indentation()) {
    return true;
  }
  return false;
}

bool _isTopScope(_Context? context) {
  for (;;) {
    if (context == null || context.type == 'top') return true;
    if (context.type == '}' && context.prev!.info != 'namespace') {
      return false;
    }
    context = context.prev;
  }
}

final class ClikeState {
  ClikeState._(
    this._tokenize,
    this._context,
    this._indented,
    this._startOfLine,
    this._prevToken,
    this._typeAtEndOfLine,
    this._cpp11RawStringDelim,
    this._interpolationStack,
  );
  _Tokenizer? _tokenize;
  _Context _context;
  int _indented;
  bool _startOfLine;
  String? _prevToken;
  bool _typeAtEndOfLine;
  String? _cpp11RawStringDelim;
  final List<_Tokenizer?> _interpolationStack;

  /// The upstream default copy: the interpolation stack is copied, the
  /// context shared.
  ClikeState copy() => ClikeState._(
    _tokenize,
    _context,
    _indented,
    _startOfLine,
    _prevToken,
    _typeAtEndOfLine,
    _cpp11RawStringDelim,
    List.of(_interpolationStack),
  );
}

bool _none(String word) => false;

Set<String> _words(String str) => str.split(' ').toSet();

/// `\w` for one code unit.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

/// The default `isIdentifierChar`, `\w`, `$` and every code unit from
/// U+00A1 on.
bool _isIdentifierUnit(int u) => _isWordUnit(u) || u == 0x24 || u >= 0xa1;

// The defaults of options none of the ported exports sets, as tests on one
// character: `isPunctuationChar`, `numberStart` and `isOperatorChar`.
const _punctuationChars = '[]{}(),;:.';
bool _isNumberStart(String ch) => ch == '.' || _isDigitUnit(ch.codeUnitAt(0));
bool _isDigitUnit(int u) => u >= 0x30 && u <= 0x39;
bool _isOperatorChar(String ch) => '+-*&%=<>!?|/'.contains(ch);

final _number = RegExp(
  r'(?:0x[a-f\d]+|0b[01]+|(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?)(u|ll?|l|f)?',
  caseSensitive: false,
);

/// Whether the stream continues with `//` or `/*`, as `/^\/[\/*]/`.
bool _atComment(StringStream stream) {
  final s = stream.string, pos = stream.pos;
  if (pos + 1 >= s.length || s.codeUnitAt(pos) != 0x2f) return false;
  final next = s.codeUnitAt(pos + 1);
  return next == 0x2f || next == 0x2a;
}

final _restIsBlankOrComment = RegExp(r'\s*(?:\/\/.*)?$');
final _callOpen = RegExp(r'\s*\(');
final _caseOrDefault = RegExp(r'^(?:case|default)\b');

/// The upstream `parserConfig` options the ported exports set.
final class _Config {
  _Config({
    this.keywords = _none,
    this.types = _none,
    this.builtin = _none,
    this.blockKeywords = _none,
    this.defKeywords = _none,
    this.atoms = _none,
    this.hooks = const {},
    this.tokenHook,
    this.indentHook,
    this.multiLineStrings = false,
    this.indentStatements = true,
    this.typeFirstDefinitions = false,
    this.dontIndentStatements,
    this.namespaceSeparator,
    RegExp? number,
    this.isIdentifierChar = _isIdentifierUnit,
    this.isReservedIdentifier,
  }) : number = number ?? _number;
  final _Words keywords, types, builtin, blockKeywords, defKeywords, atoms;
  final Map<String, _Hook> hooks;
  final _TokenHook? tokenHook;
  final _IndentHook? indentHook;
  final bool multiLineStrings, indentStatements, typeFirstDefinitions;
  final RegExp? dontIndentStatements;
  final String? namespaceSeparator;
  final RegExp number;
  final bool Function(int unit) isIdentifierChar;
  final _Words? isReservedIdentifier;
}

/// C-like languages: the upstream `clike` stream parser, configured as one
/// of its exports.
final class ClikeMode extends Mode<ClikeState> {
  /// C: the upstream `c` export.
  ClikeMode.c([ModeConfig config = const ModeConfig()]) : this._(config, _c);

  /// C++: the upstream `cpp` export.
  ClikeMode.cpp([ModeConfig config = const ModeConfig()])
    : this._(config, _cpp);

  /// Java: the upstream `java` export.
  ClikeMode.java([ModeConfig config = const ModeConfig()])
    : this._(config, _java);

  /// C#: the upstream `csharp` export.
  ClikeMode.csharp([ModeConfig config = const ModeConfig()])
    : this._(config, _csharp);

  /// Kotlin: the upstream `kotlin` export.
  ClikeMode.kotlin([ModeConfig config = const ModeConfig()])
    : this._(config, _kotlin);

  /// Dart: the upstream `dart` export.
  ClikeMode.dart([ModeConfig config = const ModeConfig()])
    : this._(config, _dart);

  ClikeMode._(super.config, this._parserConfig);

  final _Config _parserConfig;

  // Upstream's scratch variables, shared by the parser's tokens and reset for
  // each one.
  String? _curPunc;
  bool _isDefKeyword = false;

  String? _tokenBase(StringStream stream, ClikeState state) {
    final p = _parserConfig;
    final ch = stream.next()!;
    final hook = p.hooks[ch];
    if (hook != null) {
      final result = hook(stream, state);
      if (result != false) return result as String?;
    }
    if (ch == '"' || ch == "'") {
      final tokenize = state._tokenize = _tokenString(ch);
      return tokenize(stream, state);
    }
    if (_isNumberStart(ch)) {
      stream.backUp(1);
      if (stream.match(p.number) != null) return 'number';
      stream.next();
    }
    if (_punctuationChars.contains(ch)) {
      _curPunc = ch;
      return null;
    }
    if (ch == '/') {
      if (stream.eat('*') != null) {
        state._tokenize = _tokenComment;
        return _tokenComment(stream, state);
      }
      if (stream.eat('/') != null) {
        stream.skipToEnd();
        return 'comment';
      }
    }
    if (_isOperatorChar(ch)) {
      while (!_atComment(stream) && stream.eat(_isOperatorChar) != null) {}
      return 'operator';
    }
    stream.eatWhileCode(p.isIdentifierChar);
    final separator = p.namespaceSeparator;
    if (separator != null) {
      while (stream.matchString(separator)) {
        stream.eatWhileCode(p.isIdentifierChar);
      }
    }

    final cur = stream.current();
    if (p.keywords(cur)) {
      if (p.blockKeywords(cur)) _curPunc = 'newstatement';
      if (p.defKeywords(cur)) _isDefKeyword = true;
      return 'keyword';
    }
    if (p.types(cur)) return 'type';
    if (p.builtin(cur) || (p.isReservedIdentifier?.call(cur) ?? false)) {
      if (p.blockKeywords(cur)) _curPunc = 'newstatement';
      return 'builtin';
    }
    if (p.atoms(cur)) return 'atom';
    return 'variable';
  }

  _Tokenizer _tokenString(String quote) => (stream, state) {
    var escaped = false, end = false;
    String? next;
    while ((next = stream.next()) != null) {
      if (next == quote && !escaped) {
        end = true;
        break;
      }
      escaped = !escaped && next == r'\';
    }
    if (end || !(escaped || _parserConfig.multiLineStrings)) {
      state._tokenize = null;
    }
    return 'string';
  };

  void _maybeEOL(StringStream stream, ClikeState state) {
    if (_parserConfig.typeFirstDefinitions &&
        stream.eol() &&
        _isTopScope(state._context)) {
      state._typeAtEndOfLine = _typeBefore(stream, state, stream.pos);
    }
  }

  @override
  ClikeState startState([int baseColumn = 0]) => ClikeState._(
    null,
    _Context(-config.indentUnit, 0, 'top', null, false, null),
    0,
    true,
    null,
    false,
    null,
    [],
  );

  @override
  ClikeState copyState(ClikeState state) => state.copy();

  @override
  String? token(StringStream stream, ClikeState state) {
    final p = _parserConfig;
    var ctx = state._context;
    if (stream.sol()) {
      ctx.align ??= false;
      state._indented = stream.indentation();
      state._startOfLine = true;
    }
    if (stream.eatSpace()) {
      _maybeEOL(stream, state);
      return null;
    }
    _curPunc = null;
    _isDefKeyword = false;
    final tokenize = state._tokenize;
    var style = tokenize != null
        ? tokenize(stream, state)
        : _tokenBase(stream, state);
    if (style == 'comment' || style == 'meta') return style;
    ctx.align ??= true;

    final curPunc = _curPunc;
    if (curPunc == ';' ||
        curPunc == ':' ||
        (curPunc == ',' &&
            stream.match(_restIsBlankOrComment, consume: false) != null)) {
      while (state._context.type == 'statement') {
        _popContext(state);
      }
    } else if (curPunc == '{') {
      _pushContext(state, stream.column(), '}');
    } else if (curPunc == '[') {
      _pushContext(state, stream.column(), ']');
    } else if (curPunc == '(') {
      _pushContext(state, stream.column(), ')');
    } else if (curPunc == '}') {
      while (ctx.type == 'statement') {
        ctx = _popContext(state);
      }
      if (ctx.type == '}') ctx = _popContext(state);
      while (ctx.type == 'statement') {
        ctx = _popContext(state);
      }
    } else if (curPunc == ctx.type) {
      _popContext(state);
    } else if (p.indentStatements &&
        (((ctx.type == '}' || ctx.type == 'top') && curPunc != ';') ||
            (ctx.type == 'statement' && curPunc == 'newstatement'))) {
      _pushContext(state, stream.column(), 'statement', stream.current());
    }

    if (style == 'variable' &&
        (state._prevToken == 'def' ||
            (p.typeFirstDefinitions &&
                _typeBefore(stream, state, stream.start) &&
                _isTopScope(state._context) &&
                stream.match(_callOpen, consume: false) != null))) {
      style = 'def';
    }

    final tokenHook = p.tokenHook;
    if (tokenHook != null) {
      final result = tokenHook(stream, state, style);
      if (result != null) style = result;
    }

    state._startOfLine = false;
    state._prevToken = _isDefKeyword ? 'def' : style ?? _curPunc;
    _maybeEOL(stream, state);
    return style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(ClikeState state, String textAfter, String line) {
    final p = _parserConfig;
    // Upstream also tests for its base tokenizer, which it never stores.
    if (state._tokenize != null ||
        state._typeAtEndOfLine && _isTopScope(state._context)) {
      return null;
    }
    var ctx = state._context;
    final firstChar = textAfter.isEmpty ? '' : textAfter[0];
    final closing = firstChar == ctx.type;
    if (ctx.type == 'statement' && firstChar == '}') ctx = ctx.prev!;
    final dontIndentStatements = p.dontIndentStatements;
    if (dontIndentStatements != null) {
      while (ctx.type == 'statement' &&
          dontIndentStatements.hasMatch(ctx.info!)) {
        ctx = ctx.prev!;
      }
    }
    final indentHook = p.indentHook;
    if (indentHook != null) {
      final hook = indentHook(state, ctx, textAfter, config.indentUnit);
      if (hook != null) return hook;
    }
    final switchBlock = ctx.prev != null && ctx.prev!.info == 'switch';
    if (ctx.type == 'statement') {
      return ctx.indented + (firstChar == '{' ? 0 : config.indentUnit);
    }
    if (ctx.align == true) return ctx.column + (closing ? 0 : 1);
    if (ctx.type == ')' && !closing) return ctx.indented + config.indentUnit;

    return ctx.indented +
        (closing ? 0 : config.indentUnit) +
        (!closing && switchBlock && !_caseOrDefault.hasMatch(textAfter)
            ? config.indentUnit
            : 0);
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s*(?:case .*?:|default:|\{\}?|\})$');
}

String? _tokenComment(StringStream stream, ClikeState state) {
  var maybeEnd = false;
  String? ch;
  while ((ch = stream.next()) != null) {
    if (ch == '/' && maybeEnd) {
      state._tokenize = null;
      break;
    }
    maybeEnd = ch == '*';
  }
  return 'comment';
}

const _cKeywords =
    'auto if break case register continue return default do sizeof '
    'static else struct switch extern typedef union for goto while enum const '
    'volatile inline restrict asm fortran';

// Keywords from https://en.cppreference.com/w/cpp/keyword includes C++20.
const _cppKeywords =
    'alignas alignof and and_eq audit axiom bitand bitor catch '
    'class compl concept constexpr const_cast decltype delete dynamic_cast '
    'explicit export final friend import module mutable namespace new noexcept '
    'not not_eq operator or or_eq override private protected public '
    'reinterpret_cast requires static_assert static_cast template this '
    'thread_local throw try typeid typename using virtual xor xor_eq';

final _basicCTypes = _words(
  'int long char short double float unsigned signed void bool',
);
final _posixType = RegExp(r'.+_t$');

// Returns true if identifier is a "C" type.
// C type is defined as those that are reserved by the compiler (basicTypes),
// and those that end in _t (Reserved by POSIX for types)
// http://www.gnu.org/software/libc/manual/html_node/Reserved-Names.html
bool _cTypes(String identifier) =>
    _basicCTypes.contains(identifier) || _posixType.hasMatch(identifier);

const _cBlockKeywords = 'case do else for if switch while struct enum union';
const _cDefKeywords = 'struct enum union';

final _lastChar = RegExp(r'.$');

Object? _cppHook(StringStream stream, ClikeState state) {
  if (!state._startOfLine) return false;
  _Tokenizer? next;
  for (String? ch; (ch = stream.peek()) != null;) {
    if (ch == r'\' && stream.match(_lastChar) != null) {
      next = _cppContinuation;
      break;
    } else if (ch == '/' && _atComment(stream)) {
      break;
    }
    stream.next();
  }
  state._tokenize = next;
  return 'meta';
}

/// [_cppHook] as the tokenizer of a continued directive's next line. It runs
/// only from a line's start, where `startOfLine` holds, so it never passes.
String? _cppContinuation(StringStream stream, ClikeState state) =>
    _cppHook(stream, state) as String?;

Object? _pointerHook(StringStream stream, ClikeState state) {
  if (state._prevToken == 'type') return 'type';
  return false;
}

/// JavaScript's `c !== c.toLowerCase()` for one code unit: the Dart VM's
/// `toLowerCase` knows fewer capitals than its Unicode properties do.
final _changesWhenLowercased = RegExp(r'\p{CWL}', unicode: true);

// For C and C++ (and ObjC): identifiers starting with __
// or _ followed by a capital letter are reserved for the compiler.
bool _cIsReservedIdentifier(String token) {
  if (token.length < 2) return false;
  if (token[0] != '_') return false;
  return token[1] == '_' || _changesWhenLowercased.hasMatch(token[1]);
}

/// `[\w\.']` for one code unit.
bool _isCpp14LiteralUnit(int u) => _isWordUnit(u) || u == 0x2e || u == 0x27;

Object? _cpp14Literal(StringStream stream, ClikeState state) {
  stream.eatWhileCode(_isCpp14LiteralUnit);
  return 'number';
}

final _rawStringPrefix = RegExp('(?:R|u8R|uR|UR|LR)');
final _rawStringOpen = RegExp(r'"([^\s\\()]{0,16})\(');
final _unicodePrefix = RegExp('(?:u8|u|U|L)');
final _quote = RegExp('["\']');

Object? _cpp11StringHook(StringStream stream, ClikeState state) {
  stream.backUp(1);
  // Raw strings.
  if (stream.match(_rawStringPrefix) != null) {
    final match = stream.match(_rawStringOpen);
    if (match == null) {
      return false;
    }
    state._cpp11RawStringDelim = match[1];
    state._tokenize = _tokenRawString;
    return _tokenRawString(stream, state);
  }
  // Unicode strings/chars.
  if (stream.match(_unicodePrefix) != null) {
    if (stream.match(_quote, consume: false) != null) {
      return 'string';
    }
    return false;
  }
  // Ignore this hook.
  stream.next();
  return false;
}

final _constructorName = RegExp(r'(\w+)::~?(\w+)$');

bool _cppLooksLikeConstructor(String word) {
  final lastTwo = _constructorName.firstMatch(word);
  return lastTwo != null && lastTwo[1] == lastTwo[2];
}

String? _cppTokenHook(StringStream stream, ClikeState state, String? style) {
  if (style == 'variable' &&
      stream.peek() == '(' &&
      (state._prevToken == ';' ||
          state._prevToken == null ||
          state._prevToken == '}') &&
      _cppLooksLikeConstructor(stream.current())) {
    return 'def';
  }
  return null;
}

// C#-style strings where "" escapes a quote.
String? _tokenAtString(StringStream stream, ClikeState state) {
  String? next;
  while ((next = stream.next()) != null) {
    if (next == '"' && stream.eat('"') == null) {
      state._tokenize = null;
      break;
    }
  }
  return 'string';
}

final _regexSpecial = RegExp(r'[^\w\s]');

// C++11 raw string literal is <prefix>"<delim>( anything )<delim>", where
// <delim> can be a string up to 16 characters long.
String? _tokenRawString(StringStream stream, ClikeState state) {
  // Escape characters that have special regex meanings.
  final delim = state._cpp11RawStringDelim!.replaceAllMapped(
    _regexSpecial,
    (m) => '\\${m[0]}',
  );
  final match = stream.match(RegExp('.*?\\)$delim"'));
  if (match != null) {
    state._tokenize = null;
  } else {
    stream.skipToEnd();
  }
  return 'string';
}

final _c = _Config(
  keywords: _words(_cKeywords).contains,
  types: _cTypes,
  blockKeywords: _words(_cBlockKeywords).contains,
  defKeywords: _words(_cDefKeywords).contains,
  typeFirstDefinitions: true,
  atoms: _words('NULL true false').contains,
  isReservedIdentifier: _cIsReservedIdentifier,
  hooks: {'#': _cppHook, '*': _pointerHook},
);

/// C++'s `isIdentifierChar`: the default's and `~`.
bool _isCppIdentifierUnit(int u) => u == 0x7e || _isIdentifierUnit(u);

final _cpp = _Config(
  keywords: _words('$_cKeywords $_cppKeywords').contains,
  types: _cTypes,
  blockKeywords: _words('$_cBlockKeywords class try catch').contains,
  defKeywords: _words('$_cDefKeywords class namespace').contains,
  typeFirstDefinitions: true,
  atoms: _words('true false NULL nullptr').contains,
  dontIndentStatements: RegExp(r'^template$'),
  isIdentifierChar: _isCppIdentifierUnit,
  isReservedIdentifier: _cIsReservedIdentifier,
  hooks: {
    '#': _cppHook,
    '*': _pointerHook,
    'u': _cpp11StringHook,
    'U': _cpp11StringHook,
    'L': _cpp11StringHook,
    'R': _cpp11StringHook,
    for (var digit = 0; digit < 10; digit++) '$digit': _cpp14Literal,
  },
  tokenHook: _cppTokenHook,
  namespaceSeparator: '::',
);

/// `[\w\$_]` for one code unit.
bool _isAnnotationUnit(int u) => _isWordUnit(u) || u == 0x24;

final _tripleQuoteAtEnd = RegExp(r'""$');

final _java = _Config(
  keywords: _words(
    'abstract assert break case catch class const continue default '
    'do else enum extends final finally for goto if implements import '
    'instanceof interface native new package private protected public '
    'return static strictfp super switch synchronized this throw throws '
    'transient try volatile while @interface',
  ).contains,
  types: _words(
    'var byte short int long float double boolean char void Boolean Byte '
    'Character Double Float Integer Long Number Object Short String '
    'StringBuffer StringBuilder Void',
  ).contains,
  blockKeywords: _words(
    'catch class do else finally for if switch try while',
  ).contains,
  defKeywords: _words('class interface enum @interface').contains,
  typeFirstDefinitions: true,
  atoms: _words('true false null').contains,
  number: RegExp(
    r'(?:0x[a-f\d_]+|0b[01_]+|(?:[\d_]+\.?\d*|\.\d+)(?:e[-+]?[\d_]+)?)'
    r'(u|ll?|l|f)?',
    caseSensitive: false,
  ),
  hooks: {
    '@': (stream, state) {
      // Don't match the @interface keyword.
      if (stream.matchString('interface', consume: false)) return false;

      stream.eatWhileCode(_isAnnotationUnit);
      return 'meta';
    },
    '"': (stream, state) {
      if (stream.match(_tripleQuoteAtEnd) == null) return false;
      state._tokenize = _tokenTripleString;
      return _tokenTripleString(stream, state);
    },
  },
);

final _csharp = _Config(
  keywords: _words(
    'abstract as async await base break case catch checked class const '
    'continue default delegate do else enum event explicit extern finally '
    'fixed for foreach goto if implicit in init interface internal is lock '
    'namespace new operator out override params private protected public '
    'readonly record ref required return sealed sizeof stackalloc static '
    'struct switch this throw try typeof unchecked unsafe using virtual void '
    'volatile while add alias ascending descending dynamic from get global '
    'group into join let orderby partial remove select set value var yield',
  ).contains,
  types: _words(
    'Action Boolean Byte Char DateTime DateTimeOffset Decimal Double Func '
    'Guid Int16 Int32 Int64 Object SByte Single String Task TimeSpan UInt16 '
    'UInt32 UInt64 bool byte char decimal double short int long object sbyte '
    'float string ushort uint ulong',
  ).contains,
  blockKeywords: _words(
    'catch class do else finally for foreach if struct switch try while',
  ).contains,
  defKeywords: _words('class interface namespace record struct var').contains,
  typeFirstDefinitions: true,
  atoms: _words('true false null').contains,
  hooks: {
    '@': (stream, state) {
      if (stream.eat('"') != null) {
        state._tokenize = _tokenAtString;
        return _tokenAtString(stream, state);
      }
      stream.eatWhileCode(_isAnnotationUnit);
      return 'meta';
    },
  },
);

String? _tokenTripleString(StringStream stream, ClikeState state) {
  var escaped = false;
  while (!stream.eol()) {
    if (!escaped && stream.matchString('"""')) {
      state._tokenize = null;
      break;
    }
    escaped = stream.next() == r'\' && !escaped;
  }
  return 'string';
}

_Tokenizer _tokenNestedComment(int depth) => (stream, state) {
  String? ch;
  while ((ch = stream.next()) != null) {
    if (ch == '*' && stream.eat('/') != null) {
      if (depth == 1) {
        state._tokenize = null;
        break;
      } else {
        final tokenize = state._tokenize = _tokenNestedComment(depth - 1);
        return tokenize(stream, state);
      }
    } else if (ch == '/' && stream.eat('*') != null) {
      final tokenize = state._tokenize = _tokenNestedComment(depth + 1);
      return tokenize(stream, state);
    }
  }
  return 'comment';
};

Object? _nestedCommentHook(StringStream stream, ClikeState state) {
  if (stream.eat('*') == null) return false;
  final tokenize = state._tokenize = _tokenNestedComment(1);
  return tokenize(stream, state);
}

Object? _annotationHook(StringStream stream, ClikeState state) {
  stream.eatWhileCode(_isAnnotationUnit);
  return 'meta';
}

_Tokenizer _tokenKotlinString(bool tripleString) => (stream, state) {
  var escaped = false, end = false;
  while (!stream.eol()) {
    if (!tripleString && !escaped && stream.matchString('"')) {
      end = true;
      break;
    }
    if (tripleString && stream.matchString('"""')) {
      end = true;
      break;
    }
    final next = stream.next();
    if (!escaped && next == r'$' && stream.matchString('{')) {
      stream.skipTo('}');
    }
    escaped = !escaped && next == r'\' && !tripleString;
  }
  if (end || !tripleString) state._tokenize = null;
  return 'string';
};

int? _kotlinIndent(
  ClikeState state,
  _Context ctx,
  String textAfter,
  int indentUnit,
) {
  final firstChar = textAfter.isEmpty ? '' : textAfter[0];
  if ((state._prevToken == '}' || state._prevToken == ')') && textAfter == '') {
    return state._indented;
  }
  if ((state._prevToken == 'operator' &&
          textAfter != '}' &&
          state._context.type != '}') ||
      state._prevToken == 'variable' && firstChar == '.' ||
      (state._prevToken == '}' || state._prevToken == ')') &&
          firstChar == '.') {
    return indentUnit * 2 + ctx.indented;
  }
  if (ctx.align == true && ctx.type == '}') {
    return ctx.indented + (state._context.type == firstChar ? 0 : indentUnit);
  }
  return null;
}

final _kotlin = _Config(
  keywords: _words(
    // keywords
    'package as typealias class interface this super val operator '
    'var fun for is in This throw return annotation '
    'break continue object if else while do try when !in !is as? '
    // soft keywords
    'file import where by get set abstract enum open inner override private '
    'public internal protected catch finally out final vararg reified '
    'dynamic companion constructor init sealed field property receiver '
    'param sparam lateinit data inline noinline tailrec external annotation '
    'crossinline const operator infix suspend actual expect setparam',
  ).contains,
  types: _words(
    // package java.lang
    'Boolean Byte Character CharSequence Class ClassLoader Cloneable '
    'Comparable Compiler Double Exception Float Integer Long Math Number '
    'Object Package Pair Process Runtime Runnable SecurityManager Short '
    'StackTraceElement StrictMath String StringBuffer System Thread '
    'ThreadGroup ThreadLocal Throwable Triple Void Annotation Any '
    'BooleanArray ByteArray Char CharArray DeprecationLevel DoubleArray Enum '
    'FloatArray Function Int IntArray Lazy LazyThreadSafetyMode LongArray '
    'Nothing ShortArray Unit',
  ).contains,
  // Upstream also sets `intendSwitch`, a misspelling that changes nothing.
  indentStatements: false,
  multiLineStrings: true,
  number: RegExp(
    r'(?:0x[a-f\d_]+|0b[01_]+|(?:[\d_]+(\.\d+)?|\.\d+)(?:e[-+]?[\d_]+)?)'
    r'(ul?|l|f)?',
    caseSensitive: false,
  ),
  blockKeywords: _words(
    'catch class do else finally for if where try while enum',
  ).contains,
  defKeywords: _words('class val var object interface fun').contains,
  atoms: _words('true false null this').contains,
  hooks: {
    '@': _annotationHook,
    '*': (stream, state) => state._prevToken == '.' ? 'variable' : 'operator',
    '"': (stream, state) {
      final tokenize = state._tokenize = _tokenKotlinString(
        stream.matchString('""'),
      );
      return tokenize(stream, state);
    },
    '/': _nestedCommentHook,
  },
  indentHook: _kotlinIndent,
);

void _pushInterpolationStack(ClikeState state) {
  state._interpolationStack.add(state._tokenize);
}

_Tokenizer? _popInterpolationStack(ClikeState state) =>
    state._interpolationStack.isEmpty
    ? null
    : state._interpolationStack.removeLast();

int _sizeInterpolationStack(ClikeState state) =>
    state._interpolationStack.length;

String? _tokenDartString(
  String quote,
  StringStream stream,
  ClikeState state,
  bool raw,
) {
  var tripleQuoted = false;
  if (stream.eat(quote) != null) {
    if (stream.eat(quote) != null) {
      tripleQuoted = true;
    } else {
      return 'string'; //empty string
    }
  }
  String? tokenStringHelper(StringStream stream, ClikeState state) {
    var escaped = false;
    while (!stream.eol()) {
      if (!raw && !escaped && stream.peek() == r'$') {
        _pushInterpolationStack(state);
        state._tokenize = _tokenInterpolation;
        return 'string';
      }
      final next = stream.next();
      if (next == quote &&
          !escaped &&
          (!tripleQuoted || stream.matchString(quote + quote))) {
        state._tokenize = null;
        break;
      }
      escaped = !raw && !escaped && next == r'\';
    }
    return 'string';
  }

  state._tokenize = tokenStringHelper;
  return tokenStringHelper(stream, state);
}

String? _tokenInterpolation(StringStream stream, ClikeState state) {
  stream.eat(r'$');
  if (stream.eat('{') != null) {
    // let clike handle the content of ${...},
    // we take over again when "}" appears (see hooks).
    state._tokenize = null;
  } else {
    state._tokenize = _tokenInterpolationIdentifier;
  }
  return null;
}

String? _tokenInterpolationIdentifier(StringStream stream, ClikeState state) {
  stream.eatWhileCode(_isWordUnit);
  state._tokenize = _popInterpolationStack(state);
  return 'variable';
}

/// `[\w\$_\.]` for one code unit.
bool _isDartAnnotationUnit(int u) => _isAnnotationUnit(u) || u == 0x2e;

final _isUpper = RegExp(r'^[_$]*[A-Z][a-zA-Z0-9_$]*$');

final _dart = _Config(
  keywords: _words(
    'this super static final const abstract class extends external factory '
    'implements mixin get native set typedef with enum throw rethrow assert '
    'break case continue default in return new deferred async await '
    'covariant try catch finally do else for if switch while import library '
    'export part of show hide is as extension on yield late required sealed '
    'base interface when inline',
  ).contains,
  blockKeywords: _words(
    'try catch finally do else for if switch while',
  ).contains,
  builtin: _words(
    'void bool num int double dynamic var String Null Never',
  ).contains,
  atoms: _words('true false null').contains,
  // clike numbers without the suffixes, and with '_' separators.
  number: RegExp(
    r'(?:0x[a-f\d_]+|(?:[\d_]+\.?[\d_]*|\.[\d_]+)(?:e[-+]?[\d_]+)?)',
    caseSensitive: false,
  ),
  hooks: {
    '@': (stream, state) {
      stream.eatWhileCode(_isDartAnnotationUnit);
      return 'meta';
    },

    // custom string handling to deal with triple-quoted strings and string
    // interpolation
    "'": (stream, state) => _tokenDartString("'", stream, state, false),
    '"': (stream, state) => _tokenDartString('"', stream, state, false),
    'r': (stream, state) {
      final peek = stream.peek();
      if (peek == "'" || peek == '"') {
        return _tokenDartString(stream.next()!, stream, state, true);
      }
      return false;
    },

    '}': (stream, state) {
      // "}" is end of interpolation, if interpolation stack is non-empty
      if (_sizeInterpolationStack(state) > 0) {
        state._tokenize = _popInterpolationStack(state);
        return null;
      }
      return false;
    },

    '/': _nestedCommentHook,
  },
  tokenHook: (stream, state, style) {
    if (style == 'variable') {
      // Assume uppercase symbols are classes
      if (_isUpper.hasMatch(stream.current())) {
        return 'type';
      }
    }
    return null;
  },
);
