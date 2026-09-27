// Ported from @codemirror/legacy-modes 6.5.4 mode/xml.js, its `xml` and
// `html` exports: `mkXML` with its XML or HTML configuration. The editor
// helpers `skipAttribute`, `xmlCurrentTag` and `xmlCurrentContext` are not
// ported.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

/// The configuration of an [XmlMode]: upstream's `xmlConfig` or
/// `htmlConfig`, with the parser options `mkXML` reads.
final class XmlOptions {
  const XmlOptions({
    this.htmlMode = false,
    this.autoSelfClosers = const {},
    this.implicitlyClosed = const {},
    this.contextGrabbers = const {},
    this.doNotIndent = const {},
    this.allowUnquoted = false,
    this.allowMissing = false,
    this.allowMissingTagName = false,
    this.caseFold = false,
    this.matchClosing = true,
    this.multilineTagIndentPastTag = true,
    this.multilineTagIndentFactor = 1,
    this.alignCDATA = false,
  });

  /// Upstream's `xmlConfig`, of its `xml` export.
  static const xml = XmlOptions();

  /// Upstream's `htmlConfig` in HTML mode, of its `html` export.
  static const html = XmlOptions(
    htmlMode: true,
    autoSelfClosers: {
      'area', 'base', 'br', 'col', 'command', //
      'embed', 'frame', 'hr', 'img', 'input',
      'keygen', 'link', 'meta', 'param', 'source',
      'track', 'wbr', 'menuitem',
    },
    implicitlyClosed: {
      'dd', 'li', 'optgroup', 'option', 'p', //
      'rp', 'rt', 'tbody', 'td', 'tfoot',
      'th', 'tr',
    },
    contextGrabbers: {
      'dd': {'dd', 'dt'},
      'dt': {'dd', 'dt'},
      'li': {'li'},
      'option': {'option', 'optgroup'},
      'optgroup': {'optgroup'},
      'p': {
        'address', 'article', 'aside', 'blockquote', 'dir', //
        'div', 'dl', 'fieldset', 'footer', 'form',
        'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
        'header', 'hgroup', 'hr', 'menu', 'nav', 'ol',
        'p', 'pre', 'section', 'table', 'ul',
      },
      'rp': {'rp', 'rt'},
      'rt': {'rp', 'rt'},
      'tbody': {'tbody', 'tfoot'},
      'td': {'td', 'th'},
      'tfoot': {'tbody'},
      'th': {'td', 'th'},
      'thead': {'tbody', 'tfoot'},
      'tr': {'tr'},
    },
    doNotIndent: {'pre'},
    allowUnquoted: true,
    allowMissing: true,
    caseFold: true,
  );

  final bool htmlMode;

  /// Elements without content, which close themselves.
  final Set<String> autoSelfClosers;

  /// Elements whose end tag may be left out.
  final Set<String> implicitlyClosed;

  /// For an element, the elements whose start tag closes it.
  final Map<String, Set<String>> contextGrabbers;

  /// Elements whose content keeps its own indentation.
  final Set<String> doNotIndent;

  final bool allowUnquoted, allowMissing, allowMissingTagName;

  /// Whether tag names ignore case, which only the editor reads.
  final bool caseFold;

  /// Whether an end tag must match the open element.
  final bool matchClosing;

  final bool multilineTagIndentPastTag;
  final int multilineTagIndentFactor;
  final bool alignCDATA;
}

typedef _Run = String? Function(StringStream stream, XmlState state);

/// A tokenizer, marked as upstream marks its functions.
final class _Tokenizer {
  const _Tokenizer(
    this.run, {
    this.isInText = false,
    this.isInAttribute = false,
  });
  final _Run run;
  final bool isInText, isInAttribute;
}

/// Upstream's state functions, which [XmlMode] runs by name.
enum _State {
  base,
  tagName,
  closeTagName,
  close,
  closeErr,
  attr,
  attrEq,
  attrValue,
  attrContinued,
}

final class _Context {
  _Context(XmlState state, String? tagName, this.startOfLine, XmlOptions config)
    : prev = state._context,
      tagName = tagName ?? '',
      indent = state._indented,
      noIndent =
          config.doNotIndent.contains(tagName) ||
          (state._context?.noIndent ?? false);
  final _Context? prev;
  final String tagName;
  final int indent;
  final bool startOfLine, noIndent;
}

final class XmlState {
  XmlState._(
    this._tokenize,
    this._state,
    this._indented,
    this._tagName,
    this._tagStart,
    this._context, [
    this._stringStartCol,
  ]);
  _Tokenizer _tokenize;
  _State _state;
  int _indented;
  String? _tagName;
  int? _tagStart;
  _Context? _context;
  int? _stringStartCol;

  /// The upstream default copy: contexts are shared.
  XmlState copy() => XmlState._(
    _tokenize,
    _state,
    _indented,
    _tagName,
    _tagStart,
    _context,
    _stringStartCol,
  );
}

String? _lower(String? tagName) => tagName?.toLowerCase();

final _nameChar = RegExp(r'[\w\._\-]');
final _hexDigit = RegExp(r'[a-fA-F\d]');
final _digit = RegExp(r'[\d]');
final _entityChar = RegExp(r'[\w\.\-:]');
final _attributeWord = RegExp(r'''[^\s\u00a0=<>\"\']*[^\s\u00a0=<>\"\'\/]''');
final _tagAfter = RegExp(r'^<(\/)?([\w_:\.-]*)');

/// Where [XmlMode.indent] indexes the context grabbers as a plain object,
/// its prototype answers for `constructor` (the `Object` function) and
/// `__proto__` (the prototype itself): the own properties of each that a
/// lower-case tag name can name.
const _inheritedGrabbers = {
  'constructor': {
    'length', 'name', 'prototype', 'assign', 'is', 'seal', //
    'create', 'freeze', 'keys', 'entries', 'values',
  },
  '__proto__': {'constructor', '__proto__'},
};

/// XML and HTML: the stream parser of upstream's `mkXML`.
final class XmlMode extends Mode<XmlState> {
  XmlMode([super.config = const ModeConfig(), this.options = XmlOptions.xml]);

  final XmlOptions options;

  // Return variables for tokenizers
  String? _type, _setStyle;

  late final _inTextT = _Tokenizer(_inText, isInText: true);
  late final _inTagT = _Tokenizer(_inTag);

  String? _inText(StringStream stream, XmlState state) {
    String? chain(_Tokenizer parser) {
      state._tokenize = parser;
      return parser.run(stream, state);
    }

    final ch = stream.next();
    if (ch == '<') {
      if (stream.eat('!') != null) {
        if (stream.eat('[') != null) {
          if (stream.matchString('CDATA[')) {
            return chain(_inBlock('atom', ']]>'));
          } else {
            return null;
          }
        } else if (stream.matchString('--')) {
          return chain(_inBlock('comment', '-->'));
        } else if (stream.matchString('DOCTYPE', caseInsensitive: true)) {
          stream.eatWhile(_nameChar);
          return chain(_doctype(1));
        } else {
          return null;
        }
      } else if (stream.eat('?') != null) {
        stream.eatWhile(_nameChar);
        state._tokenize = _inBlock('meta', '?>');
        return 'meta';
      } else {
        _type = stream.eat('/') != null ? 'closeTag' : 'openTag';
        state._tokenize = _inTagT;
        return 'angleBracket';
      }
    } else if (ch == '&') {
      bool ok;
      if (stream.eat('#') != null) {
        if (stream.eat('x') != null) {
          ok = stream.eatWhile(_hexDigit) && stream.eat(';') != null;
        } else {
          ok = stream.eatWhile(_digit) && stream.eat(';') != null;
        }
      } else {
        ok = stream.eatWhile(_entityChar) && stream.eat(';') != null;
      }
      return ok ? 'atom' : 'error';
    } else {
      // `[^&<]`
      stream.eatWhileCode((u) => u != 0x26 && u != 0x3c);
      return null;
    }
  }

  String? _inTag(StringStream stream, XmlState state) {
    final ch = stream.next();
    if (ch == '>' || (ch == '/' && stream.eat('>') != null)) {
      state._tokenize = _inTextT;
      _type = ch == '>' ? 'endTag' : 'selfcloseTag';
      return 'angleBracket';
    } else if (ch == '=') {
      _type = 'equals';
      return null;
    } else if (ch == '<') {
      state._tokenize = _inTextT;
      state._state = _State.base;
      state._tagName = null;
      state._tagStart = null;
      state._tokenize.run(stream, state);
      return 'invalid';
    } else if (ch == "'" || ch == '"') {
      state._tokenize = _inAttribute(ch!);
      state._stringStartCol = stream.column();
      return state._tokenize.run(stream, state);
    } else {
      stream.match(_attributeWord);
      return 'word';
    }
  }

  _Tokenizer _inAttribute(String quote) => _Tokenizer((stream, state) {
    while (!stream.eol()) {
      if (stream.next() == quote) {
        state._tokenize = _inTagT;
        break;
      }
    }
    return 'string';
  }, isInAttribute: true);

  _Tokenizer _inBlock(String style, String terminator) =>
      _Tokenizer((stream, state) {
        while (!stream.eol()) {
          if (stream.matchString(terminator)) {
            state._tokenize = _inTextT;
            break;
          }
          stream.next();
        }
        return style;
      });

  _Tokenizer _doctype(int depth) => _Tokenizer((stream, state) {
    String? ch;
    while ((ch = stream.next()) != null) {
      if (ch == '<') {
        state._tokenize = _doctype(depth + 1);
        return state._tokenize.run(stream, state);
      } else if (ch == '>') {
        if (depth == 1) {
          state._tokenize = _inTextT;
          break;
        } else {
          state._tokenize = _doctype(depth - 1);
          return state._tokenize.run(stream, state);
        }
      }
    }
    return 'meta';
  });

  void _popContext(XmlState state) {
    final context = state._context;
    if (context != null) state._context = context.prev;
  }

  void _maybePopContext(XmlState state, String? nextTagName) {
    while (true) {
      final context = state._context;
      if (context == null) return;
      final grabbers = options.contextGrabbers[_lower(context.tagName)];
      if (grabbers == null || !grabbers.contains(_lower(nextTagName))) return;
      _popContext(state);
    }
  }

  _State _run(_State next, String type, StringStream stream, XmlState state) =>
      switch (next) {
        _State.base => _baseState(type, stream, state),
        _State.tagName => _tagNameState(type, stream, state),
        _State.closeTagName => _closeTagNameState(type, stream, state),
        _State.close => _closeState(type, stream, state),
        _State.closeErr => _closeStateErr(type, stream, state),
        _State.attr => _attrState(type, stream, state),
        _State.attrEq => _attrEqState(type, stream, state),
        _State.attrValue => _attrValueState(type, stream, state),
        _State.attrContinued => _attrContinuedState(type, stream, state),
      };

  _State _baseState(String type, StringStream stream, XmlState state) {
    if (type == 'openTag') {
      state._tagStart = stream.column();
      return _State.tagName;
    } else if (type == 'closeTag') {
      return _State.closeTagName;
    } else {
      return _State.base;
    }
  }

  _State _tagNameState(String type, StringStream stream, XmlState state) {
    if (type == 'word') {
      state._tagName = stream.current();
      _setStyle = 'tag';
      return _State.attr;
    } else if (options.allowMissingTagName && type == 'endTag') {
      _setStyle = 'angleBracket';
      return _attrState(type, stream, state);
    } else {
      _setStyle = 'error';
      return _State.tagName;
    }
  }

  _State _closeTagNameState(String type, StringStream stream, XmlState state) {
    if (type == 'word') {
      final tagName = stream.current();
      final context = state._context;
      if (context != null &&
          context.tagName != tagName &&
          options.implicitlyClosed.contains(_lower(context.tagName))) {
        _popContext(state);
      }
      if ((state._context != null && state._context!.tagName == tagName) ||
          !options.matchClosing) {
        _setStyle = 'tag';
        return _State.close;
      } else {
        _setStyle = 'error';
        return _State.closeErr;
      }
    } else if (options.allowMissingTagName && type == 'endTag') {
      _setStyle = 'angleBracket';
      return _closeState(type, stream, state);
    } else {
      _setStyle = 'error';
      return _State.closeErr;
    }
  }

  _State _closeState(String type, StringStream _, XmlState state) {
    if (type != 'endTag') {
      _setStyle = 'error';
      return _State.close;
    }
    _popContext(state);
    return _State.base;
  }

  _State _closeStateErr(String type, StringStream stream, XmlState state) {
    _setStyle = 'error';
    return _closeState(type, stream, state);
  }

  _State _attrState(String type, StringStream _, XmlState state) {
    if (type == 'word') {
      _setStyle = 'attribute';
      return _State.attrEq;
    } else if (type == 'endTag' || type == 'selfcloseTag') {
      final tagName = state._tagName, tagStart = state._tagStart;
      state._tagName = null;
      state._tagStart = null;
      if (type == 'selfcloseTag' ||
          options.autoSelfClosers.contains(_lower(tagName))) {
        _maybePopContext(state, tagName);
      } else {
        _maybePopContext(state, tagName);
        state._context = _Context(
          state,
          tagName,
          tagStart == state._indented,
          options,
        );
      }
      return _State.base;
    }
    _setStyle = 'error';
    return _State.attr;
  }

  _State _attrEqState(String type, StringStream stream, XmlState state) {
    if (type == 'equals') return _State.attrValue;
    if (!options.allowMissing) _setStyle = 'error';
    return _attrState(type, stream, state);
  }

  _State _attrValueState(String type, StringStream stream, XmlState state) {
    if (type == 'string') return _State.attrContinued;
    if (type == 'word' && options.allowUnquoted) {
      _setStyle = 'string';
      return _State.attr;
    }
    _setStyle = 'error';
    return _attrState(type, stream, state);
  }

  _State _attrContinuedState(String type, StringStream stream, XmlState state) {
    if (type == 'string') return _State.attrContinued;
    return _attrState(type, stream, state);
  }

  @override
  XmlState startState([int baseColumn = 0]) =>
      XmlState._(_inTextT, _State.base, 0, null, null, null);

  @override
  XmlState copyState(XmlState state) => state.copy();

  @override
  String? token(StringStream stream, XmlState state) {
    final tagName = state._tagName;
    if ((tagName == null || tagName.isEmpty) && stream.sol()) {
      state._indented = stream.indentation();
    }

    if (stream.eatSpace()) return null;
    _type = null;
    var style = state._tokenize.run(stream, state);
    if ((style != null || _type != null) && style != 'comment') {
      _setStyle = null;
      state._state = _run(state._state, _type ?? style!, stream, state);
      if (_setStyle != null) style = _setStyle;
    }
    return style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(XmlState state, String textAfter, String line) {
    var context = state._context;
    // Indent multi-line strings (e.g. css).
    if (state._tokenize.isInAttribute) {
      if (state._tagStart == state._indented) {
        return state._stringStartCol! + 1;
      } else {
        return state._indented + config.indentUnit;
      }
    }
    if (context != null && context.noIndent) return null;
    if (!identical(state._tokenize, _inTagT) &&
        !identical(state._tokenize, _inTextT)) {
      return null;
    }
    // Indent the starts of attribute names.
    final tagName = state._tagName;
    if (tagName != null && tagName.isNotEmpty) {
      // A missing start is JavaScript's null, which adds as zero.
      final tagStart = state._tagStart ?? 0;
      if (options.multilineTagIndentPastTag) {
        return tagStart + tagName.length + 2;
      } else {
        final factor = options.multilineTagIndentFactor;
        return tagStart + config.indentUnit * (factor == 0 ? 1 : factor);
      }
    }
    if (options.alignCDATA && textAfter.contains('<![CDATA[')) return 0;
    final tagAfter = textAfter.isEmpty ? null : _tagAfter.firstMatch(textAfter);
    if (tagAfter != null && tagAfter[1] != null) {
      // Closing tag spotted
      while (context != null) {
        if (context.tagName == tagAfter[2]) {
          context = context.prev;
          break;
        } else if (options.implicitlyClosed.contains(_lower(context.tagName))) {
          context = context.prev;
        } else {
          break;
        }
      }
    } else if (tagAfter != null) {
      // Opening tag spotted
      while (context != null) {
        final name = _lower(context.tagName);
        final grabbers =
            options.contextGrabbers[name] ?? _inheritedGrabbers[name];
        if (grabbers != null && grabbers.contains(_lower(tagAfter[2]))) {
          context = context.prev;
        } else {
          break;
        }
      }
    }
    while (context != null && context.prev != null && !context.startOfLine) {
      context = context.prev;
    }
    if (context != null) return context.indent + config.indentUnit;
    // Upstream falls back to `state.baseIndent`, which it never sets.
    return 0;
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'<\/[\s\w:]+>$');
}
