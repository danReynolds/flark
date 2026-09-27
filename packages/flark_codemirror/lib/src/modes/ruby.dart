// Ported from @codemirror/legacy-modes 6.5.4 mode/ruby.js, its `ruby`
// export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

Set<String> _wordObj(String words) => words.split(' ').toSet();

final _keywords = _wordObj(
  'alias and BEGIN begin break case class def defined? do else elsif END end '
  'ensure false for if in module next not or redo rescue retry return self '
  'super then true undef unless until when while yield nil raise throw catch '
  'fail loop callcc caller lambda proc public protected private require load '
  'require_relative extend autoload __END__ __FILE__ __LINE__ __dir__',
);
const _indentWords = {
  'def', 'class', 'case', 'for', 'while', 'until', 'module', 'catch', //
  'loop', 'proc', 'begin',
};
const _dedentWords = {'end', 'until'};
const _opening = {'[': ']', '{': '}', '(': ')'};
const _closing = {']': '[', '}': '{', ')': '('};

final _heredoc = RegExp(r'''<([-~])[\`\"\']?([a-zA-Z_?]\w*)[\`\"\']?(?:;|$)''');
final _wq = RegExp('[WQ]');
final _r = RegExp('[r]');
final _wxq = RegExp('[wxq]');
final _delim = RegExp(r'[^\w\s=]');
final _hexDigit = RegExp(r'[\da-fA-F]');
final _binaryDigit = RegExp('[01]');
// Matches the empty string, so it eats any character.
final _fractionTail = RegExp(r'[\d_]*(?:[eE][+\-]?[\d_]+)?');
final _octalDigit = RegExp('[0-7]');
final _digit = RegExp(r'\d');
final _number = RegExp(r'[\d_]*(?:\.[\d_]+)?(?:[eE][+\-]?[\d_]+)?');
final _control = RegExp(r'\\[CM]-');
final _word = RegExp(r'\w');
final _angle = RegExp(r'[\<\>]');
final _symbolOperator = RegExp(r'[\+\-\*\/\&\|\:\!]');
final _symbolStart = RegExp(r'[a-zA-Z$@_\xa1-\uffff]');
final _symbolChar = RegExp(r'[\w$\xa1-\uffff]');
final _symbolEnd = RegExp(r'[\?\!\=]');
final _instanceVariable = RegExp(r'@?[a-zA-Z_\xa1-\uffff]');
final _identifierChar = RegExp(r'[\w\xa1-\uffff]');
final _globalStart = RegExp('[a-zA-Z_]');
final _identifierStart = RegExp(r'[a-zA-Z_\xa1-\uffff]');
final _predicate = RegExp(r'[\?\!]');
final _punctuation = RegExp(r'[\(\)\[\]{}\\;]');
final _operatorChar = RegExp(r'[=+\-\/*:\.^%<>~|]');
final _capital = RegExp('^[A-Z]');
final _openBracket = RegExp(r'[\(\[\{]');
final _closeBracket = RegExp(r'[\)\]\}]');
final _closingWord = RegExp(r'^(?:end|until|else|elsif|when|rescue)\b');

typedef _Tokenizer = String? Function(StringStream stream, RubyState state);

final class _Context {
  const _Context(this.prev, this.type, this.indented);
  final _Context? prev;
  final String type;

  /// Null where upstream leaves it undefined, in a paused quote's context.
  final int? indented;
}

final class RubyState {
  RubyState._(
    this._tokenize,
    this._indented,
    this._context,
    this._continuedLine,
    this._lastTok,
    this._varList,
  );

  /// Upstream's tokenizer stack, innermost last.
  final List<_Tokenizer> _tokenize;
  int _indented;
  _Context _context;
  bool _continuedLine;
  String? _lastTok;
  bool _varList;

  /// The upstream default copy: the tokenizer stack is copied, its
  /// tokenizers shared.
  RubyState copy() => RubyState._(
    List.of(_tokenize),
    _indented,
    _context,
    _continuedLine,
    _lastTok,
    _varList,
  );
}

bool _regexpAhead(StringStream stream) {
  final start = stream.pos;
  var depth = 0, found = false, escaped = false;
  for (var next = stream.next(); next != null; next = stream.next()) {
    if (!escaped) {
      if ('[{('.contains(next)) {
        depth++;
      } else if (']})'.contains(next)) {
        depth--;
        if (depth < 0) break;
      } else if (next == '/' && depth == 0) {
        found = true;
        break;
      }
      escaped = next == r'\';
    } else {
      escaped = false;
    }
  }
  stream.backUp(stream.pos - start);
  return found;
}

/// Ruby: the upstream `ruby` stream parser.
final class RubyMode extends Mode<RubyState> {
  RubyMode([super.config = const ModeConfig()]);

  /// Upstream's module-level `curPunc`: the punctuation just read.
  String? _curPunc;

  late final _Tokenizer _tokenBaseT = _tokenBase;
  late final _Tokenizer _readBlockCommentT = _readBlockComment;

  String? _chain(_Tokenizer newtok, StringStream stream, RubyState state) {
    state._tokenize.add(newtok);
    return newtok(stream, state);
  }

  // Upstream's chain of `else if` branches, each of which returns.
  String? _tokenBase(StringStream stream, RubyState state) {
    if (stream.sol() && stream.matchString('=begin') && stream.eol()) {
      state._tokenize.add(_readBlockCommentT);
      return 'comment';
    }
    if (stream.eatSpace()) return null;
    final ch = stream.next()!;
    if (ch == '`' || ch == "'" || ch == '"') {
      return _chain(
        _readQuoted(ch, 'string', ch == '"' || ch == '`'),
        stream,
        state,
      );
    }
    if (ch == '/') {
      if (_regexpAhead(stream)) {
        return _chain(_readQuoted(ch, 'string.special', true), stream, state);
      }
      return 'operator';
    }
    if (ch == '%') {
      var style = 'string', embed = true;
      if (stream.eat('s') != null) {
        style = 'atom';
      } else if (stream.eat(_wq) != null) {
        style = 'string';
      } else if (stream.eat(_r) != null) {
        style = 'string.special';
      } else if (stream.eat(_wxq) != null) {
        style = 'string';
        embed = false;
      }
      final delim = stream.eat(_delim);
      if (delim == null) return 'operator';
      return _chain(
        _readQuoted(_opening[delim] ?? delim, style, embed, true),
        stream,
        state,
      );
    }
    if (ch == '#') {
      stream.skipToEnd();
      return 'comment';
    }
    if (ch == '<') {
      final m = stream.match(_heredoc);
      if (m != null) return _chain(_readHereDoc(m[2]!, m[1]!), stream, state);
    }
    if (ch == '0') {
      if (stream.eat('x') != null) {
        stream.eatWhile(_hexDigit);
      } else if (stream.eat('b') != null) {
        stream.eatWhile(_binaryDigit);
      } else if (stream.eat('.') != null) {
        stream.eat(_fractionTail);
      } else {
        stream.eatWhile(_octalDigit);
      }
      return 'number';
    }
    if (_digit.hasMatch(ch)) {
      stream.match(_number);
      return 'number';
    }
    if (ch == '?') {
      while (stream.match(_control) != null) {}
      if (stream.eat(r'\') != null) {
        stream.eatWhile(_word);
      } else {
        stream.next();
      }
      return 'string';
    }
    if (ch == ':') {
      if (stream.eat("'") != null) {
        return _chain(_readQuoted("'", 'atom', false), stream, state);
      }
      if (stream.eat('"') != null) {
        return _chain(_readQuoted('"', 'atom', true), stream, state);
      }

      // :> :>> :< :<< are valid symbols
      if (stream.eat(_angle) != null) {
        stream.eat(_angle);
        return 'atom';
      }

      // :+ :- :/ :* :| :& :! are valid symbols
      if (stream.eat(_symbolOperator) != null) {
        return 'atom';
      }

      // Symbols can't start by a digit
      if (stream.eat(_symbolStart) != null) {
        stream.eatWhile(_symbolChar);
        // Only one ? ! = is allowed and only as the last character
        stream.eat(_symbolEnd);
        return 'atom';
      }
      return 'operator';
    }
    if (ch == '@' && stream.match(_instanceVariable) != null) {
      stream.eat('@');
      stream.eatWhile(_identifierChar);
      return 'propertyName';
    }
    if (ch == r'$') {
      if (stream.eat(_globalStart) != null) {
        stream.eatWhile(_word);
      } else if (stream.eat(_digit) != null) {
        stream.eat(_digit);
      } else {
        stream.next(); // Must be a special global like $: or $!
      }
      return 'variableName.special';
    }
    if (_identifierStart.hasMatch(ch)) {
      stream.eatWhile(_identifierChar);
      stream.eat(_predicate);
      if (stream.eat(':') != null) return 'atom';
      return 'variable';
    }
    if (ch == '|' &&
        (state._varList || state._lastTok == '{' || state._lastTok == 'do')) {
      _curPunc = '|';
      return null;
    }
    if (_punctuation.hasMatch(ch)) {
      _curPunc = ch;
      return null;
    }
    if (ch == '-' && stream.eat('>') != null) {
      return 'operator';
    }
    if (_operatorChar.hasMatch(ch)) {
      final more = stream.eatWhile(_operatorChar);
      if (ch == '.' && !more) _curPunc = '.';
      return 'operator';
    }
    return null;
  }

  _Tokenizer _tokenBaseUntilBrace([int depth = 1]) => (stream, state) {
    if (stream.peek() == '}') {
      if (depth == 1) {
        state._tokenize.removeLast();
        return state._tokenize.last(stream, state);
      } else {
        state._tokenize[state._tokenize.length - 1] = _tokenBaseUntilBrace(
          depth - 1,
        );
      }
    } else if (stream.peek() == '{') {
      state._tokenize[state._tokenize.length - 1] = _tokenBaseUntilBrace(
        depth + 1,
      );
    }
    return _tokenBase(stream, state);
  };

  _Tokenizer _tokenBaseOnce() {
    var alreadyCalled = false;
    return (stream, state) {
      if (alreadyCalled) {
        state._tokenize.removeLast();
        return state._tokenize.last(stream, state);
      }
      alreadyCalled = true;
      return _tokenBase(stream, state);
    };
  }

  _Tokenizer _readQuoted(
    String quote,
    String style,
    bool embed, [
    bool unescaped = false,
  ]) => (stream, state) {
    var escaped = false;

    if (state._context.type == 'read-quoted-paused') {
      state._context = state._context.prev!;
      stream.eat('}');
    }

    for (var ch = stream.next(); ch != null; ch = stream.next()) {
      if (ch == quote && (unescaped || !escaped)) {
        state._tokenize.removeLast();
        break;
      }
      if (embed && ch == '#' && !escaped) {
        if (stream.eat('{') != null) {
          if (quote == '}') {
            state._context = _Context(
              state._context,
              'read-quoted-paused',
              null,
            );
          }
          state._tokenize.add(_tokenBaseUntilBrace());
          break;
        } else if (stream.peek() case '@' || r'$') {
          state._tokenize.add(_tokenBaseOnce());
          break;
        }
      }
      escaped = !escaped && ch == r'\';
    }
    return style;
  };

  _Tokenizer _readHereDoc(String phrase, String mayIndent) => (stream, state) {
    if (mayIndent.isNotEmpty) stream.eatSpace();
    if (stream.matchString(phrase)) {
      state._tokenize.removeLast();
    } else {
      stream.skipToEnd();
    }
    return 'string';
  };

  String? _readBlockComment(StringStream stream, RubyState state) {
    if (stream.sol() && stream.matchString('=end') && stream.eol()) {
      state._tokenize.removeLast();
    }
    stream.skipToEnd();
    return 'comment';
  }

  @override
  RubyState startState([int baseColumn = 0]) => RubyState._(
    [_tokenBaseT],
    0,
    _Context(null, 'top', -config.indentUnit),
    false,
    null,
    false,
  );

  @override
  RubyState copyState(RubyState state) => state.copy();

  @override
  String? token(StringStream stream, RubyState state) {
    _curPunc = null;
    if (stream.sol()) state._indented = stream.indentation();
    var style = state._tokenize.last(stream, state);
    String? kwtype;
    final curPunc = _curPunc;
    var thisTok = curPunc;
    if (style == 'variable') {
      final word = stream.current();
      style = state._lastTok == '.'
          ? 'property'
          : _keywords.contains(stream.current())
          ? 'keyword'
          : _capital.hasMatch(word)
          ? 'tag'
          : (state._lastTok == 'def' ||
                state._lastTok == 'class' ||
                state._varList)
          ? 'def'
          : 'variable';
      if (style == 'keyword') {
        thisTok = word;
        final contextIndented = state._context.indented;
        if (_indentWords.contains(word)) {
          kwtype = 'indent';
        } else if (_dedentWords.contains(word)) {
          kwtype = 'dedent';
        } else if ((word == 'if' || word == 'unless') &&
            stream.column() == stream.indentation()) {
          kwtype = 'indent';
        } else if (word == 'do' &&
            contextIndented != null &&
            contextIndented < state._indented) {
          kwtype = 'indent';
        }
      }
    }
    if (curPunc != null ||
        (style != null && style.isNotEmpty && style != 'comment')) {
      state._lastTok = thisTok;
    }
    if (curPunc == '|') state._varList = !state._varList;

    if (kwtype == 'indent' ||
        (curPunc != null && _openBracket.hasMatch(curPunc))) {
      state._context = _Context(
        state._context,
        curPunc ?? style!,
        state._indented,
      );
    } else if ((kwtype == 'dedent' ||
            (curPunc != null && _closeBracket.hasMatch(curPunc))) &&
        state._context.prev != null) {
      state._context = state._context.prev!;
    }

    if (stream.eol()) {
      state._continuedLine = curPunc == r'\' || style == 'operator';
    }
    return style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(RubyState state, String textAfter, String line) {
    if (!identical(state._tokenize.last, _tokenBaseT)) return null;
    final firstChar = textAfter.isEmpty ? '' : textAfter[0];
    final ct = state._context;
    final closed =
        ct.type == _closing[firstChar] ||
        ct.type == 'keyword' && _closingWord.hasMatch(textAfter);
    final indented = ct.indented;
    // Upstream adds to undefined here and answers NaN.
    if (indented == null) return null;
    return indented +
        (closed ? 0 : config.indentUnit) +
        (state._continuedLine ? config.indentUnit : 0);
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s*(?:end|rescue|elsif|else|\})$');
}
