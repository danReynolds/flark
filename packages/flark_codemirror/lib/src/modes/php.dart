// Ported from CodeMirror 5.65.21 mode/php/php.js, its `php` mode: PHP code
// in `<?php ... ?>` within HTML, over the ports of its PHP configuration of
// the clike parser (clike.dart) and of htmlmixed, as
// tool/reference/generate_mixed.mjs composes them. A fence usually holds
// bare PHP, so it starts in PHP, with `<?php` styled `meta`, unless it opens
// with an HTML tag.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';
import 'clike.dart';
import 'htmlmixed.dart';

/// An HTML token cut short at a `<?`: either the quote that opened the
/// string it was in, or where it ended and its style.
final class _Pending {
  const _Pending.quote(String this.quote) : end = 0, style = null;
  const _Pending.rest(this.end, this.style) : quote = null;
  final String? quote;
  final int end;
  final String? style;
}

final _opensWithTag = RegExp(r'^\s*<(?!\?)');
final _phpOpen = RegExp(r'<\?\w*');
final _endsWithQuote = RegExp(r'''['"]$''');
final _closeTagAfter = RegExp(r'^\s*<\/');

final class PhpState {
  PhpState._(this._html, this._php, this._inPhp, this._pending, this._sniffed);
  final HtmlMixedState _html;

  /// The PHP state, dropped at a `?>` that closes no block.
  ClikeState? _php;
  bool _inPhp;
  _Pending? _pending;

  /// Whether the fence's first line has decided whether it starts in HTML.
  bool _sniffed;
}

/// PHP, and the HTML around it.
final class PhpMode extends Mode<PhpState> {
  PhpMode([super.config = const ModeConfig()])
    : _htmlMode = HtmlMixedMode(config),
      _phpMode = ClikeMode.php(config);

  final HtmlMixedMode _htmlMode;
  final ClikeMode _phpMode;

  @override
  PhpState startState([int baseColumn = 0]) => PhpState._(
    _htmlMode.startState(),
    _phpMode.startState(),
    true,
    null,
    false,
  );

  @override
  PhpState copyState(PhpState state) => PhpState._(
    _htmlMode.copyState(state._html),
    state._php == null ? null : _phpMode.copyState(state._php!),
    state._inPhp,
    state._pending,
    state._sniffed,
  );

  @override
  String? token(StringStream stream, PhpState state) {
    if (!state._sniffed) {
      state._sniffed = true;
      if (_opensWithTag.hasMatch(stream.string)) {
        state._inPhp = false;
        state._php = null;
      }
    }
    if (stream.sol() &&
        state._pending != null &&
        state._pending!.quote == null) {
      state._pending = null;
    }
    if (!state._inPhp) {
      if (stream.match(_phpOpen) != null) {
        state._inPhp = true;
        state._php ??= _phpMode.startState(
          _htmlMode.indent(state._html, '', '') ?? 0,
        );
        return 'meta';
      }
      final pending = state._pending;
      String? style;
      if (pending != null && pending.quote != null) {
        while (!stream.eol() && stream.next() != pending.quote) {}
        style = 'string';
      } else if (pending != null && stream.pos < pending.end) {
        stream.pos = pending.end;
        style = pending.style;
      } else {
        style = _htmlMode.token(stream, state._html);
      }
      state._pending = null;
      final cur = stream.current();
      final openPhp = cur.indexOf('<?');
      if (openPhp != -1) {
        final quote = style == 'string' ? _endsWithQuote.firstMatch(cur) : null;
        state._pending = quote != null && !cur.contains('?>')
            ? _Pending.quote(quote[0]!)
            : _Pending.rest(stream.pos, style);
        stream.backUp(cur.length - openPhp);
      }
      return style;
    }
    final php = state._php!;
    if (php.atBase && stream.matchString('?>')) {
      state._inPhp = false;
      if (php.atTop) state._php = null;
      return 'meta';
    }
    if (php.atBase && stream.match(_phpOpen) != null) return 'meta';
    return _phpMode.token(stream, php);
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(PhpState state, String textAfter, String line) {
    if (!state._inPhp && _closeTagAfter.hasMatch(textAfter) ||
        state._inPhp && textAfter.startsWith('?>')) {
      return _htmlMode.indent(state._html, textAfter, line);
    }
    return state._inPhp
        ? _phpMode.indent(state._php!, textAfter, line)
        : _htmlMode.indent(state._html, textAfter, line);
  }

  /// PHP's closers, as the clike mode's, and the mixed HTML mode's.
  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(
    r'<\/[\s\w:]+>$|^\s*(?:case .*?:|default:|\{\}?|\})$',
  );
}
