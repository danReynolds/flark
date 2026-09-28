/// Evidence that an untagged fence is in a language: a pattern each of whose
/// first [cap] matches is worth [weight], which counts against the language
/// when negative. Patterns match across the whole sample, with `^` and `$`
/// at line ends.
final class DetectionSign {
  DetectionSign(
    String pattern,
    this.weight, {
    this.cap = 3,
    this.caseSensitive = true,
  }) : pattern = RegExp(pattern, multiLine: true, caseSensitive: caseSensitive),
       _lineStart = _alternativesOf(pattern).every((a) => a.startsWith('^')),
       _starts = _startTokens(pattern, caseSensitive);
  final RegExp pattern;
  final double weight;
  final int cap;
  final bool caseSensitive;

  /// Whether every match starts a line.
  final bool _lineStart;

  /// The tokens a match can start with: words, or single other characters.
  /// A sample without any has no match, and a line that starts with none
  /// does not start one. Null when the pattern does not tell.
  final Set<String>? _starts;

  /// For tuning: the tokens a match can start with.
  Set<String>? get starts => _starts;

  /// The evidence in [sample].
  double score(DetectionSample sample) {
    final count = sample._count(this);
    return (count < cap ? count : cap) * weight;
  }

  int _countIn(DetectionSample sample) {
    final starts = _starts;
    if (starts != null && _lineStart) {
      var n = 0;
      for (final token in starts) {
        for (final at in sample._linesStartingWith(token, caseSensitive)) {
          if (pattern.matchAsPrefix(sample.text, at) != null && ++n == 8) {
            return n;
          }
        }
      }
      return n;
    }
    if (_lineStart) {
      var n = 0;
      for (final at in sample._lineStarts) {
        if (pattern.matchAsPrefix(sample.text, at) != null && ++n == 8) break;
      }
      return n;
    }
    if (starts != null && !starts.any((t) => sample._has(t, caseSensitive))) {
      return 0;
    }
    var n = 0;
    for (final _ in pattern.allMatches(sample.text)) {
      if (++n == 8) break;
    }
    return n;
  }
}

/// A fence's text as detection reads it: its words and other characters,
/// and each line's first, so that signs run only where they can match.
final class DetectionSample {
  DetectionSample(this.text) {
    final s = text;
    var i = 0;
    var lineStart = true;
    _lineStarts.add(0);
    while (i < s.length) {
      final c = s.codeUnitAt(i);
      if (c == 0x0a) {
        lineStart = true;
        i++;
        if (i < s.length) _lineStarts.add(i);
        continue;
      }
      if (c == 0x20 || c == 0x09 || c == 0x0d) {
        i++;
        continue;
      }
      final start = i;
      if (_isWordUnit(c)) {
        while (i < s.length && _isWordUnit(s.codeUnitAt(i))) {
          i++;
        }
      } else {
        i++;
      }
      final token = s.substring(start, i);
      _tokens.add(token);
      if (lineStart) {
        // The offset where the line starts, before its indentation.
        var at = start;
        while (at > 0 && s.codeUnitAt(at - 1) != 0x0a) {
          at--;
        }
        (_heads[token] ??= []).add(at);
        lineStart = false;
      }
    }
  }

  final String text;
  final _lineStarts = <int>[];
  final _tokens = <String>{};
  final _heads = <String, List<int>>{};
  Set<String>? _lowerTokens;
  Map<String, List<int>>? _lowerHeads;
  final _counts = <(String, bool), int>{};

  bool _has(String token, bool caseSensitive) => caseSensitive
      ? _tokens.contains(token)
      : (_lowerTokens ??= {
          for (final t in _tokens) t.toLowerCase(),
        }).contains(token);

  List<int> _linesStartingWith(String token, bool caseSensitive) {
    if (caseSensitive) return _heads[token] ?? const [];
    final heads = _lowerHeads ??= () {
      final lower = <String, List<int>>{};
      for (final MapEntry(:key, :value) in _heads.entries) {
        (lower[key.toLowerCase()] ??= []).addAll(value);
      }
      return lower;
    }();
    return heads[token] ?? const [];
  }

  /// Matches of [sign]'s pattern, up to eight, counted once per sample for
  /// every sign with the same pattern.
  int _count(DetectionSign sign) =>
      _counts[(sign.pattern.pattern, sign.caseSensitive)] ??= sign._countIn(
        this,
      );
}

bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

/// The tokens a match of [pattern] can start with, read from its opening:
/// after a line start, indentation, word boundaries and lookarounds, a
/// literal word or character, or a group of alternatives that each start
/// with one, with optional parts before them. Null when an alternative
/// starts otherwise, or a word could continue into more word characters.
Set<String>? _startTokens(String pattern, bool caseSensitive) {
  final tokens = <String>{};
  for (final alternative in _alternativesOf(pattern)) {
    final one = _Starts(alternative).read();
    if (one == null || one.isEmpty) return null;
    tokens.addAll(one);
  }
  return caseSensitive ? tokens : {for (final t in tokens) t.toLowerCase()};
}

/// [pattern]'s top-level alternatives.
List<String> _alternativesOf(String pattern) =>
    _Starts(pattern)._split(pattern);

final class _Starts {
  _Starts(this.p);
  final String p;
  var i = 0;

  /// Whether a token boundary precedes [i]: a line start or `\b`. A word
  /// found elsewhere may be the end of a longer token.
  var _bounded = false;

  static const _escapedLiterals = r'.$()[]{}?*+|/\-^';
  static final _plain = RegExp(r'[A-Za-z0-9_ :=<>!&@#%,;"\x27/`~-]');

  /// The start tokens of the sequence from [i] to the end of the current
  /// group or alternative.
  Set<String>? read() {
    final out = <String>{};
    for (;;) {
      if (i >= p.length || p[i] == '|' || p[i] == ')') return null;
      // Zero-width openings.
      if (p.startsWith('^', i)) {
        i++;
        _bounded = true;
        continue;
      }
      if (p.startsWith(r'[ \t]*', i) && _bounded) {
        i += 6;
        continue;
      }
      if (p.startsWith(r'\b', i)) {
        i += 2;
        _bounded = true;
        continue;
      }
      if (p.startsWith('(?<!', i) ||
          p.startsWith('(?<=', i) ||
          p.startsWith('(?!', i) ||
          p.startsWith('(?=', i)) {
        _skipGroup();
        _bounded = false;
        continue;
      }
      if (p.startsWith('(', i)) {
        final groupStart = i;
        final alternatives = _alternatives();
        if (alternatives == null) return null;
        if (_optional()) {
          // Either the group or what follows it starts the match.
          out.addAll(alternatives);
          continue;
        }
        i = groupStart;
        return out..addAll(alternatives);
      }
      if (p.startsWith('[', i) && !p.startsWith('[ \\t]', i)) {
        final close = _classEnd(i);
        final body = p.substring(i + 1, close);
        final optional = close + 1 < p.length && '?*'.contains(p[close + 1]);
        if (optional ||
            body.startsWith('^') ||
            body.contains('-') ||
            _classMayHoldWord(body) ||
            body.contains(r'\')) {
          return null;
        }
        return out..addAll(body.split(''));
      }
      final words = _word();
      if (words == null || words.isEmpty) return null;
      return out..addAll(words);
    }
  }

  /// Reads the group at [i], leaving [i] after it and any quantifier; the
  /// start tokens of each alternative, or null.
  Set<String>? _alternatives() {
    i += p.startsWith('(?:', i) ? 3 : 1;
    final all = <String>{};
    for (;;) {
      final starts = _Starts(p)
        ..i = i
        .._bounded = _bounded;
      final one = starts.read();
      if (one == null) return null;
      all.addAll(one);
      // Skip to the end of this alternative.
      i = _endOfAlternative(i);
      if (i >= p.length) return null;
      if (p[i] == ')') {
        i++;
        return all;
      }
      i++; // '|'
    }
  }

  bool _optional() {
    if (i < p.length && (p[i] == '?' || p[i] == '*')) {
      i++;
      return true;
    }
    if (p.startsWith('{0,', i)) {
      i = p.indexOf('}', i) + 1;
      return true;
    }
    return false;
  }

  void _skipGroup() {
    i = _endOfAlternative(i + 1);
    while (i < p.length && p[i] == '|') {
      i = _endOfAlternative(i + 1);
    }
    i++;
  }

  /// The index of the `|` or `)` that ends the alternative at [from].
  int _endOfAlternative(int from) {
    var depth = 0;
    var inClass = false;
    for (var j = from; j < p.length; j++) {
      final c = p[j];
      if (c == r'\') {
        j++;
      } else if (inClass) {
        if (c == ']') inClass = false;
      } else if (c == '[') {
        inClass = true;
      } else if (c == '(') {
        depth++;
      } else if (c == ')') {
        if (depth == 0) return j;
        depth--;
      } else if (c == '|' && depth == 0) {
        return j;
      }
    }
    return p.length;
  }

  /// The literal tokens at [i]: a whole word, perhaps with an optional last
  /// character or suffix, or one other character. Null when none is there
  /// or the word may run on into characters the pattern does not spell.
  Set<String>? _word() {
    final buffer = StringBuffer();
    var j = i;
    while (j < p.length) {
      String unit;
      var next = j + 1;
      if (p[j] == r'\' &&
          j + 1 < p.length &&
          _escapedLiterals.contains(p[j + 1])) {
        unit = p[j + 1];
        next = j + 2;
      } else if (p[j] != r'\' && _plain.hasMatch(p[j]) && p[j] != ' ') {
        unit = p[j];
      } else {
        break;
      }
      final isWord = _isWordUnit(unit.codeUnitAt(0));
      final optional = next < p.length && '?*{'.contains(p[next]);
      if (buffer.isEmpty && !isWord) return optional ? null : {unit};
      if (!isWord) break;
      if (!_bounded) return null;
      if (next < p.length && p[next] == '?' && buffer.isNotEmpty) {
        // `pip3?`: the word with or without its last character.
        final base = buffer.toString();
        i = next + 1;
        final after = _ends(i);
        if (after == null) return null;
        return {
          for (final end in after) ...{'$base$end', '$base$unit$end'},
        };
      }
      if (optional || next < p.length && p[next] == '+') return null;
      buffer.write(unit);
      j = next;
    }
    if (buffer.isEmpty) return null;
    final base = buffer.toString();
    final after = _ends(j);
    if (after == null) return null;
    return {for (final end in after) '$base$end'};
  }

  /// How a word read up to [j] may end: with nothing more (''), or with one
  /// of the suffixes a following group spells. Null when it may run on.
  Set<String>? _ends(int j) {
    if (j >= p.length) return {''};
    final rest = p.substring(j);
    if (rest.startsWith(r'\w') ||
        rest.startsWith(r'\d') ||
        rest.startsWith('.')) {
      return null;
    }
    if (rest.startsWith('[')) {
      final close = _classEnd(j);
      return _classMayHoldWord(p.substring(j + 1, close)) ? null : {''};
    }
    if (rest.startsWith('(?=') || rest.startsWith('(?!')) return {''};
    if (rest.startsWith('(')) {
      // A group of plain words continues the word; one whose alternatives
      // start with other characters ends it.
      final open = rest.startsWith('(?:') ? j + 3 : j + 1;
      final close = _groupEnd(open);
      final body = p.substring(open, close);
      final optional = close + 1 < p.length && p[close + 1] == '?';
      final alternatives = _split(body);
      if (alternatives.every((a) => RegExp(r'^[A-Za-z0-9_]+$').hasMatch(a))) {
        return {...alternatives, if (optional) ''};
      }
      if (alternatives.every(
        (a) =>
            a.isNotEmpty && !RegExp(r'^[A-Za-z0-9_]|^\\[wdb]|^\[').hasMatch(a),
      )) {
        return {''};
      }
      return null;
    }
    return {''};
  }

  int _classEnd(int j) {
    for (var k = j + 1; k < p.length; k++) {
      if (p[k] == r'\') {
        k++;
      } else if (p[k] == ']') {
        return k;
      }
    }
    return p.length;
  }

  int _groupEnd(int from) {
    var at = from;
    for (;;) {
      at = _endOfAlternative(at);
      if (at >= p.length || p[at] == ')') return at;
      at++;
    }
  }

  List<String> _split(String body) {
    final parts = <String>[];
    var depth = 0, from = 0;
    var inClass = false;
    for (var k = 0; k < body.length; k++) {
      final c = body[k];
      if (c == r'\') {
        k++;
      } else if (inClass) {
        if (c == ']') inClass = false;
      } else if (c == '[') {
        inClass = true;
      } else if (c == '(') {
        depth++;
      } else if (c == ')') {
        depth--;
      } else if (c == '|' && depth == 0) {
        parts.add(body.substring(from, k));
        from = k + 1;
      }
    }
    return parts..add(body.substring(from));
  }

  static bool _classMayHoldWord(String body) =>
      body.startsWith('^') ||
      RegExp(r'\\[wdWDS]|[A-Za-z0-9_]').hasMatch(
        body.replaceAll(
          RegExp(r'\\x[0-9a-fA-F]{2}|\\[tnsr"\x27\\/.$()\[\]{}?*+|^-]'),
          '',
        ),
      );
}

DetectionSign _s(String pattern, double weight, {int cap = 3}) =>
    DetectionSign(pattern, weight, cap: cap);

DetectionSign _i(String pattern, double weight, {int cap = 3}) =>
    DetectionSign(pattern, weight, cap: cap, caseSensitive: false);

// Semicolon-terminated lines and brace blocks: C-like code, not Python, YAML
// or markup.
const _endsInSemicolon = r';[ \t]*(?://.*)?$';
const _opensBrace = r'\{[ \t]*$';

final javascriptSigns = [
  _s(r'^[ \t]*(?:export\s+)?(?:async\s+)?function\b\s*\*?\s*[\w$]*\s*\(', 2.5),
  _s(
    r'\.(?:map|filter|reduce|forEach|find|some|every|flatMap)\(\s*'
    r'(?:\(?[\w$, ]*\)?\s*=>|function\b)',
    2.5,
  ),
  _s(r'^[ \t]*#[A-Za-z_$][\w$]*\s*=\s*\S', 2),
  _s(r'\b(?:const|let)\s+[\w$]+\s*=', 1),
  _s(r'\b(?:const|let)\s+[{\[][\w$\s,:]*[}\]]\s*=', 2),
  _s(r'(?<=[)\w$][ \t]*)=>[ \t]*[{(\w$]', 0.5),
  _s(r'===|!==', 1.5),
  _s(r'\bconsole\.(?:log|error|warn|info|debug)\(', 3),
  _s(r'\brequire\(\s*[\x27"`]', 2),
  _s(r'\bmodule\.exports\b|\bexports\.[\w$]+\s*=', 3),
  _s(
    r'^[ \t]*import\s+(?:[\w$*{][^;\n]*?\s+from\s+)?[\x27"]'
    r'(?![^\x27"\n]*\.dart[\x27"])[^\x27"\n]+[\x27"];?[ \t]*$',
    2,
  ),
  _s(r'^[ \t]*export\s+(?:default|const|function|class|async|let|\{|\*)', 2),
  _s(r'\b(?:document|window|navigator)\.\w+', 2),
  _s(
    r'\bJSON\.(?:parse|stringify)\(|\.addEventListener\(|'
    r'\.querySelector(?:All)?\(',
    3,
  ),
  _s(r'\.then\(\s*(?:\(|function|[\w$]+\s*=>)', 2),
  _s(r'`[^`\n]*\$\{', 2),
  _s(r'\bundefined\b', 1.5),
  _s(r'\bnew\s+(?:Promise|Map|Set|Error|Date|RegExp)\b', 1.5),
  _s(r'\b(?:async|await)\s+[\w$(]', 0.5),
  _s(r'\bthis\.[\w$]+', 0.3),
  _s(r'\bclassName=|\breturn\s*\([ \t]*$', 1.5),
  _s(_endsInSemicolon, 0.2, cap: 5),
  _s(_opensBrace, 0.2, cap: 5),
];

final typescriptSigns = [
  _s(
    r'(?<=[\w$)?][ \t]*):[ \t]*(?:string|number|boolean|any|void|unknown|'
    r'never|object|bigint)(?:\[\])?(?=[ \t]*[;,)=|&\]>{}])',
    2,
  ),
  _s(
    r'^[ \t]*(?:export\s+)?(?:declare\s+)?interface\s+\w+(?:<[^>\n]*>)?\s*'
    r'(?:extends\s+[\w<>, .]+)?\s*\{',
    2,
  ),
  _s(r'^[ \t]*(?:export\s+)?(?:declare\s+)?type\s+\w+(?:<[^>\n]*>)?\s*=', 2.5),
  _s(r'\b(?:private|public|protected|readonly)\s+[\w$]+\s*[?!]?\s*:', 1.5),
  _s(r'\bas\s+(?:const|string|number|any|unknown)\b', 2),
  _s(r'(?<=[\w$])<(?:string|number|boolean|any|unknown|void)(?:\[\])?>', 2),
  _s(r'\)\s*:\s*(?:Promise<|void\b|string\b|number\b|boolean\b)', 2.5),
  _s(r'^[ \t]*(?:export\s+)?(?:declare|namespace|abstract\s+class)\s+\w+', 1),
  _s(r'^[ \t]*(?:import|export)\s+type\s', 3),
  _s(r'\b(?:const|let|var)\s+[\w$]+\s*:\s*[\w$.<>\[\]| ]+?\s*=', 1.5),
];

final pythonSigns = [
  _s(
    r'^[ \t]*(?:async\s+)?def\s+\w+\s*\(.*\)\s*(?:->\s*[^:\n]+)?:[ \t]*'
    r'(?:#.*)?$',
    4,
  ),
  _s(r'^[ \t]*class\s+\w+(?:\([^)\n]*\))?:[ \t]*$', 4),
  _s(r'^[ \t]*from\s+\.*[\w.]*\s+import\s', 4),
  _s(r'^[ \t]*import\s+\w+(?:\s*,\s*\w+)*[ \t]*$', 1),
  _s(r'^[ \t]*(?:elif\b.*|else|try|finally|except\b.*):[ \t]*$', 3),
  _s(r'^[ \t]*(?:if|for|while|with)\b.*:[ \t]*(?:#.*)?$', 1),
  _s(r'\bself\.\w+', 0.5),
  _s(r'\bNone\b', 1),
  _s(r'\b(?:True|False)\b', 0.5),
  _s(r'\b__\w+__\b', 2),
  _s(r'\blambda\s+\w*\s*:', 2),
  _s(r'\bf["\x27][^"\x27\n]*\{', 2),
  _s(r'\b(?:print|len|range|isinstance|enumerate)\(', 0.5),
  _s(r'^[ \t]*(?:"""|\x27\x27\x27)', 1),
  _s(r'\b(?:not in|is not)\b', 1),
  _s(_endsInSemicolon, -0.5, cap: 4),
  _s(_opensBrace, -0.5, cap: 4),
  _s(r'^[ \t]*\}', -0.5, cap: 4),
];

final rubySigns = [
  _s(r'^[ \t]*def\s+(?:self\.)?\w+[?!=]?(?:\s*\(.*\))?[ \t]*$', 2.5),
  _s(r'^[ \t]*end[ \t]*$', 1.5),
  _s(
    r'^[ \t]*(?:module|class)\s+[A-Z]\w*(?:::[A-Z]\w*)*'
    r'(?:\s*<\s*[A-Z][\w:]*)?[ \t]*$',
    2,
  ),
  _s(r'\b(?:do|\{)\s*\|[^|\n]*\|', 3),
  _s(r'(?<=[\w)]\.\w+[?!]?(?:\([^)\n]*\))?[ \t]+)\bdo[ \t]*$', 2),
  _s(r'\battr_(?:reader|writer|accessor)\b', 4),
  _s(r'^[ \t]*require(?:_relative)?\s+[\x27"]', 3),
  _s(r'\bputs\s+["\x27#@\w]', 1.5),
  _s(r'@[a-z_]\w*\s*(?:[-+*/|&]?=[^=~>]|\.\w|\[)', 1.5),
  _s(r'(?<=^|[\s(,{\[]):[a-z_]\w*\b', 0.5),
  _s(r'\.each(?:_\w+)?\b', 1),
  _s(r'\belsif\b|^[ \t]*unless\s', 3),
  _s(r'(?<="[^"\n]*)#\{', 1.5),
  _s(r'\bnil\b', 0.8),
  _s(_endsInSemicolon, -0.3, cap: 4),
];

final bashSigns = [
  _s(r'^#!.*\b(?:ba|z|k|da)?sh\b', 6, cap: 1),
  _s(r'^[ \t]*(?:if|elif|while)\s+\[\[?\s', 3),
  _s(r';\s*then\b|^[ \t]*then[ \t]*$', 3),
  _s(r'^[ \t]*(?:fi|done|esac)[ \t]*$', 3),
  _s(r';\s*do[ \t]*$', 3),
  _s(r'\$\{?[A-Z_][A-Z0-9_]*\}?', 0.5),
  _s(r'"\$\{?\w+\}?"', 1.5),
  _s(r'\$\(\s*[\w-]', 1),
  _s(r'^[ \t]*(?:export|local|readonly|declare)\s+[A-Za-z_]\w*=', 3),
  _s(r'^[ \t]*[A-Za-z_]\w*=(?:["\x27$(]|[\w./:-]+[ \t]*$)', 1),
  _s(
    r'^[ \t]*(?:sudo\s+)?(?:echo|printf|cd|mkdir|rm|cp|mv|chmod|chown|grep|'
    r'sed|awk|curl|wget|export|source|apt(?:-get)?|brew|npm|npx|yarn|pnpm|'
    r'pip3?|git|docker|kubectl|cargo|go|make|cat|ls|tar|ssh|dart|flutter)\s',
    1.5,
  ),
  _s(r'^[ \t]*\$\s+\w', 3),
  _s(r'2>&1|>\s*/dev/null|\|\s*(?:grep|xargs|sort|head|tail|wc|awk|sed)\b', 2),
  _s(r'^[ \t]*\w+\s*\(\)\s*\{', 2),
  _s(r'(?<=\s)(?:&&|\|\|)(?=\s)', 0.3),
  _s(_endsInSemicolon, -0.3, cap: 4),
];

final powershellSigns = [
  _s(
    r'\b(?:Get|Set|New|Remove|Add|Clear|Write|Read|Invoke|Start|Stop|Test|'
    r'Import|Export|Select|Where|ForEach|Sort|Group|Measure|Out|Format|'
    r'Convert|ConvertTo|ConvertFrom|Join|Split|Copy|Move|Rename|Resolve|'
    r'Register|Update|Install|Uninstall|Enable|Disable)-[A-Z][A-Za-z]+',
    3,
  ),
  _s(
    r'(?<=\s)-(?:eq|ne|gt|ge|lt|le|like|notlike|match|notmatch|contains|'
    r'notcontains|and|or|not|join|split|replace|in|notin)(?=\s)',
    2,
  ),
  _i(r'\$(?:true|false|null|_|PSItem|env:\w+|args|this)\b', 1.5),
  _s(
    r'\[(?:string|int|bool|array|hashtable|PSCustomObject|switch|Parameter|'
    r'CmdletBinding|ValidateNotNullOrEmpty)(?:\[\])?\]',
    3,
  ),
  _i(r'^[ \t]*param\s*\(', 3),
  _s(r'@\{', 1),
  _s(r'\$[A-Za-z_]\w*\s*=', 0.3),
  _s(r'(?<="[^"\n]*)`[nrt0"$]', 1),
];

final phpSigns = [
  _s(r'<\?php\b|<\?=', 10, cap: 1),
  _s(r'\$[a-z_]\w*\s*(?:=[^=>]|\[)', 0.5),
  _s(r'\$this->', 4),
  _s(r'\$\w+->\w+', 1.5),
  _s(r'\b(?:public|private|protected)\s+(?:static\s+)?function\b', 4),
  _s(r'\bfunction\s+\w+\s*\([^)\n]*\$\w+', 4),
  _s(r'^[ \t]*(?:namespace|use)\s+[A-Z]\w*(?:\\\w+)+', 4),
  _s(r'\barray\s*\(', 2),
  _s(r'::class\b|\bnew\s+static\b', 1.5),
  _s(r'\becho\s+[\x27"$]', 1),
  _s(r'\bforeach\s*\(\s*\$\w+\s+as\s+\$', 4),
  _s(r'\b(?:isset|empty|unset)\(\$', 3),
];

final goSigns = [
  _s(r'^package\s+[a-z_]\w*[ \t]*$', 4, cap: 1),
  _s(r'^[ \t]*func\s+(?:\([^)\n]*\)\s*)?\w+\s*\(', 4),
  _s(r'(?<=\w[ \t]*):=[ \t]*\S', 1.5),
  _s(r'\berr\s*!=\s*nil\b', 5),
  _s(r'\bfmt\.\w+\(', 4),
  _s(r'^[ \t]*import\s+(?:\([ \t]*$|"[\w./-]+"[ \t]*$)', 3),
  _s(r'\bchan\s+\w|\bgo\s+func\b|^[ \t]*defer\s', 2),
  _s(r'^[ \t]*type\s+\w+\s+(?:struct|interface)\s*\{', 5),
  _s(r'\[\]\w+\{|\bmap\[\w+\]\w+|\[\](?:string|int|byte)\b', 3),
  _s(r'\bnil\b', 0.5),
  _s(_endsInSemicolon, -0.3, cap: 4),
];

final rustSigns = [
  _s(r'\bfn\s+\w+\s*(?:<[^>\n]*>)?\s*\(', 4),
  _s(r'\blet\s+mut\s', 4),
  _s(r'^[ \t]*impl(?:<[^>\n]*>)?\s+[\w:<>, ]+(?:\s+for\s+[\w:<>]+)?\s*\{', 4),
  _s(
    r'\bpub(?:\([\w ]+\))?\s+(?:fn|struct|enum|trait|mod|use|const|type|'
    r'async|crate)\b',
    4,
  ),
  _s(r'^[ \t]*use\s+[\w:]+(?:::\{[^}\n]*\})?;', 4),
  _s(r'\b(?:std|core|alloc)::\w+::|\b(?:crate|super|self)::', 2),
  _s(r'(?<=\b[a-z_]+)![ \t]*[(\[{]', 2),
  _s(
    r'->\s*(?:Self|Result|Option|Vec|String|bool|u\d+|i\d+|usize|&|impl\b)',
    3,
  ),
  _s(r'&(?:mut\s+)?self\b|&\x27\w+|\b(?:Some|Ok|Err)\(', 2.5),
  _s(r'#!?\[\w+', 3),
  _s(r'\bmatch\s+[\w.&*]+\s*\{', 2),
  _s(r'\b(?:u8|u16|u32|u64|usize|i32|i64|f64|&str)\b', 1.5),
  _s(r'::new\(|\bSelf\b', 1),
];

final javaSigns = [
  _s(r'^[ \t]*package\s+[\w.]+;', 4, cap: 1),
  _s(r'^[ \t]*import\s+(?:static\s+)?[\w.]+(?:\.\*)?;', 3),
  _s(
    r'\b(?:public|private|protected)\s+(?:(?:static|final|abstract|sealed)\s+)*'
    r'(?:class|interface|enum|record)\s+\w+',
    1.5,
  ),
  _s(r'\bpublic\s+static\s+void\s+main\s*\(', 6),
  _s(r'\bSystem\.(?:out|err)\.print', 6),
  _s(
    r'@(?:Override|FunctionalInterface|Autowired|Test|Deprecated|'
    r'SuppressWarnings|Nullable|NonNull)\b',
    3,
  ),
  _s(
    r'\b(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?'
    r'[A-Z]\w*(?:<[^>\n]*>)?\s+\w+\s*[=;(]',
    1.5,
  ),
  _s(r'\bthrows\s+\w+', 4),
  _s(r'\b(?:ArrayList|HashMap|StringBuilder)\b|\b(?:Optional|List|Map)<', 1.5),
  _s(r'\bfinal\s+[A-Z]\w*(?:<[^>\n]*>)?\s+\w+\s*=', 1.5),
  _s(r'\bString\[\]|\bnew\s+[A-Z]\w*(?:<[^>\n]*>)?\s*\(', 0.5),
  _s(_endsInSemicolon, 0.2, cap: 5),
  _s(_opensBrace, 0.2, cap: 5),
];

final csharpSigns = [
  _s(r'^[ \t]*using\s+(?:static\s+)?[A-Z][\w.]*;', 4),
  _s(r'^[ \t]*namespace\s+[A-Z][\w.]*\s*(?:\{|;)?[ \t]*$', 3),
  _s(
    r'\b(?:public|private|protected|internal)\s+(?:static\s+)?(?:async\s+)?'
    r'(?:override\s+)?(?:void|string|int|bool|Task(?:<[^>\n]*>)?)\s+'
    r'[A-Z]\w*\s*\(',
    4,
  ),
  _s(
    r'\{\s*get;\s*(?:(?:private\s+|init\s*;)?\s*set;\s*)?\}|'
    r'^[ \t]*(?:get|set)\s*\{',
    5,
  ),
  _s(r'\bConsole\.Write(?:Line)?\(', 6),
  _s(r'\bvar\s+\w+\s*=\s*new\b', 1),
  _s(r'\bstring\s+\w+\s*[=;),]', 1.5),
  _s(r'\basync\s+Task\b|\bTask<', 3),
  _s(
    r'^[ \t]*\[(?:HttpGet|HttpPost|Route|Serializable|Fact|Test|TestMethod|'
    r'Obsolete|DllImport|Required|JsonProperty)[\](]',
    3,
  ),
  _s(r'\bforeach\s*\(\s*var\b', 4),
  _s(r'\bnameof\(|\?\?=|\busing\s*\(\s*var\b', 2),
  _s(
    r'\b(?:public|internal)\s+(?:sealed\s+|static\s+|partial\s+|abstract\s+)*'
    r'class\s+\w+\s*(?::\s*[A-Z]\w*)?',
    1,
  ),
  _s(_endsInSemicolon, 0.2, cap: 5),
];

final kotlinSigns = [
  _s(r'\bfun\s+(?:<[^>\n]*>\s*)?(?:[\w.]+\.)?\w+\s*\(', 5),
  _s(r'\bval\s+\w+\s*(?::\s*[\w<>?, ]+)?\s*=', 3),
  _s(r'\bvar\s+\w+\s*:\s*[A-Z][\w<>?]*', 2),
  _s(r'^[ \t]*package\s+[\w.]+[ \t]*$', 2, cap: 1),
  _s(r'^[ \t]*import\s+[a-z][\w]*\.[\w.]+(?:\.\*)?[ \t]*$', 1),
  _s(r'\b(?:data|sealed|open|inner|annotation)\s+class\b', 4),
  _s(r'\bcompanion\s+object\b|^[ \t]*object\s+\w+\s*[:{]', 4),
  _s(r'\bwhen\s*(?:\([^)\n]*\))?\s*\{', 3),
  _s(r'\?:\s|!!\.|::class\.java\b', 1.5),
  _s(
    r'\b(?:setOf|listOf|mapOf|mutableListOf|mutableMapOf|arrayOf|emptyList|'
    r'lazy)\s*[({<]',
    4,
  ),
  _s(r'\bit\.\w+', 1),
  _s(r'\bprintln\(', 1),
  _s(_endsInSemicolon, -0.3, cap: 4),
];

final dartSigns = [
  _s(r'^[ \t]*import\s+[\x27"](?:package|dart):', 6),
  _s(r'^[ \t]*(?:import|export|part)\s+[\x27"][^\x27"\n]+\.dart[\x27"]', 4),
  _s(r'\bvoid\s+main\s*\(', 2),
  _s(r'\bfinal\s+\w+\s*=', 1),
  _s(
    r'\b(?:StatelessWidget|StatefulWidget)\b|\bWidget\s+build\(|'
    r'\bBuildContext\s+\w+|\bsetState\(',
    4,
  ),
  _s(r'\b(?:Future|Stream)<[\w<>?, ]+>\s+\w+\s*\(', 3),
  _s(r'\)\s*async\s*\*?\s*\{', 3),
  _s(r'@override\b', 4),
  _s(r'\blate\s+(?:final\s+)?\w+|\brequired\s+this\.|\bthis\.\w+\s*[,})]', 2),
  _s(r'\b(?:dynamic|num)\b|\?\?=', 1.5),
  _s(r"'[^'\n]*\$\{?\w", 2),
  _s(r'\b(?:get|set)\s+\w+\s*(?:=>|\{)', 2),
  _s(r'\bfor\s*\(\s*(?:final|var)\s+\w+\s+in\b', 3),
  _s(_endsInSemicolon, 0.2, cap: 5),
];

final cSigns = [
  _s(r'^[ \t]*#\s*include\s*[<"][\w./-]+\.h[>"]', 4),
  _s(r'^[ \t]*#(?:define|ifdef|ifndef|endif|pragma|undef|if|else|elif)\b', 2),
  _s(
    r'^[ \t]*(?:static\s+|inline\s+|extern\s+|const\s+)*(?:int|void|char|'
    r'float|double|long|short|unsigned|signed|bool|size_t|uint8_t|uint16_t|'
    r'uint32_t|uint64_t|int8_t|int16_t|int32_t|int64_t|struct\s+\w+)\s+\**'
    r'\w+\s*\([^)\n]*\)\s*\{?[ \t]*$',
    2,
  ),
  _s(
    r'\b(?:printf|fprintf|malloc|calloc|realloc|free|sizeof|strlen|strcmp|'
    r'strcpy|memcpy|memset)\s*\(',
    2,
  ),
  _s(r'\bNULL\b', 1.5),
  _s(r'(?<=\w)->\w', 0.5),
  _s(r'\bstruct\s+\w+\s*\{|\btypedef\s+', 2),
  _s(r'\b(?:char|int|void|float|double)\s*\*+\s*\w+', 2),
  _s(r'\bint\s+main\s*\(', 3),
  _s(_endsInSemicolon, 0.2, cap: 5),
];

final cppSigns = [
  _s(r'\bstd::\w+(?!::)', 4),
  _s(
    r'^[ \t]*#\s*include\s*<(?:iostream|vector|string|map|set|memory|'
    r'algorithm|unordered_map|unordered_set|cstdio|cstdlib|cstring|cmath|'
    r'functional|utility|thread|mutex|array|optional|tuple|sstream|fstream|'
    r'chrono)>',
    5,
  ),
  _s(r'\bnamespace\s+\w+\s*\{|\busing\s+namespace\s+\w+;', 4),
  _s(r'\btemplate\s*<', 4),
  _s(r'\b(?:class|struct)\s+\w+\s*:\s*(?:public|private|protected)\s+\w+', 3),
  _s(r'^[ \t]*(?:public|private|protected):[ \t]*$', 4),
  _s(r'\bcout\s*<<|\bcin\s*>>|\bendl\b', 4),
  _s(r'\bnullptr\b|\bconstexpr\b|\bnoexcept\b', 4),
  _s(r'\bvirtual\s|\bauto\s+[\w&*]+\s*=|\bconst\s+\w+&', 1.5),
];

final sqlSigns = [
  _s(r'^[ \t]*SELECT\b', 2),
  _i(r'^[ \t]*SELECT\s[^;]{0,300}?\bFROM\s+[\w."`\[]', 3),
  _i(
    r'^[ \t]*(?:INSERT\s+INTO|UPDATE\s+[\w."`]+\s+SET|DELETE\s+FROM|'
    r'CREATE\s+(?:TABLE|INDEX|VIEW|UNIQUE|OR\s+REPLACE|DATABASE|SCHEMA|'
    r'FUNCTION|TRIGGER|EXTENSION|TYPE|SEQUENCE|MATERIALIZED)|ALTER\s+TABLE|'
    r'DROP\s+(?:TABLE|INDEX|VIEW|DATABASE|SCHEMA)|WITH\s+\w+\s+AS\s*\(|'
    r'GRANT\s+\w+\s+ON|BEGIN\s*;|COMMIT\s*;)',
    5,
  ),
  _s(
    r'\b(?:WHERE|GROUP\s+BY|ORDER\s+BY|(?:INNER|LEFT|RIGHT|FULL|CROSS)\s+'
    r'(?:OUTER\s+)?JOIN|HAVING|LIMIT\s+\d+|VALUES\s*\(|UNION\s+ALL)\b',
    1.5,
  ),
  _s(r'\b(?:VARCHAR|BIGINT|PRIMARY\s+KEY|NOT\s+NULL|FOREIGN\s+KEY)\b', 1.5),
  _s(
    r'\b(?:SELECT|FROM|WHERE|INSERT|UPDATE|DELETE|CREATE|TABLE|JOIN|INTO|'
    r'VALUES|AND|OR|AS|ON|SET|NULL|NOT|INTEGER|TEXT|DEFAULT|REFERENCES)\b',
    0.5,
    cap: 6,
  ),
  _s(r'^[ \t]*--\s', 1),
  _s(r'[{}]', -3, cap: 1),
  _s(r'=>|\$this|\bfunction\b|\bdef\b|\breturn\b', -1.5),
];

final postgresqlSigns = [
  _s(
    r'::[a-z]\w*|\$\$|\bRETURNING\b|\bSERIAL\b|\bILIKE\b|\bJSONB\b|'
    r'\bplpgsql\b|\bTIMESTAMPTZ\b',
    2,
  ),
];

final mysqlSigns = [
  _s(
    r'\b(?:FROM|INTO|TABLE|JOIN|UPDATE|EXISTS)\s+`\w+`|\bAUTO_INCREMENT\b|'
    r'\bENGINE\s*=|\bUNSIGNED\b|\bTINYINT\b|\bDELIMITER\b',
    2,
  ),
];

final cssSigns = [
  _s(r'^[ \t]*[.#][\w-]+[^{};\n]*\{[ \t]*$', 2.5),
  _s(r'^[ \t]*[.#]?[\w-]+[^{};\n]*\{[ \t]*-?[a-z-]+\s*:\s*[^;{}\n]+;', 2.5),
  _s(
    r'^[ \t]*(?:[a-z][\w-]*|\*)(?:[.#\[][^{};\n]*|:[a-z-][^{};\n]*)?'
    r'(?:[ \t]*[,>+~][ \t]*[^{};\n]*)?[ \t]*\{[ \t]*$',
    0.5,
  ),
  _s(r'^[ \t]*-?[a-z][a-z-]*\s*:\s*[^;{}\n]+;[ \t]*$', 1.2, cap: 5),
  _s(
    r'\b(?:color|background(?:-color|-image)?|margin|padding|display|'
    r'font-(?:size|family|weight)|border(?:-radius)?|width|height|max-width|'
    r'min-height|position|flex|grid|text-align|z-index|opacity|transition|'
    r'transform|box-shadow|cursor|overflow|line-height)\s*:',
    1.5,
  ),
  _s(r'\d(?:px|r?em|v[hw]|ms|deg)\b|\d%', 1),
  _s(r'#[0-9a-fA-F]{3,8}\b', 0.5),
  _s(
    r'^[ \t]*@(?:media|import|font-face|keyframes|supports|charset|layer|'
    r'container)\b',
    3,
  ),
  _s(r'!important\b', 3),
  _s(r'\b(?:function|return|const|let|var|def|class)\b', -1),
];

final scssSigns = [
  _s(r'^[ \t]*\$[\w-]+\s*:', 4),
  _s(r'@(?:mixin|include|extend|use|forward|each|function|return)\b', 4),
  _s(r'&(?::|\.|-|__)', 2),
  _s(r'#\{', 2),
];

final lessSigns = [
  _s(r'^[ \t]*@[\w-]+\s*:', 4),
  _s(r'^[ \t]*\.[\w-]+\s*\([^)\n]*\)\s*;', 3),
  _s(r'~["\x27]|\bwhen\s*\(', 2),
  _s(r'&(?::|\.|-)', 1),
];

// Tags in doc comments (` * <p>`, `/// <summary>`) are not markup.
const _notInComment = r'(?<!^[ \t]*(?:\*|//|#).*)';

const _htmlTags =
    '$_notInComment'
    r'<(?:html|head|body|div|span|p|a|ul|ol|li|table|thead|tbody|tr|td|th|'
    r'form|input|button|img|script|style|link|meta|title|h[1-6]|nav|header|'
    r'footer|section|article|main|aside|label|select|option|textarea|br|hr|'
    r'em|strong|pre|code|iframe|video|audio|canvas|svg|template|slot)\b'
    r'[^<>]{0,300}>';

final _markupSigns = [
  _s(
    '$_notInComment'
    r'(?<![\w$.])<[\w:-]+(?:\s+[\w:@.-]+(?:\s*=\s*(?:"[^"]*"|\x27[^\x27]*\x27|'
    r'[\w-]+))?)*\s*/?>',
    0.5,
    cap: 6,
  ),
  _s(
    '$_notInComment'
    r'</[\w:-]+\s*>',
    0.7,
    cap: 6,
  ),
  _s(r'<!--', 0.5),
  _s(r'\bclassName=|\breturn\s*\(?\s*<|\{[\w.]+\}', -1),
];

final htmlSigns = [
  ..._markupSigns,
  _i(r'<!DOCTYPE\s+html', 10, cap: 1),
  _i(_htmlTags, 2, cap: 5),
];

final xmlSigns = [
  ..._markupSigns,
  _s(r'^[ \t]*<\?xml\b', 10, cap: 1),
  _s(r'\bxmlns(?::\w+)?=', 4),
  _s(r'</?[a-z]+:[a-z]', 2),
  _s(r'<!\[CDATA\[', 3),
];

// A YAML value is short and not a sentence: `. ` ends prose.
const _yamlValue = r'[^\s#{(](?:[^;\n.]|\.(?=\S)){0,60}';

final yamlSigns = [
  _s(r'^[ \t]*[\w.-]+:[ \t]*$', 1, cap: 4),
  _s('^[ \\t]*[\\w.-]+:[ \\t]+$_yamlValue[ \\t]*\$', 0.6, cap: 8),
  _s('^[ \\t]*-[ \\t]+[\\w.-]+:[ \\t]+$_yamlValue[ \\t]*\$', 1.5),
  _s(r'^---[ \t]*$', 2, cap: 1),
  _s(r'^[ \t]*[\w.-]+:[ \t]+(?:\[[^\]\n]*\]|\{[^}\n]*\})[ \t]*$', 1.5),
  _s(r'^[ \t]*[\w.-]+:[ \t]*[|>][-+]?[ \t]*$', 3),
  _s(
    r'^[ \t]*(?:apiVersion|kind|metadata|spec|name|version|image|steps|uses|'
    r'with|run|jobs|on|env|services|dependencies|dev_dependencies|'
    r'environment|flutter|sdk|script|stages|include):',
    0.5,
    cap: 6,
  ),
  _s(_endsInSemicolon, -1, cap: 4),
  _s(_opensBrace, -1, cap: 4),
  _s(r',[ \t]*$', -0.5, cap: 6),
  _s(r'\)\s*\{|\bfunction\b|=>|\breturn\b', -1.5),
];
