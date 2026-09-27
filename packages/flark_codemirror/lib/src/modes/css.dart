// Ported from @codemirror/legacy-modes 6.5.4 mode/css.js, its `css`, `sCSS`
// and `less` exports, and the `keywords` export that other modes read. `gss`
// and what only it uses are not ported, nor the options no ported export
// sets (`inline`, and `highlightNonStandardPropertyKeywords`, which stays on).
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE
//
// Upstream's tokenizers hand the parser a `type` through `ret`, and its hooks
// return `[style, type]` pairs. Here every tokenizer and hook returns the
// pair, and the mode keeps the type between tokens: a tokenizer that returns
// nothing leaves the previous token's type standing, as upstream's does.

import 'dart:math' as math;

import '../mode.dart';
import '../stream.dart';

/// A token's style and its type for the parser.
typedef _Token = (String?, String?);

/// Returns null where upstream returns nothing.
typedef _Tokenizer = _Token? Function(StringStream stream, CssState state);

/// Returns null where upstream returns `false`, declining the character.
typedef _Hook = _Token? Function(StringStream stream, CssState state);

List<String> _words(String words) => words.split(' ');

/// Upstream's `keySet`: the words, lowercased.
Set<String> _keySet(List<String> array) => {
  for (final key in array) key.toLowerCase(),
};

final _documentTypeList = _words('domain regexp url url-prefix');
final _documentTypes = _keySet(_documentTypeList);

final _mediaTypeList = _words(
  'all aural braille handheld print projection screen tty tv embossed',
);
final _mediaTypes = _keySet(_mediaTypeList);

final _mediaFeatureList = _words(
  'width min-width max-width height min-height max-height device-width '
  'min-device-width max-device-width device-height min-device-height '
  'max-device-height aspect-ratio min-aspect-ratio max-aspect-ratio '
  'device-aspect-ratio min-device-aspect-ratio max-device-aspect-ratio color '
  'min-color max-color color-index min-color-index max-color-index monochrome '
  'min-monochrome max-monochrome resolution min-resolution max-resolution scan '
  'grid orientation device-pixel-ratio min-device-pixel-ratio '
  'max-device-pixel-ratio pointer any-pointer hover any-hover '
  'prefers-color-scheme dynamic-range video-dynamic-range',
);
final _mediaFeatures = _keySet(_mediaFeatureList);

final _mediaValueKeywordList = _words(
  'landscape portrait none coarse fine on-demand hover interlace progressive '
  'dark light standard high',
);
final _mediaValueKeywords = _keySet(_mediaValueKeywordList);

final _propertyKeywordList = _words(
  'align-content align-items align-self alignment-adjust alignment-baseline '
  'all anchor-point animation animation-delay animation-direction '
  'animation-duration animation-fill-mode animation-iteration-count '
  'animation-name animation-play-state animation-timing-function appearance '
  'azimuth backdrop-filter backface-visibility background '
  'background-attachment background-blend-mode background-clip '
  'background-color background-image background-origin background-position '
  'background-position-x background-position-y background-repeat '
  'background-size baseline-shift binding bleed block-size bookmark-label '
  'bookmark-level bookmark-state bookmark-target border border-bottom '
  'border-bottom-color border-bottom-left-radius border-bottom-right-radius '
  'border-bottom-style border-bottom-width border-collapse border-color '
  'border-image border-image-outset border-image-repeat border-image-slice '
  'border-image-source border-image-width border-left border-left-color '
  'border-left-style border-left-width border-radius border-right '
  'border-right-color border-right-style border-right-width border-spacing '
  'border-style border-top border-top-color border-top-left-radius '
  'border-top-right-radius border-top-style border-top-width border-width '
  'bottom box-decoration-break box-shadow box-sizing break-after break-before '
  'break-inside caption-side caret-color clear clip color color-profile '
  'column-count column-fill column-gap column-rule column-rule-color '
  'column-rule-style column-rule-width column-span column-width columns '
  'contain content counter-increment counter-reset crop cue cue-after '
  'cue-before cursor direction display dominant-baseline '
  'drop-initial-after-adjust drop-initial-after-align '
  'drop-initial-before-adjust drop-initial-before-align drop-initial-size '
  'drop-initial-value elevation empty-cells fit fit-content fit-position flex '
  'flex-basis flex-direction flex-flow flex-grow flex-shrink flex-wrap float '
  'float-offset flow-from flow-into font font-family font-feature-settings '
  'font-kerning font-language-override font-optical-sizing font-size '
  'font-size-adjust font-stretch font-style font-synthesis font-variant '
  'font-variant-alternates font-variant-caps font-variant-east-asian '
  'font-variant-ligatures font-variant-numeric font-variant-position '
  'font-variation-settings font-weight gap grid grid-area grid-auto-columns '
  'grid-auto-flow grid-auto-rows grid-column grid-column-end grid-column-gap '
  'grid-column-start grid-gap grid-row grid-row-end grid-row-gap '
  'grid-row-start grid-template grid-template-areas grid-template-columns '
  'grid-template-rows hanging-punctuation height hyphens icon '
  'image-orientation image-rendering image-resolution inline-box-align inset '
  'inset-block inset-block-end inset-block-start inset-inline inset-inline-end '
  'inset-inline-start isolation justify-content justify-items justify-self '
  'left letter-spacing line-break line-height line-height-step line-stacking '
  'line-stacking-ruby line-stacking-shift line-stacking-strategy list-style '
  'list-style-image list-style-position list-style-type margin margin-bottom '
  'margin-left margin-right margin-top marks marquee-direction marquee-loop '
  'marquee-play-count marquee-speed marquee-style mask-clip mask-composite '
  'mask-image mask-mode mask-origin mask-position mask-repeat mask-size '
  'mask-type max-block-size max-height max-inline-size max-width '
  'min-block-size min-height min-inline-size min-width mix-blend-mode move-to '
  'nav-down nav-index nav-left nav-right nav-up object-fit object-position '
  'offset offset-anchor offset-distance offset-path offset-position '
  'offset-rotate opacity order orphans outline outline-color outline-offset '
  'outline-style outline-width overflow overflow-style overflow-wrap '
  'overflow-x overflow-y padding padding-bottom padding-left padding-right '
  'padding-top page page-break-after page-break-before page-break-inside '
  'page-policy pause pause-after pause-before perspective perspective-origin '
  'pitch pitch-range place-content place-items place-self play-during position '
  'presentation-level punctuation-trim quotes region-break-after '
  'region-break-before region-break-inside region-fragment rendering-intent '
  'resize rest rest-after rest-before richness right rotate rotation '
  'rotation-point row-gap ruby-align ruby-overhang ruby-position ruby-span '
  'scale scroll-behavior scroll-margin scroll-margin-block '
  'scroll-margin-block-end scroll-margin-block-start scroll-margin-bottom '
  'scroll-margin-inline scroll-margin-inline-end scroll-margin-inline-start '
  'scroll-margin-left scroll-margin-right scroll-margin-top scroll-padding '
  'scroll-padding-block scroll-padding-block-end scroll-padding-block-start '
  'scroll-padding-bottom scroll-padding-inline scroll-padding-inline-end '
  'scroll-padding-inline-start scroll-padding-left scroll-padding-right '
  'scroll-padding-top scroll-snap-align scroll-snap-type shape-image-threshold '
  'shape-inside shape-margin shape-outside size speak speak-as speak-header '
  'speak-numeral speak-punctuation speech-rate stress string-set tab-size '
  'table-layout target target-name target-new target-position text-align '
  'text-align-last text-combine-upright text-decoration text-decoration-color '
  'text-decoration-line text-decoration-skip text-decoration-skip-ink '
  'text-decoration-style text-emphasis text-emphasis-color '
  'text-emphasis-position text-emphasis-style text-height text-indent '
  'text-justify text-orientation text-outline text-overflow text-rendering '
  'text-shadow text-size-adjust text-space-collapse text-transform '
  'text-underline-position text-wrap top touch-action transform '
  'transform-origin transform-style transition transition-delay '
  'transition-duration transition-property transition-timing-function '
  'translate unicode-bidi user-select vertical-align visibility voice-balance '
  'voice-duration voice-family voice-pitch voice-range voice-rate voice-stress '
  'voice-volume volume white-space widows width will-change word-break '
  'word-spacing word-wrap writing-mode z-index clip-path clip-rule mask '
  'enable-background filter flood-color flood-opacity lighting-color '
  'stop-color stop-opacity pointer-events color-interpolation '
  'color-interpolation-filters color-rendering fill fill-opacity fill-rule '
  'image-rendering marker marker-end marker-mid marker-start paint-order '
  'shape-rendering stroke stroke-dasharray stroke-dashoffset stroke-linecap '
  'stroke-linejoin stroke-miterlimit stroke-opacity stroke-width '
  'text-rendering baseline-shift dominant-baseline '
  'glyph-orientation-horizontal glyph-orientation-vertical text-anchor '
  'writing-mode',
);
final _propertyKeywords = _keySet(_propertyKeywordList);

final _nonStandardPropertyKeywordList = _words(
  'accent-color aspect-ratio border-block border-block-color border-block-end '
  'border-block-end-color border-block-end-style border-block-end-width '
  'border-block-start border-block-start-color border-block-start-style '
  'border-block-start-width border-block-style border-block-width '
  'border-inline border-inline-color border-inline-end border-inline-end-color '
  'border-inline-end-style border-inline-end-width border-inline-start '
  'border-inline-start-color border-inline-start-style '
  'border-inline-start-width border-inline-style border-inline-width '
  'content-visibility margin-block margin-block-end margin-block-start '
  'margin-inline margin-inline-end margin-inline-start overflow-anchor '
  'overscroll-behavior padding-block padding-block-end padding-block-start '
  'padding-inline padding-inline-end padding-inline-start scroll-snap-stop '
  'scrollbar-3d-light-color scrollbar-arrow-color scrollbar-base-color '
  'scrollbar-dark-shadow-color scrollbar-face-color scrollbar-highlight-color '
  'scrollbar-shadow-color scrollbar-track-color searchfield-cancel-button '
  'searchfield-decoration searchfield-results-button '
  'searchfield-results-decoration shape-inside zoom',
);
final _nonStandardPropertyKeywords = _keySet(_nonStandardPropertyKeywordList);

final _fontPropertyList = _words(
  'font-display font-family src unicode-range font-variant '
  'font-feature-settings font-stretch font-weight font-style',
);
final _fontProperties = _keySet(_fontPropertyList);

final _counterDescriptorList = _words(
  'additive-symbols fallback negative pad prefix range speak-as suffix symbols '
  'system',
);
final _counterDescriptors = _keySet(_counterDescriptorList);

final _colorKeywordList = _words(
  'aliceblue antiquewhite aqua aquamarine azure beige bisque black '
  'blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse '
  'chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan '
  'darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta '
  'darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen '
  'darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink '
  'deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen '
  'fuchsia gainsboro ghostwhite gold goldenrod gray grey green greenyellow '
  'honeydew hotpink indianred indigo ivory khaki lavender lavenderblush '
  'lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow '
  'lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen '
  'lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime '
  'limegreen linen magenta maroon mediumaquamarine mediumblue mediumorchid '
  'mediumpurple mediumseagreen mediumslateblue mediumspringgreen '
  'mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin '
  'navajowhite navy oldlace olive olivedrab orange orangered orchid '
  'palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff '
  'peru pink plum powderblue purple rebeccapurple red rosybrown royalblue '
  'saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue '
  'slateblue slategray slategrey snow springgreen steelblue tan teal thistle '
  'tomato turquoise violet wheat white whitesmoke yellow yellowgreen',
);
final _colorKeywords = _keySet(_colorKeywordList);

final _valueKeywordList = _words(
  'above absolute activeborder additive activecaption afar after-white-space '
  'ahead alias all all-scroll alphabetic alternate always amharic '
  'amharic-abegede antialiased appworkspace arabic-indic armenian asterisks '
  'attr auto auto-flow avoid avoid-column avoid-page avoid-region axis-pan '
  'background backwards baseline below bidi-override binary bengali blink '
  'block block-axis blur bold bolder border border-box both bottom break '
  'break-all break-word brightness bullets button buttonface buttonhighlight '
  'buttonshadow buttontext calc cambodian capitalize caps-lock-indicator '
  'caption captiontext caret cell center checkbox circle cjk-decimal '
  'cjk-earthly-branch cjk-heavenly-stem cjk-ideographic clear clip close-quote '
  'col-resize collapse color color-burn color-dodge column column-reverse '
  'compact condensed conic-gradient contain content contents content-box '
  'context-menu continuous contrast copy counter counters cover crop cross '
  'crosshair cubic-bezier currentcolor cursive cyclic darken dashed decimal '
  'decimal-leading-zero default default-button dense destination-atop '
  'destination-in destination-out destination-over devanagari difference disc '
  'discard disclosure-closed disclosure-open document dot-dash dot-dot-dash '
  'dotted double down drop-shadow e-resize ease ease-in ease-in-out ease-out '
  'element ellipse ellipsis embed end ethiopic ethiopic-abegede '
  'ethiopic-abegede-am-et ethiopic-abegede-gez ethiopic-abegede-ti-er '
  'ethiopic-abegede-ti-et ethiopic-halehame-aa-er ethiopic-halehame-aa-et '
  'ethiopic-halehame-am-et ethiopic-halehame-gez ethiopic-halehame-om-et '
  'ethiopic-halehame-sid-et ethiopic-halehame-so-et ethiopic-halehame-ti-er '
  'ethiopic-halehame-ti-et ethiopic-halehame-tig ethiopic-numeric ew-resize '
  'exclusion expanded extends extra-condensed extra-expanded fantasy fast fill '
  'fill-box fixed flat flex flex-end flex-start footnotes forwards from '
  'geometricPrecision georgian grayscale graytext grid groove gujarati '
  'gurmukhi hand hangul hangul-consonant hard-light hebrew help hidden hide '
  'higher highlight highlighttext hiragana hiragana-iroha horizontal hsl hsla '
  'hue hue-rotate icon ignore inactiveborder inactivecaption '
  'inactivecaptiontext infinite infobackground infotext inherit initial inline '
  'inline-axis inline-block inline-flex inline-grid inline-table inset inside '
  'intrinsic invert italic japanese-formal japanese-informal justify kannada '
  'katakana katakana-iroha keep-all khmer korean-hangul-formal '
  'korean-hanja-formal korean-hanja-informal landscape lao large larger left '
  'level lighter lighten line-through linear linear-gradient lines list-item '
  'listbox listitem local logical loud lower lower-alpha lower-armenian '
  'lower-greek lower-hexadecimal lower-latin lower-norwegian lower-roman '
  'lowercase ltr luminosity malayalam manipulation match matrix matrix3d '
  'media-play-button media-slider media-sliderthumb media-volume-slider '
  'media-volume-sliderthumb medium menu menulist menulist-button menutext '
  'message-box middle min-intrinsic mix mongolian monospace move multiple '
  'multiple_mask_images multiply myanmar n-resize narrower ne-resize '
  'nesw-resize no-close-quote no-drop no-open-quote no-repeat none normal '
  'not-allowed nowrap ns-resize numbers numeric nw-resize nwse-resize oblique '
  'octal opacity open-quote optimizeLegibility optimizeSpeed oriya oromo '
  'outset outside outside-shape overlay overline padding padding-box painted '
  'page paused persian perspective pinch-zoom plus-darker plus-lighter pointer '
  'polygon portrait pre pre-line pre-wrap preserve-3d progress push-button '
  'radial-gradient radio read-only read-write read-write-plaintext-only '
  'rectangle region relative repeat repeating-linear-gradient '
  'repeating-radial-gradient repeating-conic-gradient repeat-x repeat-y reset '
  'reverse rgb rgba ridge right rotate rotate3d rotateX rotateY rotateZ round '
  'row row-resize row-reverse rtl run-in running s-resize sans-serif saturate '
  'saturation scale scale3d scaleX scaleY scaleZ screen scroll scrollbar '
  'scroll-position se-resize searchfield searchfield-cancel-button '
  'searchfield-decoration searchfield-results-button '
  'searchfield-results-decoration self-start self-end semi-condensed '
  'semi-expanded separate sepia serif show sidama simp-chinese-formal '
  'simp-chinese-informal single skew skewX skewY skip-white-space slide '
  'slider-horizontal slider-vertical sliderthumb-horizontal '
  'sliderthumb-vertical slow small small-caps small-caption smaller soft-light '
  'solid somali source-atop source-in source-out source-over space '
  'space-around space-between space-evenly spell-out square square-button '
  'start static status-bar stretch stroke stroke-box sub subpixel-antialiased '
  'svg_masks super sw-resize symbolic symbols system-ui table table-caption '
  'table-cell table-column table-column-group table-footer-group '
  'table-header-group table-row table-row-group tamil telugu text text-bottom '
  'text-top textarea textfield thai thick thin threeddarkshadow threedface '
  'threedhighlight threedlightshadow threedshadow tibetan tigre tigrinya-er '
  'tigrinya-er-abegede tigrinya-et tigrinya-et-abegede to top '
  'trad-chinese-formal trad-chinese-informal transform translate translate3d '
  'translateX translateY translateZ transparent ultra-condensed ultra-expanded '
  'underline unidirectional-pan unset up upper-alpha upper-armenian '
  'upper-greek upper-hexadecimal upper-latin upper-norwegian upper-roman '
  'uppercase urdu url var vertical vertical-text view-box visible visibleFill '
  'visiblePainted visibleStroke visual w-resize wait wave wider window '
  'windowframe windowtext words wrap wrap-reverse x-large x-small xor xx-large '
  'xx-small',
);
final _valueKeywords = _keySet(_valueKeywordList);

/// The upstream `keywords` export: word lists as written, for the modes that
/// read them.
final cssKeywords = (
  properties: _propertyKeywordList,
  colors: _colorKeywordList,
  fonts: _fontPropertyList,
  values: _valueKeywordList,
  all: [
    ..._documentTypeList,
    ..._mediaTypeList,
    ..._mediaFeatureList,
    ..._mediaValueKeywordList,
    ..._propertyKeywordList,
    ..._nonStandardPropertyKeywordList,
    ..._colorKeywordList,
    ..._valueKeywordList,
  ],
);

final _important = RegExp(r'\s*\w*');
final _dashName = RegExp(r'-[\w\\\-]*');
final _colonAhead = RegExp(r'\s*:');
final _vendorPrefix = RegExp(r'\w+-');
final _className = RegExp('-?[_a-z][_a-z0-9-]*', caseSensitive: false);
final _functionName = RegExp(r'[\w\-.]+(?=\()');
final _urlFunction = RegExp(
  r'^(url(-prefix)?|domain|regexp)$',
  caseSensitive: false,
);
final _quoteAhead = RegExp(r'''\s*["')]''');
final _documentAt = RegExp(r'^@(-moz-)?document$', caseSensitive: false);
final _blockAt = RegExp(
  r'^@(media|supports|(-moz-)?document|import)$',
  caseSensitive: false,
);
final _restrictedAt = RegExp(
  '^@(font-face|counter-style)',
  caseSensitive: false,
);
final _keyframesAt = RegExp(
  r'^@(-(moz|ms|o|webkit)-)?keyframes$',
  caseSensitive: false,
);
final _nestedProperty = RegExp(r'\s*:(?:\s|$)');
final _hexColor = RegExp(
  r'^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$',
);
final _braceAhead = RegExp(r'\s*\{');
final _scssVariable = RegExp(r'[\w-]+');
final _lessAtRule = RegExp(
  r'(charset|document|font-face|import|(-(moz|ms|o|webkit)-)?keyframes|media'
  r'|namespace|page|supports)\b',
  caseSensitive: false,
);

/// `\w` for one code unit.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

/// `[\w\\\-]` for one code unit.
bool _isNameUnit(int u) => _isWordUnit(u) || u == 0x5c || u == 0x2d;

/// `[\w.%]` for one code unit.
bool _isUnitUnit(int u) => _isWordUnit(u) || u == 0x2e || u == 0x25;

/// `\d` for one character.
bool _isDigit(String ch) {
  final u = ch.codeUnitAt(0);
  return u >= 0x30 && u <= 0x39;
}

// Tokenizers

_Tokenizer _tokenString(String quote) => (stream, state) {
  var escaped = false;
  String? ch;
  while ((ch = stream.next()) != null) {
    if (ch == quote && !escaped) {
      if (quote == ')') stream.backUp(1);
      break;
    }
    escaped = !escaped && ch == r'\';
  }
  if (ch == quote || !escaped && quote != ')') state._tokenize = null;
  return ('string', 'string');
};

_Token? _tokenParenthesized(StringStream stream, CssState state) {
  stream.next(); // Must be '('
  if (stream.match(_quoteAhead, consume: false) == null) {
    state._tokenize = _tokenString(')');
  } else {
    state._tokenize = null;
  }
  return (null, '(');
}

_Token _tokenCComment(StringStream stream, CssState state) {
  var maybeEnd = false;
  String? ch;
  while ((ch = stream.next()) != null) {
    if (maybeEnd && ch == '/') {
      state._tokenize = null;
      break;
    }
    maybeEnd = ch == '*';
  }
  return ('comment', 'comment');
}

// Token hooks

_Token? _cssSlash(StringStream stream, CssState state) {
  if (stream.eat('*') == null) return null;
  state._tokenize = _tokenCComment;
  return _tokenCComment(stream, state);
}

_Token? _nestedSlash(StringStream stream, CssState state) {
  if (stream.eat('/') != null) {
    stream.skipToEnd();
    return ('comment', 'comment');
  } else if (stream.eat('*') != null) {
    state._tokenize = _tokenCComment;
    return _tokenCComment(stream, state);
  } else {
    return ('operator', 'operator');
  }
}

_Token? _scssColon(StringStream stream, CssState state) {
  if (stream.match(_braceAhead, consume: false) != null) return (null, null);
  return null;
}

_Token? _scssDollar(StringStream stream, CssState state) {
  stream.match(_scssVariable);
  if (stream.match(_colonAhead, consume: false) != null) {
    return ('def', 'variable-definition');
  }
  return ('variableName.special', 'variable');
}

_Token? _scssHash(StringStream stream, CssState state) {
  if (stream.eat('{') == null) return null;
  return (null, 'interpolation');
}

_Token? _lessAt(StringStream stream, CssState state) {
  if (stream.eat('{') != null) return (null, 'interpolation');
  if (stream.match(_lessAtRule, consume: false) != null) return null;
  stream.eatWhileCode(_isNameUnit);
  if (stream.match(_colonAhead, consume: false) != null) {
    return ('def', 'variable-definition');
  }
  return ('variableName', 'variable');
}

_Token? _lessAmpersand(StringStream stream, CssState state) => ('atom', 'atom');

const _cssHooks = <String, _Hook>{'/': _cssSlash};
const _scssHooks = <String, _Hook>{
  '/': _nestedSlash,
  ':': _scssColon,
  r'$': _scssDollar,
  '#': _scssHash,
};
const _lessHooks = <String, _Hook>{
  '/': _nestedSlash,
  '@': _lessAt,
  '&': _lessAmpersand,
};

final class _Context {
  const _Context(this.type, this.indent, this.prev);
  final String type;
  final int indent;
  final _Context? prev;
}

final class CssState {
  CssState._(this._tokenize, this._state, this._stateArg, this._context);
  _Tokenizer? _tokenize;

  /// The name of the parser state that reads the next token.
  String _state;
  String? _stateArg;
  _Context? _context;

  /// The upstream default copy: contexts are shared.
  CssState copy() => CssState._(_tokenize, _state, _stateArg, _context);
}

/// CSS, SCSS and LESS: the upstream `mkCSS` stream parser in one of its
/// configurations.
final class CssMode extends Mode<CssState> {
  CssMode._(super.config, this._tokenHooks, this._allowNested);

  /// `css`.
  CssMode.css([ModeConfig config = const ModeConfig()])
    : this._(config, _cssHooks, false);

  /// `sCSS`: SCSS.
  CssMode.scss([ModeConfig config = const ModeConfig()])
    : this._(config, _scssHooks, true);

  /// `less`: LESS.
  CssMode.less([ModeConfig config = const ModeConfig()])
    : this._(config, _lessHooks, true);

  final Map<String, _Hook> _tokenHooks;
  final bool _allowNested;

  // Upstream's `type`, which stands until a tokenizer gives another, and
  // `override`, the style a parser state may replace.
  String? _type, _override;

  _Token? _tokenBase(StringStream stream, CssState state) {
    final ch = stream.next()!;
    final hook = _tokenHooks[ch];
    if (hook != null) {
      final result = hook(stream, state);
      if (result != null) return result;
    }
    if (ch == '@') {
      stream.eatWhileCode(_isNameUnit);
      return ('def', stream.current());
    } else if (ch == '=' ||
        (ch == '~' || ch == '|') && stream.eat('=') != null) {
      return (null, 'compare');
    } else if (ch == '"' || ch == "'") {
      final tokenize = state._tokenize = _tokenString(ch);
      return tokenize(stream, state);
    } else if (ch == '#') {
      stream.eatWhileCode(_isNameUnit);
      return ('atom', 'hash');
    } else if (ch == '!') {
      stream.match(_important);
      return ('keyword', 'important');
    } else if (_isDigit(ch) || ch == '.' && stream.eat(_isDigit) != null) {
      stream.eatWhileCode(_isUnitUnit);
      return ('number', 'unit');
    } else if (ch == '-') {
      final next = stream.peek();
      if (next != null && (_isDigit(next) || next == '.')) {
        stream.eatWhileCode(_isUnitUnit);
        return ('number', 'unit');
      } else if (stream.match(_dashName) != null) {
        stream.eatWhileCode(_isNameUnit);
        if (stream.match(_colonAhead, consume: false) != null) {
          return ('def', 'variable-definition');
        }
        return ('variableName', 'variable');
      } else if (stream.match(_vendorPrefix) != null) {
        return ('meta', 'meta');
      }
    } else if (',+>*/'.contains(ch)) {
      return (null, 'select-op');
    } else if (ch == '.' && stream.match(_className) != null) {
      return ('qualifier', 'qualifier');
    } else if (':;{}[]()'.contains(ch)) {
      return (null, ch);
    } else if (stream.match(_functionName) != null) {
      if (_urlFunction.hasMatch(stream.current())) {
        state._tokenize = _tokenParenthesized;
      }
      return ('variableName.function', 'variable');
    } else if (_isNameUnit(ch.codeUnitAt(0))) {
      stream.eatWhileCode(_isNameUnit);
      return ('property', 'word');
    } else {
      return (null, null);
    }
    return null;
  }

  // Context management

  String _pushContext(CssState state, StringStream stream, String type) {
    state._context = _Context(
      type,
      stream.indentation() + stream.indentUnit,
      state._context,
    );
    return type;
  }

  String _popContext(CssState state) {
    final context = state._context!;
    if (context.prev != null) state._context = context.prev;
    return state._context!.type;
  }

  String _pass(String? type, StringStream stream, CssState state) =>
      _run(state._context!.type, type, stream, state);

  String _popAndPass(
    String? type,
    StringStream stream,
    CssState state, [
    int n = 1,
  ]) {
    for (var i = n; i > 0; i--) {
      state._context = state._context!.prev;
    }
    return _pass(type, stream, state);
  }

  // Parser

  void _wordAsValue(StringStream stream) {
    final word = stream.current().toLowerCase();
    if (_valueKeywords.contains(word)) {
      _override = 'atom';
    } else if (_colorKeywords.contains(word)) {
      _override = 'keyword';
    } else {
      _override = 'variable';
    }
  }

  /// Upstream's `states[name]`.
  String _run(String name, String? type, StringStream stream, CssState state) =>
      switch (name) {
        'top' => _top(type, stream, state),
        'block' => _block(type, stream, state),
        'maybeprop' => _maybeprop(type, stream, state),
        'prop' => _prop(type, stream, state),
        'propBlock' => _propBlock(type, stream, state),
        'parens' => _parens(type, stream, state),
        'pseudo' => _pseudo(type, stream, state),
        'documentTypes' => _documentTypesState(type, stream, state),
        'atBlock' => _atBlock(type, stream, state),
        'atBlock_parens' => _atBlockParens(type, stream, state),
        'restricted_atBlock_before' => _restrictedAtBlockBefore(
          type,
          stream,
          state,
        ),
        'restricted_atBlock' => _restrictedAtBlock(type, stream, state),
        'keyframes' => _keyframes(type, stream, state),
        'at' => _at(type, stream, state),
        'interpolation' => _interpolation(type, stream, state),
        _ => throw StateError('No parser state $name'),
      };

  String _top(String? type, StringStream stream, CssState state) {
    if (type == '{') {
      return _pushContext(state, stream, 'block');
    } else if (type == '}' && state._context!.prev != null) {
      return _popContext(state);
    } else if (type != null && _documentAt.hasMatch(type)) {
      return _pushContext(state, stream, 'documentTypes');
    } else if (type != null && _blockAt.hasMatch(type)) {
      return _pushContext(state, stream, 'atBlock');
    } else if (type != null && _restrictedAt.hasMatch(type)) {
      state._stateArg = type;
      return 'restricted_atBlock_before';
    } else if (type != null && _keyframesAt.hasMatch(type)) {
      return 'keyframes';
    } else if (type != null && type.startsWith('@')) {
      return _pushContext(state, stream, 'at');
    } else if (type == 'hash') {
      _override = 'builtin';
    } else if (type == 'word') {
      _override = 'tag';
    } else if (type == 'variable-definition') {
      return 'maybeprop';
    } else if (type == 'interpolation') {
      return _pushContext(state, stream, 'interpolation');
    } else if (type == ':') {
      return 'pseudo';
    } else if (_allowNested && type == '(') {
      return _pushContext(state, stream, 'parens');
    }
    return state._context!.type;
  }

  String _block(String? type, StringStream stream, CssState state) {
    if (type == 'word') {
      final word = stream.current().toLowerCase();
      if (_propertyKeywords.contains(word)) {
        _override = 'property';
        return 'maybeprop';
      } else if (_nonStandardPropertyKeywords.contains(word)) {
        _override = 'string.special';
        return 'maybeprop';
      } else if (_allowNested) {
        _override = stream.match(_nestedProperty, consume: false) != null
            ? 'property'
            : 'tag';
        return 'block';
      } else {
        _override = 'error';
        return 'maybeprop';
      }
    } else if (type == 'meta') {
      return 'block';
    } else if (!_allowNested && (type == 'hash' || type == 'qualifier')) {
      _override = 'error';
      return 'block';
    } else {
      return _top(type, stream, state);
    }
  }

  String _maybeprop(String? type, StringStream stream, CssState state) {
    if (type == ':') return _pushContext(state, stream, 'prop');
    return _pass(type, stream, state);
  }

  String _prop(String? type, StringStream stream, CssState state) {
    if (type == ';') return _popContext(state);
    if (type == '{' && _allowNested) {
      return _pushContext(state, stream, 'propBlock');
    }
    if (type == '}' || type == '{') return _popAndPass(type, stream, state);
    if (type == '(') return _pushContext(state, stream, 'parens');

    if (type == 'hash' && !_hexColor.hasMatch(stream.current())) {
      _override = 'error';
    } else if (type == 'word') {
      _wordAsValue(stream);
    } else if (type == 'interpolation') {
      return _pushContext(state, stream, 'interpolation');
    }
    return 'prop';
  }

  String _propBlock(String? type, StringStream stream, CssState state) {
    if (type == '}') return _popContext(state);
    if (type == 'word') {
      _override = 'property';
      return 'maybeprop';
    }
    return state._context!.type;
  }

  String _parens(String? type, StringStream stream, CssState state) {
    if (type == '{' || type == '}') return _popAndPass(type, stream, state);
    if (type == ')') return _popContext(state);
    if (type == '(') return _pushContext(state, stream, 'parens');
    if (type == 'interpolation') {
      return _pushContext(state, stream, 'interpolation');
    }
    if (type == 'word') _wordAsValue(stream);
    return 'parens';
  }

  String _pseudo(String? type, StringStream stream, CssState state) {
    if (type == 'meta') return 'pseudo';

    if (type == 'word') {
      _override = 'variableName.constant';
      return state._context!.type;
    }
    return _pass(type, stream, state);
  }

  String _documentTypesState(
    String? type,
    StringStream stream,
    CssState state,
  ) {
    if (type == 'word' && _documentTypes.contains(stream.current())) {
      _override = 'tag';
      return state._context!.type;
    } else {
      return _atBlock(type, stream, state);
    }
  }

  String _atBlock(String? type, StringStream stream, CssState state) {
    if (type == '(') return _pushContext(state, stream, 'atBlock_parens');
    if (type == '}' || type == ';') return _popAndPass(type, stream, state);
    if (type == '{') {
      _popContext(state);
      return _pushContext(state, stream, _allowNested ? 'block' : 'top');
    }

    if (type == 'interpolation') {
      return _pushContext(state, stream, 'interpolation');
    }

    if (type == 'word') {
      final word = stream.current().toLowerCase();
      if (word == 'only' || word == 'not' || word == 'and' || word == 'or') {
        _override = 'keyword';
      } else if (_mediaTypes.contains(word)) {
        _override = 'attribute';
      } else if (_mediaFeatures.contains(word)) {
        _override = 'property';
      } else if (_mediaValueKeywords.contains(word)) {
        _override = 'keyword';
      } else if (_propertyKeywords.contains(word)) {
        _override = 'property';
      } else if (_nonStandardPropertyKeywords.contains(word)) {
        _override = 'string.special';
      } else if (_valueKeywords.contains(word)) {
        _override = 'atom';
      } else if (_colorKeywords.contains(word)) {
        _override = 'keyword';
      } else {
        _override = 'error';
      }
    }
    return state._context!.type;
  }

  String _atBlockParens(String? type, StringStream stream, CssState state) {
    if (type == ')') return _popContext(state);
    if (type == '{' || type == '}') {
      return _popAndPass(type, stream, state, 2);
    }
    return _atBlock(type, stream, state);
  }

  String _restrictedAtBlockBefore(
    String? type,
    StringStream stream,
    CssState state,
  ) {
    if (type == '{') return _pushContext(state, stream, 'restricted_atBlock');
    if (type == 'word' && state._stateArg == '@counter-style') {
      _override = 'variable';
      return 'restricted_atBlock_before';
    }
    return _pass(type, stream, state);
  }

  String _restrictedAtBlock(String? type, StringStream stream, CssState state) {
    if (type == '}') {
      state._stateArg = null;
      return _popContext(state);
    }
    if (type == 'word') {
      final word = stream.current().toLowerCase();
      if ((state._stateArg == '@font-face' &&
              !_fontProperties.contains(word)) ||
          (state._stateArg == '@counter-style' &&
              !_counterDescriptors.contains(word))) {
        _override = 'error';
      } else {
        _override = 'property';
      }
      return 'maybeprop';
    }
    return 'restricted_atBlock';
  }

  String _keyframes(String? type, StringStream stream, CssState state) {
    if (type == 'word') {
      _override = 'variable';
      return 'keyframes';
    }
    if (type == '{') return _pushContext(state, stream, 'top');
    return _pass(type, stream, state);
  }

  String _at(String? type, StringStream stream, CssState state) {
    if (type == ';') return _popContext(state);
    if (type == '{' || type == '}') return _popAndPass(type, stream, state);
    if (type == 'word') {
      _override = 'tag';
    } else if (type == 'hash') {
      _override = 'builtin';
    }
    return 'at';
  }

  String _interpolation(String? type, StringStream stream, CssState state) {
    if (type == '}') return _popContext(state);
    if (type == '{' || type == ';') return _popAndPass(type, stream, state);
    if (type == 'word') {
      _override = 'variable';
    } else if (type != 'variable' && type != '(' && type != ')') {
      _override = 'error';
    }
    return 'interpolation';
  }

  /// [baseColumn] indents the top context, as CodeMirror 5's css.js takes it
  /// from a mode that embeds CSS; CodeMirror 6's takes none, which is 0.
  @override
  CssState startState([int baseColumn = 0]) =>
      CssState._(null, 'top', null, _Context('top', baseColumn, null));

  @override
  CssState copyState(CssState state) => state.copy();

  @override
  String? token(StringStream stream, CssState state) {
    final tokenize = state._tokenize;
    if (tokenize == null && stream.eatSpace()) return null;
    final result = tokenize != null
        ? tokenize(stream, state)
        : _tokenBase(stream, state);
    String? style;
    if (result != null) {
      _type = result.$2;
      style = result.$1;
    }
    _override = style;
    if (_type != 'comment') {
      state._state = _run(state._state, _type, stream, state);
    }
    return _override;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(CssState state, String textAfter, String line) {
    var cx = state._context!;
    final ch = textAfter.isEmpty ? '' : textAfter[0];
    var indent = cx.indent;
    if (cx.type == 'prop' && (ch == '}' || ch == ')')) cx = cx.prev!;
    if (cx.prev != null) {
      if (ch == '}' &&
          (cx.type == 'block' ||
              cx.type == 'top' ||
              cx.type == 'interpolation' ||
              cx.type == 'restricted_atBlock')) {
        // Resume indentation from parent context.
        cx = cx.prev!;
        indent = cx.indent;
      } else if (ch == ')' &&
              (cx.type == 'parens' || cx.type == 'atBlock_parens') ||
          ch == '{' && (cx.type == 'at' || cx.type == 'atBlock')) {
        // Dedent relative to current context.
        indent = math.max(0, cx.indent - config.indentUnit);
      }
    }
    return indent;
  }

  @override
  RegExp get electricInput => _electric;
  static final _electric = RegExp(r'^\s*\}$');
}
