// Ported from CodeMirror 5.65.21 mode/javascript/javascript.js.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE
//
// The port keeps the upstream structure: a tokenizer, then a parser of
// combinators on a stack, communicating through shared scratch fields. Where
// JavaScript relies on `undefined` (a token's missing content, a type left
// over from the previous token), the port does the same with null.

import '../mode.dart';
import '../stream.dart';

/// `parserConfig` of the upstream mode.
final class JavaScriptOptions {
  const JavaScriptOptions({
    this.json = false,
    this.jsonld = false,
    this.typescript = false,
    this.trackScope = true,
    this.statementIndent,
    this.doubleIndentSwitch = true,
    this.wordCharacters,
  });
  final bool json, jsonld, typescript, trackScope, doubleIndentSwitch;
  final int? statementIndent;
  final RegExp? wordCharacters;
}

typedef _Tokenizer = String? Function(StringStream stream, JsState state);
typedef _Run = bool Function(String? type, String? value);

/// A parser step. Lexical steps run as soon as they reach the top of the
/// stack after a token has been consumed.
final class _C {
  const _C(this.run, [this.lex = false]);
  final _Run run;
  final bool lex;
}

final class JsLexical {
  JsLexical(
    this.indented,
    this.column,
    this.type,
    this.align,
    this.prev,
    this.info,
  );
  final int indented, column;
  final String type;
  final JsLexical? prev;
  final String? info;

  /// Null until decided, like the upstream object's missing own property.
  bool? align;
  int? pos;
}

final class _Var {
  const _Var(this.name, this.next);

  /// Null for a private name (`#x`), whose token carries no content upstream.
  final String? name;
  final _Var? next;
}

final class _Context {
  const _Context(this.prev, this.vars, this.block);
  final _Context? prev;
  final _Var? vars;
  final bool block;
}

final class JsState {
  JsState._(
    this._tokenize,
    this.lastType,
    this._stack,
    this.lexical,
    this._localVars,
    this._context,
    this.indented,
    this.fatArrowAt,
  );
  _Tokenizer _tokenize;
  String? lastType;
  final List<_C> _stack;
  JsLexical lexical;
  _Var? _localVars;
  _Context? _context;
  int indented;
  int? fatArrowAt;

  /// The upstream default copy: the stack is copied, everything else shared.
  JsState copy() => JsState._(
    _tokenize,
    lastType,
    List.of(_stack),
    lexical,
    _localVars,
    _context,
    indented,
    fatArrowAt,
  );
}

final class _Keyword {
  const _Keyword(this.type, this.style);
  final String type, style;
}

// Single-character classes are string tests: a regular expression per
// character costs most under dart2wasm, where it calls into JavaScript.
const _operatorChars = '+-*&%=<>!?|~^@';
const _punctuation = '[]{}(),;:.';
const _doubled = '<>*+-|&?';
const _quoteChars = '"\'/`';
const _closers = ';})],';
const _typeClosers = '})]';

bool _isOperatorChar(String ch) => ch.isNotEmpty && _operatorChars.contains(ch);

/// Whether [type] contains any of [chars], as upstream's `type.match(/[...]/)`.
bool _hasAny(String type, String chars) {
  for (var i = 0; i < type.length; i++) {
    if (chars.contains(type[i])) return true;
  }
  return false;
}

/// The default word characters, `[\w$\xa1-\uffff]`, for one code unit.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f ||
    u == 0x24 ||
    u >= 0xa1;
final _isJsonldKeyword = RegExp(
  r'@(context|id|value|language|type|container|list|set|reverse|index|base|vocab|graph)"',
);
final _numberAfterDot = RegExp(r'\d[\d_]*(?:[eE][+\-]?[\d_]+)?');
final _radixNumber = RegExp(r'(?:x[\dA-Fa-f_]+|o[0-7_]+|b[01_]+)n?');
final _numberRest = RegExp(r'[\d_]*(?:n|(?:\.[\d_]*)?(?:[eE][+\-]?[\d_]+)?)?');
final _regexpFlags = RegExp(r'\b(([gimyus])(?![gimyus]*\2))+\b');
final _nonSpaceChar = RegExp(r'\S');
final _asyncAhead = RegExp(r'(\s|\/\*([^*]|\*(?!\/))*?\*\/)*[\[\(\w]');
final _tsReturnType = RegExp(r':\s*(?:\w+(?:<[^>]*>|\[\])?|\{[^}]*\})\s*$');
final _restIsBlank = RegExp(r'\s*$');
final _spaceWord = RegExp(r'\s*\w');
final _incDec = RegExp(r'\+\+|--');
final _typeArgsCall = RegExp(r'([^<>]|<[^<>]*>)*>\s*\(');
final _typedArrowParam = RegExp(r'\s*:\s*');
final _typePredicate = RegExp(r'\s*\w+\s+is\b');
final _optionalOrTyped = RegExp(r'\s*[?:]');
final _spaceColon = RegExp(r'\s*:');
final _classModifierAhead = RegExp(r'\s+#?[\w$\xa1-\uffff]');
final _elseAhead = RegExp(r'^\s*else\b');
final _continuation = RegExp(r'^[,\.=+\-*:?[\(]');
final _caseOrDefault = RegExp(r'^(?:case|default)\b');
final _expressionAfter = RegExp(
  r'^(?:operator|sof|keyword [bcd]|case|new|export|default|spread|[\[{}\(,;:]|=>)$',
);
final _openBraceEnd = RegExp(r'\{\s*$');
const _brackets = '([{}])';

const _atomicTypes = {
  'atom',
  'number',
  'variable',
  'string',
  'regexp',
  'this',
  'import',
  'jsonld-keyword',
};

const _a = _Keyword('keyword a', 'keyword'),
    _b = _Keyword('keyword b', 'keyword'),
    _cKw = _Keyword('keyword c', 'keyword'),
    _d = _Keyword('keyword d', 'keyword'),
    _operatorKw = _Keyword('operator', 'keyword'),
    _atomKw = _Keyword('atom', 'atom');
const _keywords = {
  'if': _Keyword('if', 'keyword'),
  'while': _a,
  'with': _a,
  'else': _b,
  'do': _b,
  'try': _b,
  'finally': _b,
  'return': _d,
  'break': _d,
  'continue': _d,
  'new': _Keyword('new', 'keyword'),
  'delete': _cKw,
  'void': _cKw,
  'throw': _cKw,
  'debugger': _Keyword('debugger', 'keyword'),
  'var': _Keyword('var', 'keyword'),
  'const': _Keyword('var', 'keyword'),
  'let': _Keyword('var', 'keyword'),
  'function': _Keyword('function', 'keyword'),
  'catch': _Keyword('catch', 'keyword'),
  'for': _Keyword('for', 'keyword'),
  'switch': _Keyword('switch', 'keyword'),
  'case': _Keyword('case', 'keyword'),
  'default': _Keyword('default', 'keyword'),
  'in': _operatorKw,
  'typeof': _operatorKw,
  'instanceof': _operatorKw,
  'true': _atomKw,
  'false': _atomKw,
  'null': _atomKw,
  'undefined': _atomKw,
  'NaN': _atomKw,
  'Infinity': _atomKw,
  'this': _Keyword('this', 'keyword'),
  'class': _Keyword('class', 'keyword'),
  'super': _Keyword('atom', 'keyword'),
  'yield': _cKw,
  'export': _Keyword('export', 'keyword'),
  'import': _Keyword('import', 'keyword'),
  'extends': _cKw,
  'await': _cKw,
};

const _defaultVars = _Var('this', _Var('arguments', null));

bool _isModifier(String? name) =>
    name == 'public' ||
    name == 'private' ||
    name == 'protected' ||
    name == 'abstract' ||
    name == 'readonly';

bool _inList(String? name, _Var? list) {
  for (var v = list; v != null; v = v.next) {
    if (v.name == name) return true;
  }
  return false;
}

/// JavaScript's `value.slice(value.length - 2)`.
String _lastTwo(String value) =>
    value.length > 2 ? value.substring(value.length - 2) : value;

/// JavaScript, TypeScript and JSON: CodeMirror's `javascript` mode.
final class JavaScriptMode extends Mode<JsState> {
  JavaScriptMode([
    this.config = const ModeConfig(),
    this.options = const JavaScriptOptions(),
  ]) : _jsonldMode = options.jsonld,
       _jsonMode = options.json || options.jsonld,
       _isTS = options.typescript,
       _wordRE = options.wordCharacters;

  final ModeConfig config;
  final JavaScriptOptions options;
  final bool _jsonldMode, _jsonMode, _isTS;
  final RegExp? _wordRE;

  bool _isWord(String ch) {
    final custom = _wordRE;
    return custom == null ? _isWordUnit(ch.codeUnitAt(0)) : custom.hasMatch(ch);
  }

  bool _eatWord(StringStream stream) {
    final custom = _wordRE;
    return custom == null
        ? stream.eatWhileCode(_isWordUnit)
        : stream.eatWhile(custom);
  }

  int get _indentUnit => config.indentUnit;

  // Scratch values from the tokenizer to the parser (`type`, `content`).
  String? _type, _content;

  // The parser's context (`cx`).
  late JsState _state;
  late StringStream _stream;
  String? _marked, _style;
  late List<_C> _cc;

  late final _Tokenizer _tokenBaseT = _tokenBase;
  late final _Tokenizer _tokenCommentT = _tokenComment;
  late final _Tokenizer _tokenQuasiT = _tokenQuasi;

  String? _ret(String tp, [String? style, String? cont]) {
    _type = tp;
    _content = cont;
    return style;
  }

  // Tokenizer

  static void _readRegexp(StringStream stream) {
    var escaped = false, inSet = false;
    String? next;
    while ((next = stream.next()) != null) {
      if (!escaped) {
        if (next == '/' && !inSet) return;
        if (next == '[') {
          inSet = true;
        } else if (inSet && next == ']') {
          inSet = false;
        }
      }
      escaped = !escaped && next == r'\';
    }
  }

  String? _tokenBase(StringStream stream, JsState state) {
    final ch = stream.next()!;
    if (ch == '"' || ch == "'") {
      state._tokenize = _tokenString(ch);
      return state._tokenize(stream, state);
    } else if (ch == '.' && stream.match(_numberAfterDot) != null) {
      return _ret('number', 'number');
    } else if (ch == '.' && stream.matchString('..')) {
      return _ret('spread', 'meta');
    } else if (_punctuation.contains(ch)) {
      return _ret(ch);
    } else if (ch == '=' && stream.eat('>') != null) {
      return _ret('=>', 'operator');
    } else if (ch == '0' && stream.match(_radixNumber) != null) {
      return _ret('number', 'number');
    } else if (ch.codeUnitAt(0) >= 0x30 && ch.codeUnitAt(0) <= 0x39) {
      stream.match(_numberRest);
      return _ret('number', 'number');
    } else if (ch == '/') {
      if (stream.eat('*') != null) {
        state._tokenize = _tokenCommentT;
        return _tokenComment(stream, state);
      } else if (stream.eat('/') != null) {
        stream.skipToEnd();
        return _ret('comment', 'comment');
      } else if (expressionAllowed(stream, state, 1)) {
        _readRegexp(stream);
        stream.match(_regexpFlags);
        return _ret('regexp', 'string-2');
      } else {
        stream.eat('=');
        return _ret('operator', 'operator', stream.current());
      }
    } else if (ch == '`') {
      state._tokenize = _tokenQuasiT;
      return _tokenQuasi(stream, state);
    } else if (ch == '#' && stream.peek() == '!') {
      stream.skipToEnd();
      return _ret('meta', 'meta');
    } else if (ch == '#' && _eatWord(stream)) {
      return _ret('variable', 'property');
    } else if (ch == '<' && stream.matchString('!--') ||
        (ch == '-' &&
            stream.matchString('->') &&
            !_nonSpaceChar.hasMatch(
              stream.string.substring(0, stream.start),
            ))) {
      stream.skipToEnd();
      return _ret('comment', 'comment');
    } else if (_isOperatorChar(ch)) {
      if (ch != '>' || state.lexical.type != '>') {
        if (stream.eat('=') != null) {
          if (ch == '!' || ch == '=') stream.eat('=');
        } else if (_doubled.contains(ch)) {
          stream.eat(ch);
          if (ch == '>') stream.eat(ch);
        }
      }
      if (ch == '?' && stream.eat('.') != null) return _ret('.');
      return _ret('operator', 'operator', stream.current());
    } else if (_isWord(ch)) {
      _eatWord(stream);
      final word = stream.current();
      if (state.lastType != '.') {
        final kw = _keywords[word];
        if (kw != null) return _ret(kw.type, kw.style, word);
        if (word == 'async' &&
            stream.match(_asyncAhead, consume: false) != null) {
          return _ret('async', 'keyword', word);
        }
      }
      return _ret('variable', 'variable', word);
    }
    // No token type: the previous token's type and content stand.
    return null;
  }

  _Tokenizer _tokenString(String quote) => (stream, state) {
    var escaped = false;
    String? next;
    if (_jsonldMode &&
        stream.peek() == '@' &&
        stream.match(_isJsonldKeyword) != null) {
      state._tokenize = _tokenBaseT;
      return _ret('jsonld-keyword', 'meta');
    }
    while ((next = stream.next()) != null) {
      if (next == quote && !escaped) break;
      escaped = !escaped && next == r'\';
    }
    if (!escaped) state._tokenize = _tokenBaseT;
    return _ret('string', 'string');
  };

  String? _tokenComment(StringStream stream, JsState state) {
    var maybeEnd = false;
    String? ch;
    while ((ch = stream.next()) != null) {
      if (ch == '/' && maybeEnd) {
        state._tokenize = _tokenBaseT;
        break;
      }
      maybeEnd = ch == '*';
    }
    return _ret('comment', 'comment');
  }

  String? _tokenQuasi(StringStream stream, JsState state) {
    var escaped = false;
    String? next;
    while ((next = stream.next()) != null) {
      if (!escaped &&
          (next == '`' || next == r'$' && stream.eat('{') != null)) {
        state._tokenize = _tokenBaseT;
        break;
      }
      escaped = !escaped && next == r'\';
    }
    return _ret('quasi', 'string-2', stream.current());
  }

  // A crude lookahead that notices the parameters of an arrow function
  // before the arrow when both are on one line.
  void _findFatArrow(StringStream stream, JsState state) {
    if (state.fatArrowAt != null && state.fatArrowAt != 0) {
      state.fatArrowAt = null;
    }
    var arrow = stream.string.indexOf('=>', stream.start);
    if (arrow < 0) return;

    if (_isTS) {
      final m = _tsReturnType.firstMatch(
        stream.string.substring(stream.start, arrow),
      );
      if (m != null) arrow = m.start;
    }

    var depth = 0, sawSomething = false;
    var pos = arrow - 1;
    for (; pos >= 0; --pos) {
      final ch = stream.string[pos];
      final bracket = _brackets.indexOf(ch);
      if (bracket >= 0 && bracket < 3) {
        if (depth == 0) {
          ++pos;
          break;
        }
        if (--depth == 0) {
          if (ch == '(') sawSomething = true;
          break;
        }
      } else if (bracket >= 3 && bracket < 6) {
        ++depth;
      } else if (_isWord(ch)) {
        sawSomething = true;
      } else if (_quoteChars.contains(ch)) {
        for (; ; --pos) {
          if (pos == 0) return;
          final next = stream.string[pos - 1];
          if (next == ch && (pos < 2 || stream.string[pos - 2] != r'\')) {
            pos--;
            break;
          }
        }
      } else if (sawSomething && depth == 0) {
        ++pos;
        break;
      }
    }
    if (sawSomething && depth == 0) state.fatArrowAt = pos;
  }

  // Parser

  bool _inScope(JsState state, String? varname) {
    if (!options.trackScope) return false;
    for (var v = state._localVars; v != null; v = v.next) {
      if (v.name == varname) return true;
    }
    for (var cx = state._context; cx != null; cx = cx.prev) {
      for (var v = cx.vars; v != null; v = v.next) {
        if (v.name == varname) return true;
      }
    }
    return false;
  }

  String? _parseJS(
    JsState state,
    String? style,
    String? type,
    String? content,
    StringStream stream,
  ) {
    final cc = state._stack;
    _state = state;
    _stream = stream;
    _marked = null;
    _cc = cc;
    _style = style;

    state.lexical.align ??= true;

    while (true) {
      final combinator = cc.isNotEmpty
          ? cc.removeLast()
          : _jsonMode
          ? _expression
          : _statement;
      if (combinator.run(type, content)) {
        while (cc.isNotEmpty && cc.last.lex) {
          cc.removeLast().run(null, null);
        }
        if (_marked != null) return _marked;
        if (type == 'variable' && _inScope(state, content)) {
          return 'variable-2';
        }
        return style;
      }
    }
  }

  // Combinator utilities

  bool _pass([_C? a, _C? b, _C? c, _C? d, _C? e, _C? f, _C? g, _C? h, _C? i]) {
    final cc = _cc;
    if (i != null) cc.add(i);
    if (h != null) cc.add(h);
    if (g != null) cc.add(g);
    if (f != null) cc.add(f);
    if (e != null) cc.add(e);
    if (d != null) cc.add(d);
    if (c != null) cc.add(c);
    if (b != null) cc.add(b);
    if (a != null) cc.add(a);
    return false;
  }

  bool _cont([_C? a, _C? b, _C? c, _C? d, _C? e, _C? f, _C? g, _C? h, _C? i]) {
    _pass(a, b, c, d, e, f, g, h, i);
    return true;
  }

  void _register(String? varname) {
    final state = _state;
    _marked = 'def';
    if (!options.trackScope) return;
    if (state._context != null) {
      if (state.lexical.info == 'var' && state._context!.block) {
        // Upstream notes that function declarations are not block scoped.
        final newContext = _registerVarScoped(varname, state._context);
        if (newContext != null) {
          state._context = newContext;
          return;
        }
      } else if (!_inList(varname, state._localVars)) {
        state._localVars = _Var(varname, state._localVars);
        return;
      }
    }
    // Falling through means a global; this port has no `globalVars` option.
  }

  _Context? _registerVarScoped(String? varname, _Context? context) {
    if (context == null) {
      return null;
    } else if (context.block) {
      final inner = _registerVarScoped(varname, context.prev);
      if (inner == null) return null;
      if (inner == context.prev) return context;
      return _Context(inner, context.vars, true);
    } else if (_inList(varname, context.vars)) {
      return context;
    } else {
      return _Context(context.prev, _Var(varname, context.vars), false);
    }
  }

  // Combinators

  late final _C _pushcontext = _C((_, _) {
    _state._context = _Context(_state._context, _state._localVars, false);
    _state._localVars = _defaultVars;
    return false;
  }, true);
  late final _C _pushblockcontext = _C((_, _) {
    _state._context = _Context(_state._context, _state._localVars, true);
    _state._localVars = null;
    return false;
  }, true);
  late final _C _popcontext = _C((_, _) {
    _state._localVars = _state._context!.vars;
    _state._context = _state._context!.prev;
    return false;
  }, true);

  _C _pushlex(String type, [String? info]) => _C((_, _) {
    final state = _state;
    var indent = state.indented;
    if (state.lexical.type == 'stat') {
      indent = state.lexical.indented;
    } else {
      for (
        JsLexical? outer = state.lexical;
        outer != null && outer.type == ')' && outer.align == true;
        outer = outer.prev
      ) {
        indent = outer.indented;
      }
    }
    state.lexical = JsLexical(
      indent,
      _stream.column(),
      type,
      null,
      state.lexical,
      info,
    );
    return false;
  }, true);

  late final _C _poplex = _C((_, _) {
    final state = _state;
    final prev = state.lexical.prev;
    if (prev != null) {
      if (state.lexical.type == ')') state.indented = state.lexical.indented;
      state.lexical = prev;
    }
    return false;
  }, true);

  _C _expect(String wanted) {
    late final _C exp;
    exp = _C((type, _) {
      if (type == wanted) {
        return _cont();
      } else if (wanted == ';' || type == '}' || type == ')' || type == ']') {
        return _pass();
      } else {
        return _cont(exp);
      }
    });
    return exp;
  }

  late final _C _statement = _C(_statementF);
  bool _statementF(String? type, String? value) {
    if (type == 'var') {
      return _cont(_pushlex('vardef', value), _vardef, _expect(';'), _poplex);
    }
    if (type == 'keyword a') {
      return _cont(_pushlex('form'), _parenExpr, _statement, _poplex);
    }
    if (type == 'keyword b') {
      return _cont(_pushlex('form'), _statement, _poplex);
    }
    if (type == 'keyword d') {
      return _stream.match(_restIsBlank, consume: false) != null
          ? _cont()
          : _cont(_pushlex('stat'), _maybeexpression, _expect(';'), _poplex);
    }
    if (type == 'debugger') return _cont(_expect(';'));
    if (type == '{') {
      return _cont(
        _pushlex('}'),
        _pushblockcontext,
        _block,
        _poplex,
        _popcontext,
      );
    }
    if (type == ';') return _cont();
    if (type == 'if') {
      if (_state.lexical.info == 'else' &&
          _state._stack.isNotEmpty &&
          identical(_state._stack.last, _poplex)) {
        _state._stack.removeLast().run(null, null);
      }
      return _cont(
        _pushlex('form'),
        _parenExpr,
        _statement,
        _poplex,
        _maybeelse,
      );
    }
    if (type == 'function') return _cont(_functiondef);
    if (type == 'for') {
      return _cont(
        _pushlex('form'),
        _pushblockcontext,
        _forspec,
        _statement,
        _popcontext,
        _poplex,
      );
    }
    if (type == 'class' || (_isTS && value == 'interface')) {
      _marked = 'keyword';
      return _cont(
        _pushlex('form', type == 'class' ? type : value),
        _className,
        _poplex,
      );
    }
    if (type == 'variable') {
      if (_isTS && value == 'declare') {
        _marked = 'keyword';
        return _cont(_statement);
      } else if (_isTS &&
          (value == 'module' || value == 'enum' || value == 'type') &&
          _stream.match(_spaceWord, consume: false) != null) {
        _marked = 'keyword';
        if (value == 'enum') {
          return _cont(_enumdef);
        } else if (value == 'type') {
          return _cont(_typename, _expect('operator'), _typeexpr, _expect(';'));
        } else {
          return _cont(
            _pushlex('form'),
            _pattern,
            _expect('{'),
            _pushlex('}'),
            _block,
            _poplex,
            _poplex,
          );
        }
      } else if (_isTS && value == 'namespace') {
        _marked = 'keyword';
        return _cont(_pushlex('form'), _expression, _statement, _poplex);
      } else if (_isTS && value == 'abstract') {
        _marked = 'keyword';
        return _cont(_statement);
      } else {
        return _cont(_pushlex('stat'), _maybelabel);
      }
    }
    if (type == 'switch') {
      return _cont(
        _pushlex('form'),
        _parenExpr,
        _expect('{'),
        _pushlex('}', 'switch'),
        _pushblockcontext,
        _block,
        _poplex,
        _poplex,
        _popcontext,
      );
    }
    if (type == 'case') return _cont(_expression, _expect(':'));
    if (type == 'default') return _cont(_expect(':'));
    if (type == 'catch') {
      return _cont(
        _pushlex('form'),
        _pushcontext,
        _maybeCatchBinding,
        _statement,
        _poplex,
        _popcontext,
      );
    }
    if (type == 'export') {
      return _cont(_pushlex('stat'), _afterExport, _poplex);
    }
    if (type == 'import') {
      return _cont(_pushlex('stat'), _afterImport, _poplex);
    }
    if (type == 'async') return _cont(_statement);
    if (value == '@') return _cont(_expression, _statement);
    return _pass(_pushlex('stat'), _expression, _expect(';'), _poplex);
  }

  late final _C _maybeCatchBinding = _C((type, _) {
    if (type == '(') return _cont(_funarg, _expect(')'));
    return false;
  });

  late final _C _expression = _C(
    (type, value) => _expressionInner(type, value, false),
  );
  late final _C _expressionNoComma = _C(
    (type, value) => _expressionInner(type, value, true),
  );
  late final _C _parenExpr = _C((type, _) {
    if (type != '(') return _pass();
    return _cont(_pushlex(')'), _maybeexpression, _expect(')'), _poplex);
  });

  bool _expressionInner(String? type, String? value, bool noComma) {
    if (_state.fatArrowAt == _stream.start) {
      final body = noComma ? _arrowBodyNoComma : _arrowBody;
      if (type == '(') {
        return _cont(
          _pushcontext,
          _pushlex(')'),
          _commasep(_funarg, ')'),
          _poplex,
          _expect('=>'),
          body,
          _popcontext,
        );
      } else if (type == 'variable') {
        return _pass(_pushcontext, _pattern, _expect('=>'), body, _popcontext);
      }
    }

    final maybeop = noComma ? _maybeoperatorNoComma : _maybeoperatorComma;
    if (_atomicTypes.contains(type)) return _cont(maybeop);
    if (type == 'function') return _cont(_functiondef, maybeop);
    if (type == 'class' || (_isTS && value == 'interface')) {
      _marked = 'keyword';
      return _cont(_pushlex('form'), _classExpression, _poplex);
    }
    if (type == 'keyword c' || type == 'async') {
      return _cont(noComma ? _expressionNoComma : _expression);
    }
    if (type == '(') {
      return _cont(
        _pushlex(')'),
        _maybeexpression,
        _expect(')'),
        _poplex,
        maybeop,
      );
    }
    if (type == 'operator' || type == 'spread') {
      return _cont(noComma ? _expressionNoComma : _expression);
    }
    if (type == '[') {
      return _cont(_pushlex(']'), _arrayLiteral, _poplex, maybeop);
    }
    if (type == '{') return _contCommasep(_objprop, '}', null, maybeop);
    if (type == 'quasi') return _pass(_quasi, maybeop);
    if (type == 'new') return _cont(_maybeTarget(noComma));
    return _cont();
  }

  late final _C _maybeexpression = _C((type, _) {
    if (_hasAny(type!, _closers)) return _pass();
    return _pass(_expression);
  });

  late final _C _maybeoperatorComma = _C((type, value) {
    if (type == ',') return _cont(_maybeexpression);
    return _maybeoperatorNoCommaF(type, value, false);
  });
  late final _C _maybeoperatorNoComma = _C(
    (type, value) => _maybeoperatorNoCommaF(type, value, null),
  );

  /// [noComma] is null when the step runs from the stack, as upstream's
  /// missing argument.
  bool _maybeoperatorNoCommaF(String? type, String? value, bool? noComma) {
    final me = noComma == false ? _maybeoperatorComma : _maybeoperatorNoComma;
    final expr = noComma == false ? _expression : _expressionNoComma;
    if (type == '=>') {
      return _cont(
        _pushcontext,
        noComma == true ? _arrowBodyNoComma : _arrowBody,
        _popcontext,
      );
    }
    if (type == 'operator') {
      if (_incDec.hasMatch(value ?? 'undefined') || _isTS && value == '!') {
        return _cont(me);
      }
      if (_isTS &&
          value == '<' &&
          _stream.match(_typeArgsCall, consume: false) != null) {
        return _cont(_pushlex('>'), _commasep(_typeexpr, '>'), _poplex, me);
      }
      if (value == '?') return _cont(_expression, _expect(':'), expr);
      return _cont(expr);
    }
    if (type == 'quasi') return _pass(_quasi, me);
    if (type == ';') return false;
    if (type == '(') return _contCommasep(_expressionNoComma, ')', 'call', me);
    if (type == '.') return _cont(_property, me);
    if (type == '[') {
      return _cont(_pushlex(']'), _maybeexpression, _expect(']'), _poplex, me);
    }
    if (_isTS && value == 'as') {
      _marked = 'keyword';
      return _cont(_typeexpr, me);
    }
    if (type == 'regexp') {
      _state.lastType = _marked = 'operator';
      _stream.backUp(_stream.pos - _stream.start - 1);
      return _cont(expr);
    }
    return false;
  }

  late final _C _quasi = _C((type, value) {
    if (type != 'quasi') return _pass();
    if (_lastTwo(value!) != r'${') return _cont(_quasi);
    return _cont(_maybeexpression, _continueQuasi);
  });
  late final _C _continueQuasi = _C((type, _) {
    if (type == '}') {
      _marked = 'string-2';
      _state._tokenize = _tokenQuasiT;
      return _cont(_quasi);
    }
    return false;
  });
  late final _C _arrowBody = _C((type, _) {
    _findFatArrow(_stream, _state);
    return _pass(type == '{' ? _statement : _expression);
  });
  late final _C _arrowBodyNoComma = _C((type, _) {
    _findFatArrow(_stream, _state);
    return _pass(type == '{' ? _statement : _expressionNoComma);
  });
  _C _maybeTarget(bool noComma) => _C((type, _) {
    if (type == '.') {
      return _cont(noComma ? _targetNoComma : _target);
    } else if (type == 'variable' && _isTS) {
      return _cont(
        _maybeTypeArgs,
        noComma ? _maybeoperatorNoComma : _maybeoperatorComma,
      );
    } else {
      return _pass(noComma ? _expressionNoComma : _expression);
    }
  });
  late final _C _target = _C((_, value) {
    if (value == 'target') {
      _marked = 'keyword';
      return _cont(_maybeoperatorComma);
    }
    return false;
  });
  late final _C _targetNoComma = _C((_, value) {
    if (value == 'target') {
      _marked = 'keyword';
      return _cont(_maybeoperatorNoComma);
    }
    return false;
  });
  late final _C _maybelabel = _C((type, _) {
    if (type == ':') return _cont(_poplex, _statement);
    return _pass(_maybeoperatorComma, _expect(';'), _poplex);
  });
  late final _C _property = _C((type, _) {
    if (type == 'variable') {
      _marked = 'property';
      return _cont();
    }
    return false;
  });
  late final _C _objprop = _C((type, value) {
    if (type == 'async') {
      _marked = 'property';
      return _cont(_objprop);
    } else if (type == 'variable' || _style == 'keyword') {
      _marked = 'property';
      if (value == 'get' || value == 'set') return _cont(_getterSetter);
      // Upstream's work-around for typed arrow parameters.
      Match? m;
      if (_isTS &&
          _state.fatArrowAt == _stream.start &&
          (m = _stream.match(_typedArrowParam, consume: false)) != null) {
        _state.fatArrowAt = _stream.pos + m![0]!.length;
      }
      return _cont(_afterprop);
    } else if (type == 'number' || type == 'string') {
      _marked = _jsonldMode ? 'property' : '$_style property';
      return _cont(_afterprop);
    } else if (type == 'jsonld-keyword') {
      return _cont(_afterprop);
    } else if (_isTS && _isModifier(value)) {
      _marked = 'keyword';
      return _cont(_objprop);
    } else if (type == '[') {
      return _cont(_expression, _maybetype, _expect(']'), _afterprop);
    } else if (type == 'spread') {
      return _cont(_expressionNoComma, _afterprop);
    } else if (value == '*') {
      _marked = 'keyword';
      return _cont(_objprop);
    } else if (type == ':') {
      return _pass(_afterprop);
    }
    return false;
  });
  late final _C _getterSetter = _C((type, _) {
    if (type != 'variable') return _pass(_afterprop);
    _marked = 'property';
    return _cont(_functiondef);
  });
  late final _C _afterprop = _C((type, _) {
    if (type == ':') return _cont(_expressionNoComma);
    if (type == '(') return _pass(_functiondef);
    return false;
  });

  _C _commasep(_C what, String end, [String? sep]) {
    late final _C proceed;
    proceed = _C((type, value) {
      if (sep != null ? sep.contains(type ?? 'undefined') : type == ',') {
        final lex = _state.lexical;
        if (lex.info == 'call') lex.pos = (lex.pos ?? 0) + 1;
        return _cont(
          _C((type, value) {
            if (type == end || value == end) return _pass();
            return _pass(what);
          }),
          proceed,
        );
      }
      if (type == end || value == end) return _cont();
      if (sep != null && sep.contains(';')) return _pass(what);
      return _cont(_expect(end));
    });
    return _C((type, value) {
      if (type == end || value == end) return _cont();
      return _pass(what, proceed);
    });
  }

  bool _contCommasep(_C what, String end, [String? info, _C? after]) {
    if (after != null) _cc.add(after);
    return _cont(_pushlex(end, info), _commasep(what, end), _poplex);
  }

  late final _C _block = _C((type, _) {
    if (type == '}') return _cont();
    return _pass(_statement, _block);
  });
  late final _C _maybetype = _C((type, value) {
    if (_isTS) {
      if (type == ':') return _cont(_typeexpr);
      if (value == '?') return _cont(_maybetype);
    }
    return false;
  });
  late final _C _maybetypeOrIn = _C((type, value) {
    if (_isTS && (type == ':' || value == 'in')) return _cont(_typeexpr);
    return false;
  });
  late final _C _mayberettype = _C((type, _) {
    if (_isTS && type == ':') {
      if (_stream.match(_typePredicate, consume: false) != null) {
        return _cont(_expression, _isKW, _typeexpr);
      } else {
        return _cont(_typeexpr);
      }
    }
    return false;
  });
  late final _C _isKW = _C((_, value) {
    if (value == 'is') {
      _marked = 'keyword';
      return _cont();
    }
    return false;
  });
  late final _C _typeexpr = _C((type, value) {
    if (value == 'keyof' ||
        value == 'typeof' ||
        value == 'infer' ||
        value == 'readonly') {
      _marked = 'keyword';
      return _cont(value == 'typeof' ? _expressionNoComma : _typeexpr);
    }
    if (type == 'variable' || value == 'void') {
      _marked = 'type';
      return _cont(_afterType);
    }
    if (value == '|' || value == '&') return _cont(_typeexpr);
    if (type == 'string' || type == 'number' || type == 'atom') {
      return _cont(_afterType);
    }
    if (type == '[') {
      return _cont(
        _pushlex(']'),
        _commasep(_typeexpr, ']', ','),
        _poplex,
        _afterType,
      );
    }
    if (type == '{') {
      return _cont(_pushlex('}'), _typeprops, _poplex, _afterType);
    }
    if (type == '(') {
      return _cont(_commasep(_typearg, ')'), _maybeReturnType, _afterType);
    }
    if (type == '<') return _cont(_commasep(_typeexpr, '>'), _typeexpr);
    if (type == 'quasi') return _pass(_quasiType, _afterType);
    return false;
  });
  late final _C _maybeReturnType = _C((type, _) {
    if (type == '=>') return _cont(_typeexpr);
    return false;
  });
  late final _C _typeprops = _C((type, _) {
    if (_hasAny(type!, _typeClosers)) return _cont();
    if (type == ',' || type == ';') return _cont(_typeprops);
    return _pass(_typeprop, _typeprops);
  });
  late final _C _typeprop = _C((type, value) {
    if (type == 'variable' || _style == 'keyword') {
      _marked = 'property';
      return _cont(_typeprop);
    } else if (value == '?' || type == 'number' || type == 'string') {
      return _cont(_typeprop);
    } else if (type == ':') {
      return _cont(_typeexpr);
    } else if (type == '[') {
      return _cont(
        _expect('variable'),
        _maybetypeOrIn,
        _expect(']'),
        _typeprop,
      );
    } else if (type == '(') {
      return _pass(_functiondecl, _typeprop);
    } else if (!_hasAny(type!, _closers)) {
      return _cont();
    }
    return false;
  });
  late final _C _quasiType = _C((type, value) {
    if (type != 'quasi') return _pass();
    if (_lastTwo(value!) != r'${') return _cont(_quasiType);
    return _cont(_typeexpr, _continueQuasiType);
  });
  late final _C _continueQuasiType = _C((type, _) {
    if (type == '}') {
      _marked = 'string-2';
      _state._tokenize = _tokenQuasiT;
      return _cont(_quasiType);
    }
    return false;
  });
  late final _C _typearg = _C((type, value) {
    if (type == 'variable' &&
            _stream.match(_optionalOrTyped, consume: false) != null ||
        value == '?') {
      return _cont(_typearg);
    }
    if (type == ':') return _cont(_typeexpr);
    if (type == 'spread') return _cont(_typearg);
    return _pass(_typeexpr);
  });
  late final _C _afterType = _C((type, value) {
    if (value == '<') {
      return _cont(
        _pushlex('>'),
        _commasep(_typeexpr, '>'),
        _poplex,
        _afterType,
      );
    }
    if (value == '|' || type == '.' || value == '&') return _cont(_typeexpr);
    if (type == '[') return _cont(_typeexpr, _expect(']'), _afterType);
    if (value == 'extends' || value == 'implements') {
      _marked = 'keyword';
      return _cont(_typeexpr);
    }
    if (value == '?') return _cont(_typeexpr, _expect(':'), _typeexpr);
    return false;
  });
  late final _C _maybeTypeArgs = _C((_, value) {
    if (value == '<') {
      return _cont(
        _pushlex('>'),
        _commasep(_typeexpr, '>'),
        _poplex,
        _afterType,
      );
    }
    return false;
  });
  late final _C _typeparam = _C((_, _) => _pass(_typeexpr, _maybeTypeDefault));
  late final _C _maybeTypeDefault = _C((_, value) {
    if (value == '=') return _cont(_typeexpr);
    return false;
  });
  late final _C _vardef = _C((_, value) {
    if (value == 'enum') {
      _marked = 'keyword';
      return _cont(_enumdef);
    }
    return _pass(_pattern, _maybetype, _maybeAssign, _vardefCont);
  });
  late final _C _pattern = _C((type, value) {
    if (_isTS && _isModifier(value)) {
      _marked = 'keyword';
      return _cont(_pattern);
    }
    if (type == 'variable') {
      _register(value);
      return _cont();
    }
    if (type == 'spread') return _cont(_pattern);
    if (type == '[') return _contCommasep(_eltpattern, ']');
    if (type == '{') return _contCommasep(_proppattern, '}');
    return false;
  });
  late final _C _proppattern = _C((type, value) {
    if (type == 'variable' &&
        _stream.match(_spaceColon, consume: false) == null) {
      _register(value);
      return _cont(_maybeAssign);
    }
    if (type == 'variable') _marked = 'property';
    if (type == 'spread') return _cont(_pattern);
    if (type == '}') return _pass();
    if (type == '[') {
      return _cont(_expression, _expect(']'), _expect(':'), _proppattern);
    }
    return _cont(_expect(':'), _pattern, _maybeAssign);
  });
  late final _C _eltpattern = _C((_, _) => _pass(_pattern, _maybeAssign));
  late final _C _maybeAssign = _C((_, value) {
    if (value == '=') return _cont(_expressionNoComma);
    return false;
  });
  late final _C _vardefCont = _C((type, _) {
    if (type == ',') return _cont(_vardef);
    return false;
  });
  late final _C _maybeelse = _C((type, value) {
    if (type == 'keyword b' && value == 'else') {
      return _cont(_pushlex('form', 'else'), _statement, _poplex);
    }
    return false;
  });
  late final _C _forspec = _C((type, value) {
    if (value == 'await') return _cont(_forspec);
    if (type == '(') return _cont(_pushlex(')'), _forspec1, _poplex);
    return false;
  });
  late final _C _forspec1 = _C((type, _) {
    if (type == 'var') return _cont(_vardef, _forspec2);
    if (type == 'variable') return _cont(_forspec2);
    return _pass(_forspec2);
  });
  late final _C _forspec2 = _C((type, value) {
    if (type == ')') return _cont();
    if (type == ';') return _cont(_forspec2);
    if (value == 'in' || value == 'of') {
      _marked = 'keyword';
      return _cont(_expression, _forspec2);
    }
    return _pass(_expression, _forspec2);
  });
  late final _C _functiondef = _C((type, value) {
    if (value == '*') {
      _marked = 'keyword';
      return _cont(_functiondef);
    }
    if (type == 'variable') {
      _register(value);
      return _cont(_functiondef);
    }
    if (type == '(') {
      return _cont(
        _pushcontext,
        _pushlex(')'),
        _commasep(_funarg, ')'),
        _poplex,
        _mayberettype,
        _statement,
        _popcontext,
      );
    }
    if (_isTS && value == '<') {
      return _cont(
        _pushlex('>'),
        _commasep(_typeparam, '>'),
        _poplex,
        _functiondef,
      );
    }
    return false;
  });
  late final _C _functiondecl = _C((type, value) {
    if (value == '*') {
      _marked = 'keyword';
      return _cont(_functiondecl);
    }
    if (type == 'variable') {
      _register(value);
      return _cont(_functiondecl);
    }
    if (type == '(') {
      return _cont(
        _pushcontext,
        _pushlex(')'),
        _commasep(_funarg, ')'),
        _poplex,
        _mayberettype,
        _popcontext,
      );
    }
    if (_isTS && value == '<') {
      return _cont(
        _pushlex('>'),
        _commasep(_typeparam, '>'),
        _poplex,
        _functiondecl,
      );
    }
    return false;
  });
  late final _C _typename = _C((type, value) {
    if (type == 'keyword' || type == 'variable') {
      _marked = 'type';
      return _cont(_typename);
    } else if (value == '<') {
      return _cont(_pushlex('>'), _commasep(_typeparam, '>'), _poplex);
    }
    return false;
  });
  late final _C _funarg = _C((type, value) {
    // Upstream pushes without returning here.
    if (value == '@') {
      _cont(_expression, _funarg);
    }
    if (type == 'spread') return _cont(_funarg);
    if (_isTS && _isModifier(value)) {
      _marked = 'keyword';
      return _cont(_funarg);
    }
    if (_isTS && type == 'this') return _cont(_maybetype, _maybeAssign);
    return _pass(_pattern, _maybetype, _maybeAssign);
  });
  late final _C _classExpression = _C((type, value) {
    // Class expressions may have an optional name.
    if (type == 'variable') return _className.run(type, value);
    return _classNameAfter.run(type, value);
  });
  late final _C _className = _C((type, value) {
    if (type == 'variable') {
      _register(value);
      return _cont(_classNameAfter);
    }
    return false;
  });
  late final _C _classNameAfter = _C((type, value) {
    if (value == '<') {
      return _cont(
        _pushlex('>'),
        _commasep(_typeparam, '>'),
        _poplex,
        _classNameAfter,
      );
    }
    if (value == 'extends' || value == 'implements' || (_isTS && type == ',')) {
      if (value == 'implements') _marked = 'keyword';
      return _cont(_isTS ? _typeexpr : _expression, _classNameAfter);
    }
    if (type == '{') return _cont(_pushlex('}'), _classBody, _poplex);
    return false;
  });
  late final _C _classBody = _C((type, value) {
    if (type == 'async' ||
        (type == 'variable' &&
            (value == 'static' ||
                value == 'get' ||
                value == 'set' ||
                (_isTS && _isModifier(value))) &&
            _stream.match(_classModifierAhead, consume: false) != null)) {
      _marked = 'keyword';
      return _cont(_classBody);
    }
    if (type == 'variable' || _style == 'keyword') {
      _marked = 'property';
      return _cont(_classfield, _classBody);
    }
    if (type == 'number' || type == 'string') {
      return _cont(_classfield, _classBody);
    }
    if (type == '[') {
      return _cont(
        _expression,
        _maybetype,
        _expect(']'),
        _classfield,
        _classBody,
      );
    }
    if (value == '*') {
      _marked = 'keyword';
      return _cont(_classBody);
    }
    if (_isTS && type == '(') return _pass(_functiondecl, _classBody);
    if (type == ';' || type == ',') return _cont(_classBody);
    if (type == '}') return _cont();
    if (value == '@') return _cont(_expression, _classBody);
    return false;
  });
  late final _C _classfield = _C((type, value) {
    if (value == '!') return _cont(_classfield);
    if (value == '?') return _cont(_classfield);
    if (type == ':') return _cont(_typeexpr, _maybeAssign);
    if (value == '=') return _cont(_expressionNoComma);
    final context = _state.lexical.prev;
    final isInterface = context != null && context.info == 'interface';
    return _pass(isInterface ? _functiondecl : _functiondef);
  });
  late final _C _afterExport = _C((type, value) {
    if (value == '*') {
      _marked = 'keyword';
      return _cont(_maybeFrom, _expect(';'));
    }
    if (value == 'default') {
      _marked = 'keyword';
      return _cont(_expression, _expect(';'));
    }
    if (type == '{') {
      return _cont(_commasep(_exportField, '}'), _maybeFrom, _expect(';'));
    }
    return _pass(_statement);
  });
  late final _C _exportField = _C((type, value) {
    if (value == 'as') {
      _marked = 'keyword';
      return _cont(_expect('variable'));
    }
    if (type == 'variable') return _pass(_expressionNoComma, _exportField);
    return false;
  });
  late final _C _afterImport = _C((type, _) {
    if (type == 'string') return _cont();
    if (type == '(') return _pass(_expression);
    if (type == '.') return _pass(_maybeoperatorComma);
    return _pass(_importSpec, _maybeMoreImports, _maybeFrom);
  });
  late final _C _importSpec = _C((type, value) {
    if (type == '{') return _contCommasep(_importSpec, '}');
    if (type == 'variable') _register(value);
    if (value == '*') _marked = 'keyword';
    return _cont(_maybeAs);
  });
  late final _C _maybeMoreImports = _C((type, _) {
    if (type == ',') return _cont(_importSpec, _maybeMoreImports);
    return false;
  });
  late final _C _maybeAs = _C((_, value) {
    if (value == 'as') {
      _marked = 'keyword';
      return _cont(_importSpec);
    }
    return false;
  });
  late final _C _maybeFrom = _C((_, value) {
    if (value == 'from') {
      _marked = 'keyword';
      return _cont(_expression);
    }
    return false;
  });
  late final _C _arrayLiteral = _C((type, _) {
    if (type == ']') return _cont();
    return _pass(_commasep(_expressionNoComma, ']'));
  });
  late final _C _enumdef = _C(
    (_, _) => _pass(
      _pushlex('form'),
      _pattern,
      _expect('{'),
      _pushlex('}'),
      _commasep(_enummember, '}'),
      _poplex,
      _poplex,
    ),
  );
  late final _C _enummember = _C((_, _) => _pass(_pattern, _maybeAssign));

  bool _isContinuedStatement(JsState state, String textAfter) {
    final first = textAfter.isEmpty ? '' : textAfter[0];
    return state.lastType == 'operator' ||
        state.lastType == ',' ||
        _isOperatorChar(first) ||
        first.isNotEmpty && ',.'.contains(first);
  }

  /// Whether a regular expression may start here, as upstream's
  /// `expressionAllowed`.
  bool expressionAllowed(
    StringStream stream,
    JsState state, [
    int backUp = 0,
  ]) =>
      identical(state._tokenize, _tokenBaseT) &&
          _expressionAfter.hasMatch(state.lastType ?? 'undefined') ||
      (state.lastType == 'quasi' &&
          _openBraceEnd.hasMatch(
            stream.string.substring(0, stream.pos - backUp),
          ));

  // Interface

  @override
  JsState startState([int baseColumn = 0]) => JsState._(
    _tokenBaseT,
    'sof',
    [],
    JsLexical(baseColumn - _indentUnit, 0, 'block', false, null, null),
    null,
    null,
    baseColumn,
    null,
  );

  @override
  JsState copyState(JsState state) => state.copy();

  @override
  String? token(StringStream stream, JsState state) {
    if (stream.sol()) {
      state.lexical.align ??= false;
      state.indented = stream.indentation();
      _findFatArrow(stream, state);
    }
    if (!identical(state._tokenize, _tokenCommentT) && stream.eatSpace()) {
      return null;
    }
    final style = state._tokenize(stream, state);
    if (_type == 'comment') return style;
    state.lastType =
        _type == 'operator' && (_content == '++' || _content == '--')
        ? 'incdec'
        : _type;
    return _parseJS(state, style, _type, _content, stream);
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(JsState state, String textAfter, String line) {
    if (identical(state._tokenize, _tokenCommentT) ||
        identical(state._tokenize, _tokenQuasiT)) {
      return null;
    }
    if (!identical(state._tokenize, _tokenBaseT)) return 0;
    final firstChar = textAfter.isEmpty ? '' : textAfter[0];
    var lexical = state.lexical;
    // Upstream's kludge to keep `maybeelse` from blocking lexical pops.
    if (!_elseAhead.hasMatch(textAfter)) {
      for (var i = state._stack.length - 1; i >= 0; --i) {
        final c = state._stack[i];
        if (identical(c, _poplex)) {
          lexical = lexical.prev!;
        } else if (!identical(c, _maybeelse) && !identical(c, _popcontext)) {
          break;
        }
      }
    }
    while ((lexical.type == 'stat' || lexical.type == 'form') &&
        (firstChar == '}' ||
            (state._stack.isNotEmpty &&
                (identical(state._stack.last, _maybeoperatorComma) ||
                    identical(state._stack.last, _maybeoperatorNoComma)) &&
                !_continuation.hasMatch(textAfter)))) {
      lexical = lexical.prev!;
    }
    // Upstream tests the option for truth, so 0 means unset.
    final statementIndent = options.statementIndent == 0
        ? null
        : options.statementIndent;
    if (statementIndent != null &&
        lexical.type == ')' &&
        lexical.prev!.type == 'stat') {
      lexical = lexical.prev!;
    }
    final type = lexical.type, closing = firstChar == type;

    if (type == 'vardef') {
      return lexical.indented +
          (state.lastType == 'operator' || state.lastType == ','
              ? lexical.info!.length + 1
              : 0);
    } else if (type == 'form' && firstChar == '{') {
      return lexical.indented;
    } else if (type == 'form') {
      return lexical.indented + _indentUnit;
    } else if (type == 'stat') {
      return lexical.indented +
          (_isContinuedStatement(state, textAfter)
              ? (statementIndent ?? _indentUnit)
              : 0);
    } else if (lexical.info == 'switch' &&
        !closing &&
        options.doubleIndentSwitch) {
      return lexical.indented +
          (_caseOrDefault.hasMatch(textAfter) ? _indentUnit : 2 * _indentUnit);
    } else if (lexical.align == true) {
      return lexical.column + (closing ? 0 : 1);
    } else {
      return lexical.indented + (closing ? 0 : _indentUnit);
    }
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s*(?:case .*?:|default:|\{|\})$');
}
