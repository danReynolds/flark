import '../mode.dart';
import '../stream.dart';

/// Brackets open and close blocks in almost every language, so a fence in a
/// language without a ported mode still indents by them: Enter inside an
/// unclosed `(`, `[` or `{` indents one unit past the line that opened it,
/// and a closer starting a line returns to that line's indentation. Nothing
/// is styled, and with no bracket open indentation stays the previous
/// line's. Brackets inside strings and comments count too, since which
/// characters start those is unknown.
final class BracketsMode extends Mode<BracketsState> {
  BracketsMode([super.config = const ModeConfig()]);

  @override
  BracketsState startState([int baseColumn = 0]) => BracketsState._([], 0);

  @override
  BracketsState copyState(BracketsState state) =>
      BracketsState._(List.of(state._open), state._indented);

  @override
  String? token(StringStream stream, BracketsState state) {
    if (stream.sol()) state._indented = stream.indentation();
    final ch = stream.next()!;
    final opener = _openers.indexOf(ch);
    if (opener >= 0) {
      state._open.add((_closers[opener], state._indented));
    } else if (_closers.contains(ch)) {
      // A closer ends the innermost block it matches; a stray one is text.
      final at = state._open.lastIndexWhere((open) => open.$1 == ch);
      if (at >= 0) state._open.removeRange(at, state._open.length);
    } else {
      stream.eatWhile((c) => !_openers.contains(c) && !_closers.contains(c));
    }
    return null;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(BracketsState state, String textAfter, String line) {
    if (state._open.isEmpty) return null;
    final (closer, indented) = state._open.last;
    return textAfter.startsWith(closer)
        ? indented
        : indented + config.indentUnit;
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s*[\}\]\)]$');
}

const _openers = '([{';
const _closers = ')]}';

final class BracketsState {
  BracketsState._(this._open, this._indented);

  /// Each open bracket's closer and the indentation of its line.
  final List<(String, int)> _open;
  int _indented;
}
