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
final _colon = RegExp(r':\s*');

/// The characters a key can start with only when the next is not white
/// space, a quote or a colon. The key's other branch takes any first
/// character but these and `#`.
const _keyIndicators = ',[]{}&*!|>\'"%@`';

bool _acceptedColon(String line, int i) =>
    line.codeUnitAt(i) == 0x3a &&
    (i + 1 == line.length || isJsSpace(line.codeUnitAt(i + 1)));
bool _isHash(String line, int i) => line.codeUnitAt(i) == 0x23;
bool _isNotSpace(String line, int i) => !isJsSpace(line.codeUnitAt(i));

/// The next index of a line, from a position on, where [test] holds. It is
/// remembered while the line is read: positions only grow along a line, so
/// the line is scanned once however many positions ask.
final class _NextIndex {
  _NextIndex(this.test);
  final bool Function(String line, int i) test;
  String? _line;
  int _from = 0, _found = -1;

  int from(String line, int pos) {
    if (!identical(line, _line) ||
        pos < _from ||
        (_found >= 0 && _found < pos)) {
      var i = pos;
      while (i < line.length && !test(line, i)) {
        i++;
      }
      _line = line;
      _from = pos;
      _found = i < line.length ? i : -1;
    }
    return _found;
  }
}

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

  final _colons = _NextIndex(_acceptedColon);
  final _laterColons = _NextIndex(_acceptedColon);
  final _hashes = _NextIndex(_isHash), _laterHashes = _NextIndex(_isHash);
  final _nonSpace = _NextIndex(_isNotSpace);

  /// Where upstream's key pattern matches from [pos], or null where it does
  /// not:
  ///
  ///     \s*(?:[,\[\]{}&*!|>'"%@`][^\s'":]|[^,\[\]{}#&*!|>'"%@`])[^#]*?(?=\s*:($|\s))
  ///
  /// The pattern is tried at every character of a line until a key is
  /// found. Run as a regular expression, a failure retries every split of
  /// the leading `\s*`, the lazy run and the lookahead's `\s*`, so 512
  /// spaces took 40 seconds; even a match costs the square of the white
  /// space inside the key. Read directly, a failure costs nothing and a
  /// match a scan to its colon. The branches are tried as the regex tries
  /// them, so the result is the same.
  int? _keyEnd(String line, int pos) {
    // The lookahead's colon: followed by white space or the line end, and
    // after at least the key's first character.
    final colon = _colons.from(line, pos + 1);
    if (colon < 0) return null;
    final hash = _hashes.from(line, pos);

    // The end of the lazy `[^#]*?` from `from`: as soon as the lookahead
    // holds, which is at the white space before the first accepted colon,
    // unless a `#` comes first.
    int? lazyEnd(int from) {
      final to = colon >= from ? colon : _laterColons.from(line, from);
      if (to < 0) return null;
      final stop = hash < 0 || hash >= from
          ? hash
          : _laterHashes.from(line, from);
      if (stop >= 0 && stop < to) return null;
      var end = to;
      while (end > from && isJsSpace(line.codeUnitAt(end - 1))) {
        end--;
      }
      return end;
    }

    // The greedy `\s*` first takes all the white space.
    final first = _nonSpace.from(line, pos);
    final unit = line.codeUnitAt(first);
    int? end;
    if (_keyIndicators.contains(line[first])) {
      final next = first + 1 < line.length ? line.codeUnitAt(first + 1) : -1;
      if (next >= 0 &&
          !isJsSpace(next) &&
          next != 0x27 &&
          next != 0x22 &&
          next != 0x3a) {
        end = lazyEnd(first + 2);
      }
    } else if (unit != 0x23) {
      end = lazyEnd(first + 1);
    }
    // Otherwise `\s*` gives back a space, which the second branch takes.
    // Giving back more only lengthens a run that fails the same way.
    if (end == null && first > pos) end = lazyEnd(first);
    return end;
  }

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
    if (!state._pair) {
      final end = _keyEnd(stream.string, stream.pos);
      if (end != null) {
        stream.pos = end;
        state._pair = true;
        state._keyCol = stream.indentation();
        return 'atom';
      }
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
