// Ported from @codemirror/legacy-modes 6.5.4 mode/shell.js, its `shell`
// export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

const _commonAtoms = ['true', 'false'];
const _commonKeywords = [
  'if', 'then', 'do', 'else', 'elif', 'while', 'until', 'for', 'in', //
  'esac', 'fi', 'fin', 'fil', 'done', 'exit', 'set', 'unset', 'export',
  'function',
];
final _commonCommands =
    ('ab awk bash beep cat cc cd chown chmod chroot clear cp curl cut diff '
            'echo find gawk gcc get git grep hg kill killall ln ls make mkdir '
            'openssl mv nc nl node npm ping ps restart rm rmdir sed service sh '
            'shopt shred source sort sleep ssh start stop su sudo svn tee '
            'telnet top touch vi vim wall wc wget who write yes zsh')
        .split(' ');

/// Each word's style; later definitions win, as upstream's `define` calls.
final _words = {
  for (final word in _commonAtoms) word: 'atom',
  for (final word in _commonKeywords) word: 'keyword',
  for (final word in _commonCommands) word: 'builtin',
};

final _digit = RegExp(r'\d');
final _word = RegExp(r'\w');
final _wordOrDash = RegExp(r'[\w-]');
final _anyWord = RegExp(r'\w+');
final _heredoc = RegExp(r'''<-?\s*(?:['"]([^'"]*)['"]|([^'"\s]*))''');
final _dollarOpen = RegExp(r'''['"({]''');

bool _isQuote(String ch) => ch == "'" || ch == '"';

typedef _Tokenizer = String? Function(StringStream stream, ShellState state);

final class ShellState {
  ShellState._(this._tokens);

  /// Upstream's `tokens`: pending tokenizers, innermost first.
  final List<_Tokenizer> _tokens;

  /// The upstream default copy: the tokenizer list is copied.
  ShellState copy() => ShellState._(List.of(_tokens));
}

/// Shell: the upstream `shell` stream parser.
final class ShellMode extends Mode<ShellState> {
  ShellMode([super.config = const ModeConfig()]);

  late final _Tokenizer _tokenBaseT = _tokenBase;
  late final _Tokenizer _tokenDollarT = _tokenDollar;

  String? _tokenBase(StringStream stream, ShellState state) {
    if (stream.eatSpace()) return null;

    final sol = stream.sol();
    final ch = stream.next()!;

    if (ch == r'\') {
      stream.next();
      return null;
    }
    if (ch == "'" || ch == '"' || ch == '`') {
      state._tokens.insert(0, _tokenString(ch, ch == '`' ? 'quote' : 'string'));
      return _tokenize(stream, state);
    }
    if (ch == '#') {
      if (sol && stream.eat('!') != null) {
        stream.skipToEnd();
        return 'meta'; // 'comment'?
      }
      stream.skipToEnd();
      return 'comment';
    }
    if (ch == r'$') {
      state._tokens.insert(0, _tokenDollarT);
      return _tokenize(stream, state);
    }
    if (ch == '+' || ch == '=') {
      return 'operator';
    }
    if (ch == '-') {
      stream.eat('-');
      stream.eatWhile(_word);
      return 'attribute';
    }
    if (ch == '<') {
      if (stream.matchString('<<')) return 'operator';
      final heredoc = stream.match(_heredoc);
      if (heredoc != null) {
        // `heredoc[1] || heredoc[2]`: an empty quoted name falls through.
        final quoted = heredoc[1];
        state._tokens.insert(
          0,
          _tokenHeredoc(
            quoted != null && quoted.isNotEmpty ? quoted : heredoc[2],
          ),
        );
        return 'string.special';
      }
    }
    if (_digit.hasMatch(ch)) {
      stream.eatWhile(_digit);
      if (stream.eol() || !_word.hasMatch(stream.peek()!)) {
        return 'number';
      }
    }
    stream.eatWhile(_wordOrDash);
    final cur = stream.current();
    if (stream.peek() == '=' && _anyWord.hasMatch(cur)) return 'def';
    return _words[cur];
  }

  _Tokenizer _tokenString(String quote, String style) {
    final close = quote == '('
        ? ')'
        : quote == '{'
        ? '}'
        : quote;
    return (stream, state) {
      var escaped = false;
      for (var next = stream.next(); next != null; next = stream.next()) {
        if (next == close && !escaped) {
          state._tokens.removeAt(0);
          break;
        } else if (next == r'$' &&
            !escaped &&
            quote != "'" &&
            stream.peek() != close) {
          escaped = true;
          stream.backUp(1);
          state._tokens.insert(0, _tokenDollarT);
          break;
        } else if (!escaped && quote != close && next == quote) {
          state._tokens.insert(0, _tokenString(quote, style));
          return _tokenize(stream, state);
        } else if (!escaped && _isQuote(next) && !_isQuote(quote)) {
          state._tokens.insert(0, _tokenStringStart(next, 'string'));
          stream.backUp(1);
          break;
        }
        escaped = !escaped && next == r'\';
      }
      return style;
    };
  }

  _Tokenizer _tokenStringStart(String quote, String style) => (stream, state) {
    state._tokens[0] = _tokenString(quote, style);
    stream.next();
    return _tokenize(stream, state);
  };

  String? _tokenDollar(StringStream stream, ShellState state) {
    if (state._tokens.length > 1) stream.eat(r'$');
    final ch = stream.next();
    if (ch != null && _dollarOpen.hasMatch(ch)) {
      state._tokens[0] = _tokenString(
        ch,
        ch == '('
            ? 'quote'
            : ch == '{'
            ? 'def'
            : 'string',
      );
      return _tokenize(stream, state);
    }
    // Upstream tests `undefined` at the line's end, which has no digit.
    if (ch == null || !_digit.hasMatch(ch)) stream.eatWhile(_word);
    state._tokens.removeAt(0);
    return 'def';
  }

  _Tokenizer _tokenHeredoc(String? delim) => (stream, state) {
    if (stream.sol() && stream.string == delim) state._tokens.removeAt(0);
    stream.skipToEnd();
    return 'string.special';
  };

  String? _tokenize(StringStream stream, ShellState state) =>
      (state._tokens.isEmpty ? _tokenBaseT : state._tokens[0])(stream, state);

  @override
  ShellState startState([int baseColumn = 0]) => ShellState._([]);

  @override
  ShellState copyState(ShellState state) => state.copy();

  @override
  String? token(StringStream stream, ShellState state) =>
      _tokenize(stream, state);
}
