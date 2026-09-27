// Ported from @codemirror/legacy-modes 6.5.4 mode/simple-mode.js, its
// `simpleMode` export: modes declared as states of rules, each a regular
// expression and the style of what it matches. Of the language data, only
// `dontIndentStates` and `indentOnInput` are ported; `name`, `mergeTokens`
// and the comment tokens are not.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

/// A rule's style computed from its match: a style, a list of styles or
/// null, used as returned.
typedef SimpleTokenFunction = Object? Function(Match matches);

/// A rule of a simple mode's state, as upstream's rule objects.
///
/// [regex] is a [RegExp], of which only case insensitivity and the Unicode
/// flag are kept, a [String] source, or null for the empty match. [token] is
/// a style, a list of styles for the regex's groups, or a
/// [SimpleTokenFunction]; the dots of given styles become spaces.
final class SimpleRule {
  const SimpleRule({
    this.regex,
    this.token,
    this.sol = false,
    this.next,
    this.push,
    this.pop = false,
    this.indent = false,
    this.dedent = false,
    this.dedentIfLineStart = true,
  });
  final Object? regex;
  final Object? token;
  final bool sol, pop, indent, dedent, dedentIfLineStart;
  final String? next, push;
}

/// Upstream's `simpleMode`: [states] of rules, from `start`, compiled once
/// and shared by the [SimpleMode]s made from them. [dontIndentStates] and
/// [indentOnInput] are upstream's language data.
final class SimpleModeSpec {
  SimpleModeSpec(
    Map<String, List<SimpleRule>> states, {
    this.dontIndentStates = const [],
    this.indentOnInput,
  }) {
    _ensureState(states, 'start');
    for (final MapEntry(key: state, value: orig) in states.entries) {
      final list = _states[state] = [];
      for (final data in orig) {
        list.add(_Rule(data, states));
        if (data.indent || data.dedent) _hasIndentation = true;
      }
    }
  }

  final List<String> dontIndentStates;
  final RegExp? indentOnInput;
  final _states = <String, List<_Rule>>{};
  var _hasIndentation = false;
}

void _ensureState(Map<String, List<SimpleRule>> states, String name) {
  if (!states.containsKey(name)) {
    throw ArgumentError('Undefined state $name in simple mode');
  }
}

bool _truthy(String? value) => value != null && value.isNotEmpty;

/// A rule's regular expression, which upstream anchors with `^` and matches
/// on the rest of the line. Matched on the line itself it reads the same,
/// except for what it reads before its start: [anchors] for `^` and
/// lookbehinds, [boundaries] for `\b` and `\B`.
final class _Regex {
  _Regex._(this.regex, this.anchors, this.boundaries);

  factory _Regex(
    String source, {
    bool caseSensitive = true,
    bool unicode = false,
  }) {
    var anchors = false, boundaries = false, inClass = false;
    for (var i = 0; i < source.length; i++) {
      switch (source[i]) {
        case r'\':
          i++;
          if (!inClass && i < source.length) {
            if (source[i] == 'b' || source[i] == 'B') boundaries = true;
          }
        case ']' when inClass:
          inClass = false;
        case '[' when !inClass:
          inClass = true;
        case '^' when !inClass:
          anchors = true;
        case '(' when !inClass:
          if (source.startsWith('(?<=', i) || source.startsWith('(?<!', i)) {
            anchors = true;
          }
      }
    }
    return _Regex._(
      RegExp('(?:$source)', caseSensitive: caseSensitive, unicode: unicode),
      anchors,
      boundaries,
    );
  }

  final RegExp regex;
  final bool anchors, boundaries;
}

_Regex _toRegex(Object? val) {
  if (val == null || val == '') return _Regex('');
  if (val is RegExp) {
    return _Regex(
      val.pattern,
      caseSensitive: val.isCaseSensitive,
      unicode: val.isUnicode,
    );
  }
  return _Regex('$val');
}

Object? _asToken(Object? val) {
  if (val == null || val == '') return null;
  if (val is SimpleTokenFunction) return val;
  if (val is String) return val.replaceAll('.', ' ');
  return [
    for (final style in val as List<String?>)
      _truthy(style) ? style!.replaceAll('.', ' ') : style,
  ];
}

final class _Rule {
  _Rule(this.data, Map<String, List<SimpleRule>> states)
    : regex = _toRegex(data.regex),
      token = _asToken(data.token) {
    if (_truthy(data.next) || _truthy(data.push)) {
      _ensureState(states, _truthy(data.next) ? data.next! : data.push!);
    }
  }
  final SimpleRule data;
  final _Regex regex;
  final Object? token;
}

final class _Pending {
  const _Pending(this.text, this.token);
  final String text;
  final String? token;
}

/// JavaScript's `\w` for one code unit, with the two characters it adds when
/// a Unicode regex ignores case.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f ||
    u == 0x17f ||
    u == 0x212a;

/// CodeMirror 6's `stream.match` of a rule's regex: on the rest of the line,
/// as though the line began at the stream's position. A Unicode regex in
/// the middle of a surrogate pair also needs the rest, where the pair's
/// second half stands alone.
Match? _match(StringStream stream, _Regex rule) {
  final string = stream.string, pos = stream.pos;
  final before = pos > 0 ? string.codeUnitAt(pos - 1) : 0;
  final sliced =
      pos > 0 &&
      (rule.anchors ||
          rule.boundaries && _isWordUnit(before) ||
          rule.regex.isUnicode &&
              (before & 0xfc00) == 0xd800 &&
              pos < string.length &&
              (string.codeUnitAt(pos) & 0xfc00) == 0xdc00);
  final found = sliced
      ? rule.regex.matchAsPrefix(string.substring(pos))
      : rule.regex.matchAsPrefix(string, pos);
  if (found != null) stream.pos += found.end - found.start;
  return found;
}

/// The first style of a computed token, read as upstream indexes it.
String? _first(Object? token) => switch (token) {
  String s => s.isEmpty ? null : s[0],
  List<Object?> list => list.isEmpty ? null : list[0] as String?,
  _ => throw StateError('Cannot read the styles of $token'),
};

final class SimpleState {
  SimpleState._(this._state, this._pending, this._indent, [this._stack]);
  String _state;
  List<_Pending>? _pending;
  final List<int>? _indent;
  List<String>? _stack;

  /// Upstream's copy: the indentation and state stacks are copied, pending
  /// tokens shared.
  SimpleState copy() => SimpleState._(
    _state,
    _pending,
    _indent == null ? null : List.of(_indent),
    _stack == null ? null : List.of(_stack!),
  );
}

/// A mode declared by a [SimpleModeSpec]: the stream parser upstream's
/// `simpleMode` returns.
class SimpleMode extends Mode<SimpleState> {
  SimpleMode(this.spec, [super.config = const ModeConfig()]);
  final SimpleModeSpec spec;

  @override
  SimpleState startState([int baseColumn = 0]) =>
      SimpleState._('start', null, spec._hasIndentation ? [] : null);

  @override
  SimpleState copyState(SimpleState state) => state.copy();

  @override
  String? token(StringStream stream, SimpleState state) {
    final pending = state._pending;
    if (pending != null) {
      // An exhausted list yields upstream's `undefined`, which it then reads.
      final pend = pending.isEmpty ? null : pending.removeAt(0);
      if (pending.isEmpty) state._pending = null;
      stream.pos += pend!.text.length;
      return pend.token;
    }

    final curState = spec._states[state._state]!;
    for (final rule in curState) {
      final data = rule.data;
      final matches = !data.sol || stream.sol()
          ? _match(stream, rule.regex)
          : null;
      if (matches == null) continue;
      final stack = state._stack;
      if (_truthy(data.next)) {
        state._state = data.next!;
      } else if (_truthy(data.push)) {
        (state._stack ??= []).add(state._state);
        state._state = data.push!;
      } else if (data.pop && stack != null && stack.isNotEmpty) {
        state._state = stack.removeLast();
      }

      final indent = state._indent;
      if (data.indent) indent!.add(stream.indentation() + stream.indentUnit);
      if (data.dedent && indent!.isNotEmpty) indent.removeLast();
      var token = rule.token;
      if (token is SimpleTokenFunction) token = token(matches);
      final length = matches.groupCount + 1;
      if (length > 2 && rule.token != null && rule.token is! String) {
        // A function has no styles by group: upstream indexes the function.
        final styles = rule.token;
        final list = state._pending = [];
        for (var j = 2; j < length; j++) {
          final text = matches[j];
          if (_truthy(text)) {
            list.add(
              _Pending(
                text!,
                styles is List<String?> && j - 1 < styles.length
                    ? styles[j - 1]
                    : null,
              ),
            );
          }
        }
        stream.backUp(matches[0]!.length - (matches[1]?.length ?? 0));
        return _first(token);
      } else if (token is List) {
        return token.isEmpty ? null : token[0] as String?;
      } else {
        return token as String?;
      }
    }
    stream.next();
    return null;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(SimpleState state, String textAfter, String line) {
    final indent = state._indent;
    if (indent == null || spec.dontIndentStates.contains(state._state)) {
      return null;
    }

    var pos = indent.length - 1;
    final rules = spec._states[state._state]!;
    scan:
    for (;;) {
      for (final rule in rules) {
        if (rule.data.dedent && rule.data.dedentIfLineStart) {
          final m = rule.regex.regex.matchAsPrefix(textAfter);
          if (m != null && m[0]!.isNotEmpty) {
            pos--;
            // Upstream reads `next` and `push` off the compiled rule, which
            // has neither, so the rules stay the state's.
            textAfter = textAfter.substring(m[0]!.length);
            continue scan;
          }
        }
      }
      break;
    }
    return pos < 0 ? 0 : indent[pos];
  }

  @override
  RegExp? get electricInput => spec.indentOnInput;
}
