// Ported from @codemirror/legacy-modes 6.5.4 mode/powershell.js, its
// `powerShell` export.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

final _special = RegExp(r'[-\/\\^$*+?.()|[\]{}]');

/// Upstream's `buildRegexp`: [patterns], regular expressions or literal
/// strings, as alternatives between [prefix] and [suffix], ignoring case.
/// Upstream's default prefix is `^`, which [StringStream.match] implies.
RegExp _buildRegexp(
  List<Object> patterns, {
  String prefix = '',
  String suffix = r'\b',
}) {
  final sources = [
    for (final pattern in patterns)
      pattern is RegExp
          ? pattern.pattern
          : (pattern as String).replaceAllMapped(_special, (m) => '\\${m[0]}'),
  ];
  return RegExp('$prefix(${sources.join('|')})$suffix', caseSensitive: false);
}

const _notCharacterOrDash = r'(?=[^A-Za-z\d\-_]|$)';
final _varNames = RegExp(r'[\w\-:]');
final _keywords = _buildRegexp([
  RegExp('begin|break|catch|continue|data|default|do|dynamicparam'),
  RegExp('else|elseif|end|exit|filter|finally|for|foreach|from|function|if|in'),
  RegExp('param|process|return|switch|throw|trap|try|until|where|while'),
], suffix: _notCharacterOrDash);

final _punctuation = RegExp(r'[\[\]{},;`\\\.]|@[({]');
final _wordOperators = _buildRegexp([
  'f',
  RegExp('b?not'),
  RegExp('[ic]?split'), 'join', //
  RegExp('is(not)?'), 'as',
  RegExp('[ic]?(eq|ne|[gl][te])'),
  RegExp('[ic]?(not)?(like|match|contains)'),
  RegExp('[ic]?replace'),
  RegExp('b?(and|or|xor)'),
], prefix: '-');
final _symbolOperators = RegExp(
  r'[+\-*\/%]=|\+\+|--|\.\.|[+\-*&^%:=!|\/]|<(?!#)|(?!#)>',
);
final _operators = _buildRegexp([_wordOperators, _symbolOperators], suffix: '');

final _numbers = RegExp(
  r'((0x[\da-f]+)|((\d+\.\d+|\d\.|\.\d+|\d+)(e[\+\-]?\d+)?))[ld]?([kmgtp]b)?',
  caseSensitive: false,
);

final _identifiers = RegExp(r'[A-Za-z\_][A-Za-z\-\_\d]*\b');

final _symbolBuiltins = RegExp(r'[A-Z]:|%|\?', caseSensitive: false);
final _namedBuiltins = _buildRegexp([
  RegExp('Add-(Computer|Content|History|Member|PSSnapin|Type)'),
  RegExp('Checkpoint-Computer'),
  RegExp('Clear-(Content|EventLog|History|Host|Item(Property)?|Variable)'),
  RegExp('Compare-Object'),
  RegExp('Complete-Transaction'),
  RegExp('Connect-PSSession'),
  RegExp('ConvertFrom-(Csv|Json|SecureString|StringData)'),
  RegExp('Convert-Path'),
  RegExp('ConvertTo-(Csv|Html|Json|SecureString|Xml)'),
  RegExp('Copy-Item(Property)?'),
  RegExp('Debug-Process'),
  RegExp(
    'Disable-(ComputerRestore|PSBreakpoint|PSRemoting|'
    'PSSessionConfiguration)',
  ),
  RegExp('Disconnect-PSSession'),
  RegExp(
    'Enable-(ComputerRestore|PSBreakpoint|PSRemoting|'
    'PSSessionConfiguration)',
  ),
  RegExp('(Enter|Exit)-PSSession'),
  RegExp(
    'Export-(Alias|Clixml|Console|Counter|Csv|FormatData|ModuleMember|'
    'PSSession)',
  ),
  RegExp('ForEach-Object'),
  RegExp('Format-(Custom|List|Table|Wide)'),
  RegExp(
    'Get-(Acl|Alias|AuthenticodeSignature|ChildItem|Command|'
    'ComputerRestorePoint|Content|ControlPanelItem|Counter|Credential'
    '|Culture|Date|Event|EventLog|EventSubscriber|ExecutionPolicy|'
    'FormatData|Help|History|Host|HotFix|Item|ItemProperty|Job'
    '|Location|Member|Module|PfxCertificate|Process|PSBreakpoint|'
    'PSCallStack|PSDrive|PSProvider|PSSession|PSSessionConfiguration'
    '|PSSnapin|Random|Service|TraceSource|Transaction|TypeData|UICulture|'
    'Unique|Variable|Verb|WinEvent|WmiObject)',
  ),
  RegExp('Group-Object'),
  RegExp('Import-(Alias|Clixml|Counter|Csv|LocalizedData|Module|PSSession)'),
  RegExp('ImportSystemModules'),
  RegExp(
    'Invoke-(Command|Expression|History|Item|RestMethod|WebRequest|'
    'WmiMethod)',
  ),
  RegExp('Join-Path'),
  RegExp('Limit-EventLog'),
  RegExp('Measure-(Command|Object)'),
  RegExp('Move-Item(Property)?'),
  RegExp(
    'New-(Alias|Event|EventLog|Item(Property)?|Module|ModuleManifest|'
    'Object|PSDrive|PSSession|PSSessionConfigurationFile'
    '|PSSessionOption|PSTransportOption|Service|TimeSpan|Variable|'
    'WebServiceProxy|WinEvent)',
  ),
  RegExp('Out-(Default|File|GridView|Host|Null|Printer|String)'),
  RegExp('Pause'),
  RegExp('(Pop|Push)-Location'),
  RegExp('Read-Host'),
  RegExp('Receive-(Job|PSSession)'),
  RegExp('Register-(EngineEvent|ObjectEvent|PSSessionConfiguration|WmiEvent)'),
  RegExp(
    'Remove-(Computer|Event|EventLog|Item(Property)?|Job|Module|'
    'PSBreakpoint|PSDrive|PSSession|PSSnapin|TypeData|Variable|'
    'WmiObject)',
  ),
  RegExp('Rename-(Computer|Item(Property)?)'),
  RegExp('Reset-ComputerMachinePassword'),
  RegExp('Resolve-Path'),
  RegExp('Restart-(Computer|Service)'),
  RegExp('Restore-Computer'),
  RegExp('Resume-(Job|Service)'),
  RegExp('Save-Help'),
  RegExp('Select-(Object|String|Xml)'),
  RegExp('Send-MailMessage'),
  RegExp(
    'Set-(Acl|Alias|AuthenticodeSignature|Content|Date|ExecutionPolicy|'
    'Item(Property)?|Location|PSBreakpoint|PSDebug'
    '|PSSessionConfiguration|Service|StrictMode|TraceSource|Variable|'
    'WmiInstance)',
  ),
  RegExp('Show-(Command|ControlPanelItem|EventLog)'),
  RegExp('Sort-Object'),
  RegExp('Split-Path'),
  RegExp('Start-(Job|Process|Service|Sleep|Transaction|Transcript)'),
  RegExp('Stop-(Computer|Job|Process|Service|Transcript)'),
  RegExp('Suspend-(Job|Service)'),
  RegExp('TabExpansion2'),
  RegExp('Tee-Object'),
  RegExp(
    'Test-(ComputerSecureChannel|Connection|ModuleManifest|Path|'
    'PSSessionConfigurationFile)',
  ),
  RegExp('Trace-Command'),
  RegExp('Unblock-File'),
  RegExp('Undo-Transaction'),
  RegExp('Unregister-(Event|PSSessionConfiguration)'),
  RegExp('Update-(FormatData|Help|List|TypeData)'),
  RegExp('Use-Transaction'),
  RegExp('Wait-(Event|Job|Process)'),
  RegExp('Where-Object'),
  RegExp('Write-(Debug|Error|EventLog|Host|Output|Progress|Verbose|Warning)'),
  RegExp('cd|help|mkdir|more|oss|prompt'),
  RegExp(
    'ac|asnp|cat|cd|chdir|clc|clear|clhy|cli|clp|cls|clv|cnsn|compare|'
    'copy|cp|cpi|cpp|cvpa|dbp|del|diff|dir|dnsn|ebp',
  ),
  RegExp(
    'echo|epal|epcsv|epsn|erase|etsn|exsn|fc|fl|foreach|ft|fw|gal|gbp|gc|'
    'gci|gcm|gcs|gdr|ghy|gi|gjb|gl|gm|gmo|gp|gps',
  ),
  RegExp(
    'group|gsn|gsnp|gsv|gu|gv|gwmi|h|history|icm|iex|ihy|ii|ipal|ipcsv|'
    'ipmo|ipsn|irm|ise|iwmi|iwr|kill|lp|ls|man|md',
  ),
  RegExp(
    'measure|mi|mount|move|mp|mv|nal|ndr|ni|nmo|npssc|nsn|nv|ogv|oh|popd|'
    'ps|pushd|pwd|r|rbp|rcjb|rcsn|rd|rdr|ren|ri',
  ),
  RegExp(
    'rjb|rm|rmdir|rmo|rni|rnp|rp|rsn|rsnp|rujb|rv|rvpa|rwmi|sajb|sal|'
    'saps|sasv|sbp|sc|select|set|shcm|si|sl|sleep|sls',
  ),
  RegExp(
    'sort|sp|spjb|spps|spsv|start|sujb|sv|swmi|tee|trcm|type|where|wjb|'
    'write',
  ),
], suffix: '');
final _variableBuiltins = _buildRegexp(
  [
    RegExp(
      r'[$?^_]|Args|ConfirmPreference|ConsoleFileName|DebugPreference|Error|'
      'ErrorActionPreference|ErrorView|ExecutionContext',
    ),
    RegExp(
      'FormatEnumerationLimit|Home|Host|Input|MaximumAliasCount|'
      'MaximumDriveCount|MaximumErrorCount|MaximumFunctionCount',
    ),
    RegExp(
      'MaximumHistoryCount|MaximumVariableCount|MyInvocation|'
      'NestedPromptLevel|OutputEncoding|Pid|Profile|ProgressPreference',
    ),
    RegExp(
      'PSBoundParameters|PSCommandPath|PSCulture|PSDefaultParameterValues|'
      'PSEmailServer|PSHome|PSScriptRoot|PSSessionApplicationName',
    ),
    RegExp(
      'PSSessionConfigurationName|PSSessionOption|PSUICulture|'
      'PSVersionTable|Pwd|ShellId|StackTrace|VerbosePreference',
    ),
    RegExp('WarningPreference|WhatIfPreference'),

    RegExp('Event|EventArgs|EventSubscriber|Sender'),
    RegExp(
      'Matches|Ofs|ForEach|LastExitCode|PSCmdlet|PSItem|PSSenderInfo|This',
    ),
    RegExp('true|false|null'),
  ],
  prefix: r'\$',
  suffix: '',
);

final _builtins = _buildRegexp([
  _symbolBuiltins,
  _namedBuiltins,
  _variableBuiltins,
], suffix: _notCharacterOrDash);

final _grammar = [
  ('keyword', _keywords),
  ('number', _numbers),
  ('operator', _operators),
  ('builtin', _builtins),
  ('punctuation', _punctuation),
  ('variable', _identifiers),
];

final _quote = RegExp('["\']');

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

  if (stream.eat('(') != null) {
    state._bracketNesting += 1;
    return 'punctuation';
  }

  if (stream.eat(')') != null) {
    state._bracketNesting -= 1;
    return 'punctuation';
  }

  for (final (key, pattern) in _grammar) {
    if (stream.match(pattern) != null) return key;
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
    final quoteMatch = stream.eat(_quote);
    if (quoteMatch != null && stream.eol()) {
      state._tokenize = _tokenMultiString;
      state._startQuote = quoteMatch;
      return _tokenMultiString(stream, state);
    } else if (stream.eol()) {
      return 'error';
    } else if ('({'.contains(stream.peek()!)) {
      return 'punctuation';
    } else if (_varNames.hasMatch(stream.peek()!)) {
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
  } else if (ch != null && _varNames.hasMatch(ch)) {
    stream.eatWhile(_varNames);
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
