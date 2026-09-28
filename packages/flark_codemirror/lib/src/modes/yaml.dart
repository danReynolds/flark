// Ported from @codemirror/legacy-modes 6.5.4 mode/yaml.js, its `yaml`
// export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

const _cons = ['true', 'false', 'on', 'off', 'yes', 'no'];
// Upstream's pattern starts with `\b` and is not anchored. On the rest of
// the line, a match at its start needs only a word character there, which
// every constant begins with; a later match is refused. Anchored here, a
// `\b` would look at the character before the stream's position instead.
final _keywordRegex = RegExp(
  '((${_cons.join(')|(')}))\$',
  caseSensitive: false,
);

final _space = RegExp(r'\s');
final _string = RegExp(r'''('([^']|\\.)*'?|"([^"]|\\.)*"?)''');
final _listItem = RegExp(r'\s*-\s+');
final _bracket = RegExp(r'(\{|\}|\[|\])');
final _blockLiteral = RegExp(r'\s*(\||\>)\s*');
final _reference = RegExp(r'\s*(\&|\*)[a-z0-9\._-]+\b', caseSensitive: false);
final _number = RegExp(r'\s*-?[0-9\.\,]+\s?$');
final _inlineNumber = RegExp(r'\s*-?[0-9\.\,]+\s?(?=(,|}))');
final _key = RegExp(
  r'''\s*(?:[,\[\]{}&*!|>'"%@`][^\s'":]|[^,\[\]{}#&*!|>'"%@`])[^#]*?(?=\s*:($|\s))''',
);
final _colon = RegExp(r':\s*');

final class YamlState {
  YamlState._(
    this._pair,
    this._pairStart,
    this._keyCol,
    this._inlinePairs,
    this._inlineList,
    this._literal,
    this._escaped,
  );
  bool _pair, _pairStart;
  int _keyCol, _inlinePairs, _inlineList;
  bool _literal, _escaped;

  /// The upstream default copy.
  YamlState copy() => YamlState._(
    _pair,
    _pairStart,
    _keyCol,
    _inlinePairs,
    _inlineList,
    _literal,
    _escaped,
  );
}

/// YAML: the upstream `yaml` stream parser.
final class YamlMode extends Mode<YamlState> {
  YamlMode([super.config = const ModeConfig()]);

  @override
  String? token(StringStream stream, YamlState state) {
    final ch = stream.peek();
    final esc = state._escaped;
    state._escaped = false;
    // comments
    if (ch == '#' &&
        (stream.pos == 0 || _space.hasMatch(stream.string[stream.pos - 1]))) {
      stream.skipToEnd();
      return 'comment';
    }

    if (stream.match(_string) != null) return 'string';

    if (state._literal && stream.indentation() > state._keyCol) {
      stream.skipToEnd();
      return 'string';
    } else if (state._literal) {
      state._literal = false;
    }
    if (stream.sol()) {
      state._keyCol = 0;
      state._pair = false;
      state._pairStart = false;
      // document start
      if (stream.matchString('---')) return 'def';
      // document end
      if (stream.matchString('...')) return 'def';
      // array list item
      if (stream.match(_listItem) != null) return 'meta';
    }
    // inline pairs/lists
    if (stream.match(_bracket) != null) {
      if (ch == '{') {
        state._inlinePairs++;
      } else if (ch == '}') {
        state._inlinePairs--;
      } else if (ch == '[') {
        state._inlineList++;
      } else {
        state._inlineList--;
      }
      return 'meta';
    }

    // list separator
    if (state._inlineList > 0 && !esc && ch == ',') {
      stream.next();
      return 'meta';
    }
    // pairs separator
    if (state._inlinePairs > 0 && !esc && ch == ',') {
      state._keyCol = 0;
      state._pair = false;
      state._pairStart = false;
      stream.next();
      return 'meta';
    }

    // start of value of a pair
    if (state._pairStart) {
      // block literals
      if (stream.match(_blockLiteral) != null) {
        state._literal = true;
        return 'meta';
      }
      // references
      if (stream.match(_reference) != null) return 'variable';
      // numbers
      if (state._inlinePairs == 0 && stream.match(_number) != null) {
        return 'number';
      }
      if (state._inlinePairs > 0 && stream.match(_inlineNumber) != null) {
        return 'number';
      }
      // keywords
      if (stream.match(_keywordRegex) != null) return 'keyword';
    }

    // pairs (associative arrays) -> key
    if (!state._pair && stream.match(_key) != null) {
      state._pair = true;
      state._keyCol = stream.indentation();
      return 'atom';
    }
    if (state._pair && stream.match(_colon) != null) {
      state._pairStart = true;
      return 'meta';
    }

    // nothing found, continue
    state._pairStart = false;
    state._escaped = ch == r'\';
    stream.next();
    return null;
  }

  @override
  YamlState startState([int baseColumn = 0]) =>
      YamlState._(false, false, 0, 0, 0, false, false);

  @override
  YamlState copyState(YamlState state) => state.copy();
}
