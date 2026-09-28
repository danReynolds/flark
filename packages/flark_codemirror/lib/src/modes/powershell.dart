// Ported from @codemirror/legacy-modes 6.5.4 mode/powershell.js, its
// `powerShell` export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

// Upstream builds its grammar with `buildRegexp`: case-insensitive
// alternatives of these patterns, each then tried at the stream in turn.
// Past the first character, a keyword or builtin must be followed by a
// character outside `[A-Za-z\d\-_]` and a word operator by one outside
// `\w`, so each matches exactly when the whole run of such characters at the
// stream is one of its words. The port reads the words out of the patterns
// once and looks the run up in place.
const _keywordPatterns = [
  'begin|break|catch|continue|data|default|do|dynamicparam',
  'else|elseif|end|exit|filter|finally|for|foreach|from|function|if|in',
  'param|process|return|switch|throw|trap|try|until|where|while',
];
const _wordOperatorPatterns = [
  'f',
  'b?not',
  '[ic]?split', 'join', //
  'is(not)?', 'as',
  '[ic]?(eq|ne|[gl][te])',
  '[ic]?(not)?(like|match|contains)',
  '[ic]?replace',
  'b?(and|or|xor)',
];

const _namedBuiltinPatterns = [
  'Add-(Computer|Content|History|Member|PSSnapin|Type)',
  'Checkpoint-Computer',
  'Clear-(Content|EventLog|History|Host|Item(Property)?|Variable)',
  'Compare-Object',
  'Complete-Transaction',
  'Connect-PSSession',
  'ConvertFrom-(Csv|Json|SecureString|StringData)',
  'Convert-Path',
  'ConvertTo-(Csv|Html|Json|SecureString|Xml)',
  'Copy-Item(Property)?',
  'Debug-Process',
  'Disable-(ComputerRestore|PSBreakpoint|PSRemoting|'
      'PSSessionConfiguration)',
  'Disconnect-PSSession',
  'Enable-(ComputerRestore|PSBreakpoint|PSRemoting|'
      'PSSessionConfiguration)',
  '(Enter|Exit)-PSSession',
  'Export-(Alias|Clixml|Console|Counter|Csv|FormatData|ModuleMember|'
      'PSSession)',
  'ForEach-Object',
  'Format-(Custom|List|Table|Wide)',
  'Get-(Acl|Alias|AuthenticodeSignature|ChildItem|Command|'
      'ComputerRestorePoint|Content|ControlPanelItem|Counter|Credential'
      '|Culture|Date|Event|EventLog|EventSubscriber|ExecutionPolicy|'
      'FormatData|Help|History|Host|HotFix|Item|ItemProperty|Job'
      '|Location|Member|Module|PfxCertificate|Process|PSBreakpoint|'
      'PSCallStack|PSDrive|PSProvider|PSSession|PSSessionConfiguration'
      '|PSSnapin|Random|Service|TraceSource|Transaction|TypeData|UICulture|'
      'Unique|Variable|Verb|WinEvent|WmiObject)',
  'Group-Object',
  'Import-(Alias|Clixml|Counter|Csv|LocalizedData|Module|PSSession)',
  'ImportSystemModules',
  'Invoke-(Command|Expression|History|Item|RestMethod|WebRequest|'
      'WmiMethod)',
  'Join-Path',
  'Limit-EventLog',
  'Measure-(Command|Object)',
  'Move-Item(Property)?',
  'New-(Alias|Event|EventLog|Item(Property)?|Module|ModuleManifest|'
      'Object|PSDrive|PSSession|PSSessionConfigurationFile'
      '|PSSessionOption|PSTransportOption|Service|TimeSpan|Variable|'
      'WebServiceProxy|WinEvent)',
  'Out-(Default|File|GridView|Host|Null|Printer|String)',
  'Pause',
  '(Pop|Push)-Location',
  'Read-Host',
  'Receive-(Job|PSSession)',
  'Register-(EngineEvent|ObjectEvent|PSSessionConfiguration|WmiEvent)',
  'Remove-(Computer|Event|EventLog|Item(Property)?|Job|Module|'
      'PSBreakpoint|PSDrive|PSSession|PSSnapin|TypeData|Variable|'
      'WmiObject)',
  'Rename-(Computer|Item(Property)?)',
  'Reset-ComputerMachinePassword',
  'Resolve-Path',
  'Restart-(Computer|Service)',
  'Restore-Computer',
  'Resume-(Job|Service)',
  'Save-Help',
  'Select-(Object|String|Xml)',
  'Send-MailMessage',
  'Set-(Acl|Alias|AuthenticodeSignature|Content|Date|ExecutionPolicy|'
      'Item(Property)?|Location|PSBreakpoint|PSDebug'
      '|PSSessionConfiguration|Service|StrictMode|TraceSource|Variable|'
      'WmiInstance)',
  'Show-(Command|ControlPanelItem|EventLog)',
  'Sort-Object',
  'Split-Path',
  'Start-(Job|Process|Service|Sleep|Transaction|Transcript)',
  'Stop-(Computer|Job|Process|Service|Transcript)',
  'Suspend-(Job|Service)',
  'TabExpansion2',
  'Tee-Object',
  'Test-(ComputerSecureChannel|Connection|ModuleManifest|Path|'
      'PSSessionConfigurationFile)',
  'Trace-Command',
  'Unblock-File',
  'Undo-Transaction',
  'Unregister-(Event|PSSessionConfiguration)',
  'Update-(FormatData|Help|List|TypeData)',
  'Use-Transaction',
  'Wait-(Event|Job|Process)',
  'Where-Object',
  'Write-(Debug|Error|EventLog|Host|Output|Progress|Verbose|Warning)',
  'cd|help|mkdir|more|oss|prompt',
  'ac|asnp|cat|cd|chdir|clc|clear|clhy|cli|clp|cls|clv|cnsn|compare|'
      'copy|cp|cpi|cpp|cvpa|dbp|del|diff|dir|dnsn|ebp',
  'echo|epal|epcsv|epsn|erase|etsn|exsn|fc|fl|foreach|ft|fw|gal|gbp|gc|'
      'gci|gcm|gcs|gdr|ghy|gi|gjb|gl|gm|gmo|gp|gps',
  'group|gsn|gsnp|gsv|gu|gv|gwmi|h|history|icm|iex|ihy|ii|ipal|ipcsv|'
      'ipmo|ipsn|irm|ise|iwmi|iwr|kill|lp|ls|man|md',
  'measure|mi|mount|move|mp|mv|nal|ndr|ni|nmo|npssc|nsn|nv|ogv|oh|popd|'
      'ps|pushd|pwd|r|rbp|rcjb|rcsn|rd|rdr|ren|ri',
  'rjb|rm|rmdir|rmo|rni|rnp|rp|rsn|rsnp|rujb|rv|rvpa|rwmi|sajb|sal|'
      'saps|sasv|sbp|sc|select|set|shcm|si|sl|sleep|sls',
  'sort|sp|spjb|spps|spsv|start|sujb|sv|swmi|tee|trcm|type|where|wjb|'
      'write',
];

const _variableBuiltinPatterns = [
  r'[$?^_]|Args|ConfirmPreference|ConsoleFileName|DebugPreference|Error|'
      'ErrorActionPreference|ErrorView|ExecutionContext',
  'FormatEnumerationLimit|Home|Host|Input|MaximumAliasCount|'
      'MaximumDriveCount|MaximumErrorCount|MaximumFunctionCount',
  'MaximumHistoryCount|MaximumVariableCount|MyInvocation|'
      'NestedPromptLevel|OutputEncoding|Pid|Profile|ProgressPreference',
  'PSBoundParameters|PSCommandPath|PSCulture|PSDefaultParameterValues|'
      'PSEmailServer|PSHome|PSScriptRoot|PSSessionApplicationName',
  'PSSessionConfigurationName|PSSessionOption|PSUICulture|'
      'PSVersionTable|Pwd|ShellId|StackTrace|VerbosePreference',
  'WarningPreference|WhatIfPreference',
  'Event|EventArgs|EventSubscriber|Sender',
  'Matches|Ofs|ForEach|LastExitCode|PSCmdlet|PSItem|PSSenderInfo|This',
  'true|false|null',
];

final _keywords = _words(_keywordPatterns);
final _wordOperators = _words(_wordOperatorPatterns, _isWordUnit);
final _namedBuiltins = _words(_namedBuiltinPatterns);
final _variableBuiltins = _words(_variableBuiltinPatterns);

final _numbers = RegExp(
  r'((0x[\da-f]+)|((\d+\.\d+|\d\.|\.\d+|\d+)(e[\+\-]?\d+)?))[ld]?([kmgtp]b)?',
  caseSensitive: false,
);

/// JavaScript's `\w` for one code unit, which the patterns' `i` flag leaves
/// ASCII.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

/// Upstream's `[A-Za-z\d\-_]`, ignoring case: a keyword or builtin must not
/// be followed by one.
bool _isNameUnit(int u) => _isWordUnit(u) || u == 0x2d;

bool _isLetter(int u) => (u >= 0x41 && u <= 0x5a) || (u >= 0x61 && u <= 0x7a);

bool _isDigit(int u) => u >= 0x30 && u <= 0x39;

/// Upstream's `varNames`, `[\w\-:]`.
bool _isVarNameUnit(int u) => _isNameUnit(u) || u == 0x3a;

/// The end of the run of [unit] units in [s] from [start].
int _runEnd(String s, int start, bool Function(int) unit) {
  var i = start;
  while (i < s.length && unit(s.codeUnitAt(i))) {
    i++;
  }
  return i;
}

/// Whether upstream's lookahead `(?=[^A-Za-z\d\-_]|$)` holds at [i].
bool _endsName(String s, int i) =>
    i >= s.length || !_isNameUnit(s.codeUnitAt(i));

/// The words [patterns] match: each is a run of [unit] units, or one other
/// character.
_Words _words(List<String> patterns, [bool Function(int) unit = _isNameUnit]) {
  final words = <String>{};
  for (final pattern in patterns) {
    for (final word in _Expander(pattern).expand()) {
      assert(
        word.length == 1 || word.isNotEmpty && word.codeUnits.every(unit),
        word,
      );
      words.add(word.toLowerCase());
    }
  }
  return _Words(words);
}

/// ASCII lower case, which is all the `i` flag folds into ASCII.
int _lower(int u) => u >= 0x41 && u <= 0x5a ? u + 0x20 : u;

int _hash(String s, int start, int end) {
  var hash = 0;
  for (var i = start; i < end; i++) {
    hash = (hash * 31 + _lower(s.codeUnitAt(i))) & 0x3fffffff;
  }
  return hash;
}

/// Lower-cased words, found by a span of text without copying it.
final class _Words {
  _Words(Set<String> words)
    : _slots = List.filled(_sizeFor(words.length), null) {
    for (final word in words) {
      (_slots[_hash(word, 0, word.length) & (_slots.length - 1)] ??= []).add(
        word,
      );
    }
  }

  static int _sizeFor(int count) {
    var size = 16;
    while (size < 2 * count) {
      size *= 2;
    }
    return size;
  }

  final List<List<String>?> _slots;

  /// Whether [s] from [start] to [end], lower-cased, is one of the words.
  bool contains(String s, int start, int end) {
    final slot = _slots[_hash(s, start, end) & (_slots.length - 1)];
    if (slot == null) return false;
    search:
    for (final word in slot) {
      if (word.length != end - start) continue;
      for (var i = 0; i < word.length; i++) {
        if (_lower(s.codeUnitAt(start + i)) != word.codeUnitAt(i)) {
          continue search;
        }
      }
      return true;
    }
    return false;
  }
}

/// The words a pattern of the tables above matches. Its syntax is literal
/// characters, groups of alternatives, classes of single characters and `?`
/// after any of them; the reader refuses anything else.
final class _Expander {
  _Expander(this.pattern);
  final String pattern;
  var _i = 0;

  Set<String> expand() {
    final words = _alternatives();
    if (_i != pattern.length) throw FormatException('', pattern, _i);
    return words;
  }

  Set<String> _alternatives() {
    final words = _sequence();
    while (_i < pattern.length && pattern[_i] == '|') {
      _i++;
      words.addAll(_sequence());
    }
    return words;
  }

  Set<String> _sequence() {
    var words = {''};
    while (_i < pattern.length && pattern[_i] != '|' && pattern[_i] != ')') {
      final c = pattern[_i++];
      Set<String> item;
      if (c == '(') {
        if (_i < pattern.length && pattern[_i] == '?') {
          throw FormatException('', pattern, _i);
        }
        item = _alternatives();
        if (_i >= pattern.length || pattern[_i] != ')') {
          throw FormatException('', pattern, _i);
        }
        _i++;
      } else if (c == '[') {
        final close = pattern.indexOf(']', _i);
        final members = pattern.substring(_i, close);
        if (members.isEmpty ||
            members.startsWith('^') ||
            members.contains('-')) {
          throw FormatException('', pattern, _i);
        }
        if (members.contains(r'\')) throw FormatException('', pattern, _i);
        item = members.split('').toSet();
        _i = close + 1;
      } else if (r'\.*+{}^$'.contains(c)) {
        throw FormatException('', pattern, _i - 1);
      } else {
        item = {c};
      }
      if (_i < pattern.length && pattern[_i] == '?') {
        _i++;
        item = {...item, ''};
      }
      words = {
        for (final word in words)
          for (final end in item) '$word$end',
      };
    }
    return words;
  }
}

/// Upstream's `operators`: `-` and a whole word operator (the `\b` after
/// its letters), else `[+\-*\/%]=|\+\+|--|\.\.|[+\-*&^%:=!|\/]|<(?!#)|(?!#)>`.
/// The length matched at [pos], or 0.
int _operatorLength(String s, int pos) {
  final c = s.codeUnitAt(pos);
  final next = pos + 1 < s.length ? s.codeUnitAt(pos + 1) : -1;
  if (c == 0x2d) {
    final end = _runEnd(s, pos + 1, _isWordUnit);
    if (end > pos + 1 && _wordOperators.contains(s, pos + 1, end)) {
      return end - pos;
    }
  }
  switch (c) {
    // + - * / %
    case 0x2b || 0x2d || 0x2a || 0x2f || 0x25 when next == 0x3d:
      return 2;
    // ++ -- ..
    case 0x2b || 0x2d || 0x2e when next == c:
      return 2;
    // + - * & ^ % : = ! | /
    case 0x2b || 0x2d || 0x2a || 0x26 || 0x5e || 0x25 || 0x3a || 0x3d:
    case 0x21 || 0x7c || 0x2f:
      return 1;
    // < not before #
    case 0x3c when next != 0x23:
      return 1;
    // >, whatever follows
    case 0x3e:
      return 1;
  }
  return 0;
}

/// Upstream's `builtins`: `[A-Z]:`, `%`, `\?`, a named builtin, or `\$` and a
/// variable builtin, then its lookahead. [end] ends the run of name units at
/// [pos]. The length matched, or 0.
int _builtinLength(String s, int pos, int end) {
  final c = s.codeUnitAt(pos);
  if (_isLetter(c) &&
      pos + 1 < s.length &&
      s.codeUnitAt(pos + 1) == 0x3a &&
      _endsName(s, pos + 2)) {
    return 2;
  }
  if ((c == 0x25 || c == 0x3f) && _endsName(s, pos + 1)) return 1;
  if (end > pos && _namedBuiltins.contains(s, pos, end)) return end - pos;
  if (c == 0x24 && pos + 1 < s.length) {
    final d = s.codeUnitAt(pos + 1);
    if (!_isNameUnit(d)) {
      // A variable builtin of one other character: `$`, `?` or `^`.
      return _variableBuiltins.contains(s, pos + 1, pos + 2) &&
              _endsName(s, pos + 2)
          ? 2
          : 0;
    }
    final variableEnd = _runEnd(s, pos + 1, _isNameUnit);
    if (_variableBuiltins.contains(s, pos + 1, variableEnd)) {
      return variableEnd - pos;
    }
  }
  return 0;
}

/// Upstream's `punctuation`, `[\[\]{},;`\\\.]|@[({]`: the length matched at
/// [pos], or 0.
int _punctuationLength(String s, int pos) {
  switch (s.codeUnitAt(pos)) {
    case 0x5b || 0x5d || 0x7b || 0x7d || 0x2c || 0x3b || 0x60 || 0x5c || 0x2e:
      return 1;
    case 0x40 when pos + 1 < s.length:
      final next = s.codeUnitAt(pos + 1);
      return next == 0x28 || next == 0x7b ? 2 : 0;
  }
  return 0;
}

/// Upstream's `identifiers`, `[A-Za-z\_][A-Za-z\-\_\d]*\b`: the run of name
/// units at [pos], which ends at [end], back to its last word character,
/// where `\b` holds. The length matched, or 0.
int _identifierLength(String s, int pos, int end) {
  final c = s.codeUnitAt(pos);
  if (!_isLetter(c) && c != 0x5f) return 0;
  while (s.codeUnitAt(end - 1) == 0x2d) {
    end--;
  }
  return end - pos;
}

typedef _Tokenizer =
    String? Function(StringStream stream, PowerShellState state);

/// An entry of upstream's `returnStack`: where an interpolation returns to,
/// once the bracket nesting is back at [bracketNesting], or at once when
/// that is null.
final class _Return {
  const _Return(this.tokenize, [this.bracketNesting]);
  final _Tokenizer tokenize;
  final int? bracketNesting;

  bool shouldReturnFrom(PowerShellState state) =>
      bracketNesting == null || state._bracketNesting == bracketNesting;
}

final class PowerShellState {
  PowerShellState._(
    this._returnStack,
    this._bracketNesting,
    this._tokenize, [
    this._startQuote,
  ]);
  final List<_Return> _returnStack;
  int _bracketNesting;
  _Tokenizer _tokenize;
  String? _startQuote;

  /// The upstream default copy: the return stack is copied, its entries
  /// shared.
  PowerShellState copy() => PowerShellState._(
    List.of(_returnStack),
    _bracketNesting,
    _tokenize,
    _startQuote,
  );
}

// tokenizers
String? _tokenBase(StringStream stream, PowerShellState state) {
  final parent = state._returnStack.isEmpty ? null : state._returnStack.last;
  if (parent != null && parent.shouldReturnFrom(state)) {
    state._tokenize = parent.tokenize;
    state._returnStack.removeLast();
    return state._tokenize(stream, state);
  }

  if (stream.eatSpace()) return null;

  final s = stream.string, pos = stream.pos;
  final c = s.codeUnitAt(pos);
  if (c == 0x28) {
    stream.pos++;
    state._bracketNesting += 1;
    return 'punctuation';
  }

  if (c == 0x29) {
    stream.pos++;
    state._bracketNesting -= 1;
    return 'punctuation';
  }

  // Upstream's grammar: keyword, number, operator, builtin, punctuation and
  // variable patterns, tried in turn at the stream.
  final end = _runEnd(s, pos, _isNameUnit);
  if (end > pos && _keywords.contains(s, pos, end)) {
    stream.pos = end;
    return 'keyword';
  }
  // A number starts with a digit, or a point before one.
  if ((_isDigit(c) ||
          c == 0x2e && pos + 1 < s.length && _isDigit(s.codeUnitAt(pos + 1))) &&
      stream.match(_numbers) != null) {
    return 'number';
  }
  final operator = _operatorLength(s, pos);
  if (operator > 0) {
    stream.pos += operator;
    return 'operator';
  }
  final builtin = _builtinLength(s, pos, end);
  if (builtin > 0) {
    stream.pos += builtin;
    return 'builtin';
  }
  final punctuation = _punctuationLength(s, pos);
  if (punctuation > 0) {
    stream.pos += punctuation;
    return 'punctuation';
  }
  final identifier = _identifierLength(s, pos, end);
  if (identifier > 0) {
    stream.pos += identifier;
    return 'variable';
  }

  final ch = stream.next();

  // single-quote string
  if (ch == "'") return _tokenSingleQuoteString(stream, state);

  if (ch == r'$') return _tokenVariable(stream, state);

  // double-quote string
  if (ch == '"') return _tokenDoubleQuoteString(stream, state);

  if (ch == '<' && stream.eat('#') != null) {
    state._tokenize = _tokenComment;
    return _tokenComment(stream, state);
  }

  if (ch == '#') {
    stream.skipToEnd();
    return 'comment';
  }

  if (ch == '@') {
    final quoteMatch = stream.eat('"') ?? stream.eat("'");
    if (quoteMatch != null && stream.eol()) {
      state._tokenize = _tokenMultiString;
      state._startQuote = quoteMatch;
      return _tokenMultiString(stream, state);
    } else if (stream.eol()) {
      return 'error';
    } else if ('({'.contains(stream.peek()!)) {
      return 'punctuation';
    } else if (_isVarNameUnit(stream.peek()!.codeUnitAt(0))) {
      // splatted variable
      return _tokenVariable(stream, state);
    }
  }
  return 'error';
}

String? _tokenSingleQuoteString(StringStream stream, PowerShellState state) {
  String? ch;
  while ((ch = stream.peek()) != null) {
    stream.next();

    if (ch == "'" && stream.eat("'") == null) {
      state._tokenize = _tokenBase;
      return 'string';
    }
  }

  return 'error';
}

String? _tokenDoubleQuoteString(StringStream stream, PowerShellState state) {
  String? ch;
  while ((ch = stream.peek()) != null) {
    if (ch == r'$') {
      state._tokenize = _tokenStringInterpolation;
      return 'string';
    }

    stream.next();
    if (ch == '`') {
      stream.next();
      continue;
    }

    if (ch == '"' && stream.eat('"') == null) {
      state._tokenize = _tokenBase;
      return 'string';
    }
  }

  return 'error';
}

String? _tokenStringInterpolation(StringStream stream, PowerShellState state) =>
    _tokenInterpolation(stream, state, _tokenDoubleQuoteString);

String? _tokenMultiStringReturn(StringStream stream, PowerShellState state) {
  state._tokenize = _tokenMultiString;
  state._startQuote = '"';
  return _tokenMultiString(stream, state);
}

String? _tokenHereStringInterpolation(
  StringStream stream,
  PowerShellState state,
) => _tokenInterpolation(stream, state, _tokenMultiStringReturn);

String? _tokenInterpolation(
  StringStream stream,
  PowerShellState state,
  _Tokenizer parentTokenize,
) {
  if (stream.matchString(r'$(')) {
    final savedBracketNesting = state._bracketNesting;
    state._returnStack.add(_Return(parentTokenize, savedBracketNesting));
    state._tokenize = _tokenBase;
    state._bracketNesting += 1;
    return 'punctuation';
  } else {
    stream.next();
    state._returnStack.add(_Return(parentTokenize));
    state._tokenize = _tokenVariable;
    return state._tokenize(stream, state);
  }
}

String? _tokenComment(StringStream stream, PowerShellState state) {
  var maybeEnd = false;
  String? ch;
  while ((ch = stream.next()) != null) {
    if (maybeEnd && ch == '>') {
      state._tokenize = _tokenBase;
      break;
    }
    maybeEnd = ch == '#';
  }
  return 'comment';
}

String? _tokenVariable(StringStream stream, PowerShellState state) {
  final ch = stream.peek();
  if (stream.eat('{') != null) {
    state._tokenize = _tokenVariableWithBraces;
    return _tokenVariableWithBraces(stream, state);
  } else if (ch != null && _isVarNameUnit(ch.codeUnitAt(0))) {
    stream.eatWhileCode(_isVarNameUnit);
    state._tokenize = _tokenBase;
    return 'variable';
  } else {
    state._tokenize = _tokenBase;
    return 'error';
  }
}

String? _tokenVariableWithBraces(StringStream stream, PowerShellState state) {
  String? ch;
  while ((ch = stream.next()) != null) {
    if (ch == '}') {
      state._tokenize = _tokenBase;
      break;
    }
  }
  return 'variable';
}

String? _tokenMultiString(StringStream stream, PowerShellState state) {
  final quote = state._startQuote!;
  // Upstream matches `new RegExp(quote + '@')`, the quote being literal.
  if (stream.sol() && stream.matchString('$quote@')) {
    state._tokenize = _tokenBase;
  } else if (quote == '"') {
    while (!stream.eol()) {
      final ch = stream.peek();
      if (ch == r'$') {
        state._tokenize = _tokenHereStringInterpolation;
        return 'string';
      }

      stream.next();
      if (ch == '`') stream.next();
    }
  } else {
    stream.skipToEnd();
  }

  return 'string';
}

/// PowerShell: the upstream `powerShell` stream parser.
final class PowerShellMode extends Mode<PowerShellState> {
  PowerShellMode([super.config = const ModeConfig()]);

  @override
  PowerShellState startState([int baseColumn = 0]) =>
      PowerShellState._([], 0, _tokenBase);

  @override
  PowerShellState copyState(PowerShellState state) => state.copy();

  @override
  String? token(StringStream stream, PowerShellState state) =>
      state._tokenize(stream, state);
}
