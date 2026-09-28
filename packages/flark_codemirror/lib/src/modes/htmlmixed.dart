// Ported from CodeMirror 5.65.21 mode/htmlmixed/htmlmixed.js, over the ports
// of @codemirror/legacy-modes 6.5.4's xml (its `html` export) and css modes
// and of CodeMirror 5.65.21's javascript mode, as
// tool/reference/generate_mixed.mjs composes them. The xml mode styles a
// tag's angle brackets `angleBracket` where CodeMirror 5's styled them
// `tag bracket`, so both read as a tag. Only the default script and style
// tags are ported, not the `tags` option.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';
import 'css.dart';
import 'javascript.dart';
import 'xml.dart';

/// Which mode reads a tag's content, from its attributes: each rule's
/// attribute value must match its pattern, and a rule without an attribute
/// always applies.
typedef _TagRule = (String?, RegExp?, String);

final _defaultTags = <String, List<_TagRule>>{
  'script': [
    ('lang', RegExp('(javascript|babel)', caseSensitive: false), 'javascript'),
    (
      'type',
      RegExp(
        r'^(?:text|application)\/(?:x-)?(?:java|ecma)script$|^module$|^$',
        caseSensitive: false,
      ),
      'javascript',
    ),
    ('type', RegExp('.'), 'text/plain'),
    (null, null, 'javascript'),
  ],
  'style': [
    ('lang', RegExp(r'^css$', caseSensitive: false), 'css'),
    (
      'type',
      RegExp(r'^(text\/)?(x-)?(stylesheet|css)$', caseSensitive: false),
      'css',
    ),
    ('type', RegExp('.'), 'text/plain'),
    (null, null, 'css'),
  ],
};

final _tagStyle = RegExp(r'\btag\b');
final _tagPunctuation = RegExp(r'[<>\s\/]');
final _tagEnd = RegExp(r'>$');
final _inTagParts = RegExp(r'^([\S]+) (.*)');
final _openCloseTag = RegExp(r'<\/?$');
final _closeTagAfter = RegExp(r'^\s*<\/');
final _trimmed = RegExp(r'^\s*(.*?)\s*$');

String? _maybeBackup(StringStream stream, RegExp pat, String? style) {
  final cur = stream.current();
  final close = pat.firstMatch(cur)?.start ?? -1;
  if (close > -1) {
    stream.backUp(cur.length - close);
  } else if (_openCloseTag.hasMatch(cur)) {
    stream.backUp(cur.length);
    if (stream.match(pat, consume: false) == null) stream.matchString(cur);
  }
  return style;
}

final _attrRegexpCache = <String, RegExp>{};

RegExp _getAttrRegexp(String attr) => _attrRegexpCache[attr] ??= RegExp(
  '\\s+$attr\\s*=\\s*(\'|")?([^\'"]+)(\'|")?\\s*',
);

String _getAttrValue(String text, String attr) {
  final match = _getAttrRegexp(attr).firstMatch(text);
  return match != null ? _trimmed.firstMatch(match[2]!)![1]! : '';
}

/// A closing tag for [tagName], anywhere; [StringStream.match] anchors it.
RegExp _getTagRegexp(String tagName) =>
    RegExp('<\\/\\s*$tagName\\s*>', caseSensitive: false);

String _findMatchingMode(List<_TagRule> tagInfo, String tagText) {
  for (final (attr, pattern, mode) in tagInfo) {
    if (attr == null || pattern!.hasMatch(_getAttrValue(tagText, attr))) {
      return mode;
    }
  }
  // The last rule has no attribute.
  throw StateError('unreachable');
}

/// A script's or style's content with no mode: CodeMirror's `null` mode.
final class _PlainMode extends Mode<bool> {
  @override
  bool startState([int baseColumn = 0]) => true;

  @override
  bool copyState(bool state) => state;

  @override
  String? token(StringStream stream, bool state) {
    stream.skipToEnd();
    return null;
  }
}

/// The mode reading a script's or style's content, until its closing tag.
final class _Local {
  _Local(this.mode, this.state, this.endTag);
  final Mode<Object?> mode;
  final Object? state;
  final RegExp endTag;
}

final class HtmlMixedState {
  HtmlMixedState._(this._htmlState, this._inTag, this._local);
  final XmlState _htmlState;

  /// The script or style tag being read and its text so far.
  String? _inTag;
  _Local? _local;
}

/// HTML with CSS in its style tags and JavaScript in its script tags.
final class HtmlMixedMode extends Mode<HtmlMixedState> {
  HtmlMixedMode([super.config = const ModeConfig()])
    : _html = XmlMode(config, XmlOptions.html),
      _modes = {
        'javascript': JavaScriptMode(config),
        'css': CssMode.css(config),
        'text/plain': _PlainMode(),
      };

  final XmlMode _html;
  final Map<String, Mode<Object?>> _modes;

  String? _htmlToken(StringStream stream, HtmlMixedState state) {
    final style = _html.token(stream, state._htmlState);
    final tag =
        (style != null && _tagStyle.hasMatch(style)) || style == 'angleBracket';
    final tagName = state._htmlState.tagName?.toLowerCase();
    if (tag &&
        !_tagPunctuation.hasMatch(stream.current()) &&
        tagName != null &&
        tagName.isNotEmpty &&
        _defaultTags.containsKey(tagName)) {
      state._inTag = '$tagName ';
    } else if (state._inTag != null &&
        tag &&
        _tagEnd.hasMatch(stream.current())) {
      final inTag = _inTagParts.firstMatch(state._inTag!)!;
      state._inTag = null;
      final modeSpec = stream.current() == '>'
          ? _findMatchingMode(_defaultTags[inTag[1]]!, inTag[2]!)
          : 'text/plain';
      final mode = _modes[modeSpec]!;
      state._local = _Local(
        mode,
        mode.startState(_html.indent(state._htmlState, '', '') ?? 0),
        _getTagRegexp(inTag[1]!),
      );
    } else if (state._inTag != null) {
      state._inTag = state._inTag! + stream.current();
      if (stream.eol()) state._inTag = '${state._inTag} ';
    }
    return style;
  }

  @override
  HtmlMixedState startState([int baseColumn = 0]) =>
      HtmlMixedState._(_html.startState(), null, null);

  @override
  HtmlMixedState copyState(HtmlMixedState state) {
    final local = state._local;
    return HtmlMixedState._(
      _html.copyState(state._htmlState),
      state._inTag,
      local == null
          ? null
          : _Local(local.mode, local.mode.copyState(local.state), local.endTag),
    );
  }

  @override
  String? token(StringStream stream, HtmlMixedState state) {
    final local = state._local;
    if (local == null) return _htmlToken(stream, state);
    if (stream.match(local.endTag, consume: false) != null) {
      state._local = null;
      return null;
    }
    return _maybeBackup(
      stream,
      local.endTag,
      local.mode.token(stream, local.state),
    );
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(HtmlMixedState state, String textAfter, String line) {
    final local = state._local;
    if (local == null || _closeTagAfter.hasMatch(textAfter)) {
      return _html.indent(state._htmlState, textAfter, line);
    }
    if (local.mode.hasIndent) {
      return local.mode.indent(local.state, textAfter, line);
    }
    return null;
  }

  /// A closing tag, as the xml mode's, or a line of a script's or style's
  /// that the javascript or css mode re-indents.
  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(
    r'<\/[\s\w:]+>$|^\s*(?:case .*?:|default:|\{|\})$',
  );
}
