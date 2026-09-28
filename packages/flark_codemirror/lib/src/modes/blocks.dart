import '../mode.dart';
import '../stream.dart';

/// Indentation, as Flark's policy, for a mode that has none (CodeMirror's
/// shell mode keeps each new line at the previous one's indentation). A
/// block opened by a word at the start of a command (`if`, `for`) or by a
/// bracket indents its lines one unit past the line that opened it; a line
/// starting with the block's closer (`fi`), or with a word that continues
/// it (`then`, `else`), returns to that line's indentation. Tokens the inner
/// mode styles as strings, comments or quotes do not count, and neither
/// does a closer with no block open.
final class BlockIndentMode<S> extends Mode<BlockIndentState<S>> {
  BlockIndentMode(
    this.inner, {
    this.words = const {},
    this.brackets = const {},
    this.continuations = const {},
    required this.electricInput,
  }) : super(inner.config);

  final Mode<S> inner;

  /// Words that open a block at the start of a command, each with the word
  /// that closes it.
  final Map<String, String> words;

  /// Brackets that open a block anywhere, each with its closer.
  final Map<String, String> brackets;

  /// Words that continue the innermost block at its opener's indentation.
  final Set<String> continuations;

  @override
  final RegExp electricInput;

  late final _wordClosers = words.values.toSet();
  late final _bracketClosers = brackets.values.toSet();

  @override
  BlockIndentState<S> startState([int baseColumn = 0]) =>
      BlockIndentState._(inner.startState(baseColumn), [], 0, true);

  @override
  BlockIndentState<S> copyState(BlockIndentState<S> state) =>
      BlockIndentState._(
        inner.copyState(state._inner),
        List.of(state._blocks),
        state._indented,
        state._commandStart,
      );

  @override
  void blankLine(BlockIndentState<S> state) => inner.blankLine(state._inner);

  @override
  String? token(StringStream stream, BlockIndentState<S> state) {
    if (stream.sol()) {
      state._indented = stream.indentation();
      state._commandStart = true;
    }
    final from = stream.pos;
    final style = inner.token(stream, state._inner);
    final text = stream.pos > from
        ? stream.string.substring(from, stream.pos)
        : '';
    if (text.trim().isEmpty) return style;
    if (style != null &&
        (style.contains('string') ||
            style.contains('comment') ||
            style.contains('quote'))) {
      state._commandStart = false;
      return style;
    }
    final blocks = state._blocks;
    var nextStartsCommand = false;
    final opens = state._commandStart ? words[text] : null;
    if (opens != null) {
      blocks.add((opens, state._indented));
      nextStartsCommand = true;
    } else if (state._commandStart && _wordClosers.contains(text)) {
      _close(blocks, text);
    } else if (brackets[text] case final closer?) {
      blocks.add((closer, state._indented));
      nextStartsCommand = true;
    } else if (_bracketClosers.contains(text)) {
      _close(blocks, text);
    } else if (continuations.contains(text) || _separator.hasMatch(text)) {
      nextStartsCommand = true;
    }
    state._commandStart = nextStartsCommand;
    return style;
  }

  static void _close(List<(String, int)> blocks, String closer) {
    final at = blocks.lastIndexWhere((block) => block.$1 == closer);
    if (at >= 0) blocks.removeRange(at, blocks.length);
  }

  @override
  bool get hasIndent => true;

  /// Null outside every block, which keeps the previous line's indentation.
  @override
  int? indent(BlockIndentState<S> state, String textAfter, String line) {
    if (state._blocks.isEmpty) return null;
    final (closer, indented) = state._blocks.last;
    final first = _firstWord.firstMatch(textAfter)?[0] ?? '';
    return first == closer || continuations.contains(first)
        ? indented
        : indented + config.indentUnit;
  }
}

final _separator = RegExp(r'^[;&|]+$');
final _firstWord = RegExp(r'^(?:\w+|[^\s\w])');

final class BlockIndentState<S> {
  BlockIndentState._(
    this._inner,
    this._blocks,
    this._indented,
    this._commandStart,
  );
  final S _inner;

  /// Each open block's closer and the indentation of the line that opened
  /// it.
  final List<(String, int)> _blocks;
  int _indented;

  /// Whether the next token starts a command.
  bool _commandStart;
}
