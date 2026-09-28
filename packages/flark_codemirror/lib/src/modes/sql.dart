// Ported from @codemirror/legacy-modes 6.5.4 mode/sql.js, its `standardSQL`,
// `pgSQL` and `mySQL` exports: the shared `sql` stream parser and the three
// configurations. The other dialects and their tables are not ported.
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

import '../mode.dart';
import '../stream.dart';

typedef _Tokenizer = String? Function(StringStream stream, SqlState state);

/// A dialect's token for a character; the ported hooks never decline.
typedef _Hook = String? Function(StringStream stream);

/// `parserConfig` of the upstream `sql` function, as far as the ported
/// configurations set it. Character classes are given as their characters.
final class _Dialect {
  _Dialect({
    required this.keywords,
    required this.builtin,
    required this.atoms,
    required this.dateSQL,
    required this.support,
    this.client = const {},
    this.operatorChars = '*+-%<>!=&|~^/',
    this.hooks = const {},
    this.backslashStringEscapes = true,
  });
  final Set<String> client, atoms, builtin, keywords, dateSQL, support;
  final String operatorChars;
  final Map<String, _Hook> hooks;
  final bool backslashStringEscapes;
}

// The default `brackets` and `punctuation`.
const _brackets = '{}()[]';
const _punctuation = ';.,:';

final _hexAfterZero = RegExp('[xX][0-9a-fA-F]+');
final _hexQuoted = RegExp("'[0-9a-fA-F]*'");
final _binaryQuoted = RegExp("'[01]+'");
final _binaryAfterZero = RegExp('b[01]*');
final _numberRest = RegExp(r'[0-9]*(\.[0-9]+)?([eE][-+]?[0-9]+)?');
final _trailingDot = RegExp(r'\.(?!\.)');
final _charsetName = RegExp('[a-z][a-z0-9]*', caseSensitive: false);
final _zerolessFloat = RegExp(r'(?:\d+(?:e[+-]?\d+)?)', caseSensitive: false);
final _dots = RegExp(r'\.+');
final _odbcTable = RegExp(r'[\w\d_$#]+');
final _odbcDateSingle = RegExp(r"( )*(d|D|t|T|ts|TS)( )*'[^']*'( )*\}");
final _odbcDateDouble = RegExp(r'( )*(d|D|t|T|ts|TS)( )*"[^"]*"( )*\}');
final _dateSingle = RegExp(r"( )+'[^']*'");
final _dateDouble = RegExp(r'( )+"[^"]*"');
final _commentMark = RegExp(r'.*?(\/\*|\*\/)');
final _toSingle = RegExp(".*'");
final _toDouble = RegExp('.*"');
final _toBacktick = RegExp('.*`');
final _variableName = RegExp(r'[0-9a-zA-Z$._]+');
final _clientCommand = RegExp('[a-zA-Z.#!?]');

/// `\w`, and upstream's `[_\w\d]`, for one code unit.
bool _isWordUnit(int u) =>
    (u >= 0x30 && u <= 0x39) ||
    (u >= 0x41 && u <= 0x5a) ||
    (u >= 0x61 && u <= 0x7a) ||
    u == 0x5f;

/// JavaScript's `toLowerCase` where it differs from Dart's in a way the
/// ASCII tables can see: U+0130 lowercases to `i` and a combining dot.
String _lowerCase(String s) => s.replaceAll('\u0130', 'i\u0307').toLowerCase();

// `identifier`
String? _hookIdentifier(StringStream stream) {
  // MySQL/MariaDB identifiers
  String? ch;
  while ((ch = stream.next()) != null) {
    if (ch == '`' && stream.eat('`') == null) return 'string.special';
  }
  stream.backUp(stream.current().length - 1);
  return stream.eatWhileCode(_isWordUnit) ? 'string.special' : null;
}

// variable token
String? _hookVar(StringStream stream) {
  // variables
  // @@prefix.varName @varName
  // varName can be quoted with ` or ' or "
  if (stream.eat('@') != null) {
    stream.matchString('session.');
    stream.matchString('local.');
    stream.matchString('global.');
  }

  if (stream.eat("'") != null) {
    stream.match(_toSingle);
    return 'string.special';
  } else if (stream.eat('"') != null) {
    stream.match(_toDouble);
    return 'string.special';
  } else if (stream.eat('`') != null) {
    stream.match(_toBacktick);
    return 'string.special';
  } else if (stream.match(_variableName) != null) {
    return 'string.special';
  }
  return null;
}

// short client keyword token
String? _hookClient(StringStream stream) {
  // \N means NULL
  if (stream.eat('N') != null) return 'atom';
  // \g, etc
  return stream.match(_clientCommand) != null ? 'string.special' : null;
}

// these keywords are used by all SQL dialects (however, a mode can still
// overwrite it)
const _sqlKeywords =
    'alter and as asc between by count create delete desc distinct drop from '
    'group having in insert into is join like not on or order select set table '
    'union update values where limit ';

/// Upstream's `set`: a space-separated list as a set of its words.
Set<String> _set(String str) => str.split(' ').toSet();

const _defaultBuiltin =
    'bool boolean bit blob enum long longblob longtext medium mediumblob '
    'mediumint mediumtext time timestamp tinyblob tinyint tinytext text bigint '
    'int int1 int2 int3 int4 int8 integer float float4 float8 double char '
    'varbinary varchar varcharacter precision real date datetime year unsigned '
    'signed decimal numeric';

// A generic SQL Mode. It's not a standard, it just try to support what is
// generally supported
final _standardSql = _Dialect(
  keywords: _set('${_sqlKeywords}begin'),
  builtin: _set(_defaultBuiltin),
  atoms: _set('false true null unknown'),
  dateSQL: _set('date time timestamp'),
  support: _set('ODBCdotTable doubleQuote binaryNumber hexNumber'),
);

final _mySql = _Dialect(
  client: _set(
    'charset clear connect edit ego exit go help nopager notee nowarning pager '
    'print prompt quit rehash source status system tee',
  ),
  keywords: _set(
    '${_sqlKeywords}accessible action add after algorithm all analyze '
    'asensitive at authors auto_increment autocommit avg avg_row_length before '
    'binary binlog both btree cache call cascade cascaded case catalog_name '
    'chain change changed character check checkpoint checksum class_origin '
    'client_statistics close coalesce code collate collation collations column '
    'columns comment commit committed completion concurrent condition '
    'connection consistent constraint contains continue contributors convert '
    'cross current current_date current_time current_timestamp current_user '
    'cursor data database databases day_hour day_microsecond day_minute '
    'day_second deallocate dec declare default delay_key_write delayed '
    'delimiter des_key_file describe deterministic dev_pop dev_samp deviance '
    'diagnostics directory disable discard distinctrow div dual dumpfile each '
    'elseif enable enclosed end ends engine engines enum errors escape escaped '
    'even event events every execute exists exit explain extended fast fetch '
    'field fields first flush for force foreign found_rows full fulltext '
    'function general get global grant grants group group_concat handler hash '
    'help high_priority hosts hour_microsecond hour_minute hour_second if '
    'ignore ignore_server_ids import index index_statistics infile inner '
    'innodb inout insensitive insert_method install interval invoker isolation '
    'iterate key keys kill language last leading leave left level limit linear '
    'lines list load local localtime localtimestamp lock logs low_priority '
    'master master_heartbeat_period master_ssl_verify_server_cert masters '
    'match max max_rows maxvalue message_text middleint migrate min min_rows '
    'minute_microsecond minute_second mod mode modifies modify mutex '
    'mysql_errno natural next no no_write_to_binlog offline offset one online '
    'open optimize option optionally out outer outfile pack_keys parser '
    'partition partitions password phase plugin plugins prepare preserve prev '
    'primary privileges procedure processlist profile profiles purge query '
    'quick range read read_write reads real rebuild recover references regexp '
    'relaylog release remove rename reorganize repair repeatable replace '
    'require resignal restrict resume return returns revoke right rlike '
    'rollback rollup row row_format rtree savepoint schedule schema '
    'schema_name schemas second_microsecond security sensitive separator '
    'serializable server session share show signal slave slow smallint '
    'snapshot soname spatial specific sql sql_big_result sql_buffer_result '
    'sql_cache sql_calc_found_rows sql_no_cache sql_small_result sqlexception '
    'sqlstate sqlwarning ssl start starting starts status std stddev '
    'stddev_pop stddev_samp storage straight_join subclass_origin sum suspend '
    'table_name table_statistics tables tablespace temporary terminated to '
    'trailing transaction trigger triggers truncate uncommitted undo uninstall '
    'unique unlock upgrade usage use use_frm user user_resources '
    'user_statistics using utc_date utc_time utc_timestamp value variables '
    'varying view views warnings when while with work write xa xor year_month '
    'zerofill begin do then else loop repeat',
  ),
  builtin: _set(
    'bool boolean bit blob decimal double float long longblob longtext medium '
    'mediumblob mediumint mediumtext time timestamp tinyblob tinyint tinytext '
    'text bigint int int1 int2 int3 int4 int8 integer float float4 float8 '
    'double char varbinary varchar varcharacter precision date datetime year '
    'unsigned signed numeric',
  ),
  atoms: _set('false true null unknown'),
  operatorChars: '*+-%<>!=&|^',
  dateSQL: _set('date time timestamp'),
  support: _set(
    'ODBCdotTable decimallessFloat zerolessFloat binaryNumber hexNumber '
    'doubleQuote nCharCast charsetCast commentHash commentSpaceRequired',
  ),
  hooks: const {'@': _hookVar, '`': _hookIdentifier, r'\': _hookClient},
);

final _pgSql = _Dialect(
  client: _set('source'),
  keywords: _set(
    '${_sqlKeywords}a abort abs absent absolute access according action ada '
    'add admin after aggregate alias all allocate also alter always analyse '
    'analyze and any are array array_agg array_max_cardinality as asc '
    'asensitive assert assertion assignment asymmetric at atomic attach '
    'attribute attributes authorization avg backward base64 before begin '
    'begin_frame begin_partition bernoulli between bigint binary bit '
    'bit_length blob blocked bom boolean both breadth by c cache call called '
    'cardinality cascade cascaded case cast catalog catalog_name ceil ceiling '
    'chain char char_length character character_length character_set_catalog '
    'character_set_name character_set_schema characteristics characters check '
    'checkpoint class class_origin clob close cluster coalesce cobol collate '
    'collation collation_catalog collation_name collation_schema collect '
    'column column_name columns command_function command_function_code comment '
    'comments commit committed concurrently condition condition_number '
    'configuration conflict connect connection connection_name constant '
    'constraint constraint_catalog constraint_name constraint_schema '
    'constraints constructor contains content continue control conversion '
    'convert copy corr corresponding cost count covar_pop covar_samp create '
    'cross csv cube cume_dist current current_catalog current_date '
    'current_default_transform_group current_path current_role current_row '
    'current_schema current_time current_timestamp '
    'current_transform_group_for_type current_user cursor cursor_name cycle '
    'data database datalink datatype date datetime_interval_code '
    'datetime_interval_precision day db deallocate debug dec decimal declare '
    'default defaults deferrable deferred defined definer degree delete '
    'delimiter delimiters dense_rank depends depth deref derived desc describe '
    'descriptor detach detail deterministic diagnostics dictionary disable '
    'discard disconnect dispatch distinct dlnewcopy dlpreviouscopy '
    'dlurlcomplete dlurlcompleteonly dlurlcompletewrite dlurlpath '
    'dlurlpathonly dlurlpathwrite dlurlscheme dlurlserver dlvalue do document '
    'domain double drop dump dynamic dynamic_function dynamic_function_code '
    'each element else elseif elsif empty enable encoding encrypted end '
    'end_frame end_partition endexec enforced enum equals errcode error escape '
    'event every except exception exclude excluding exclusive exec execute '
    'exists exit exp explain expression extension external extract false '
    'family fetch file filter final first first_value flag float floor '
    'following for force foreach foreign fortran forward found frame_row free '
    'freeze from fs full function functions fusion g general generated get '
    'global go goto grant granted greatest group grouping groups handler '
    'having header hex hierarchy hint hold hour id identity if ignore ilike '
    'immediate immediately immutable implementation implicit import in include '
    'including increment indent index indexes indicator info inherit inherits '
    'initially inline inner inout input insensitive insert instance '
    'instantiable instead int integer integrity intersect intersection '
    'interval into invoker is isnull isolation join k key key_member key_type '
    'label lag language large last last_value lateral lead leading leakproof '
    'least left length level library like like_regex limit link listen ln load '
    'local localtime localtimestamp location locator lock locked log logged '
    'loop lower m map mapping match matched materialized max max_cardinality '
    'maxvalue member merge message message_length message_octet_length '
    'message_text method min minute minvalue mod mode modifies module month '
    'more move multiset mumps name names namespace national natural nchar '
    'nclob nesting new next nfc nfd nfkc nfkd nil no none normalize normalized '
    'not nothing notice notify notnull nowait nth_value ntile null nullable '
    'nullif nulls number numeric object occurrences_regex octet_length octets '
    'of off offset oids old on only open operator option options or order '
    'ordering ordinality others out outer output over overlaps overlay '
    'overriding owned owner p pad parallel parameter parameter_mode '
    'parameter_name parameter_ordinal_position parameter_specific_catalog '
    'parameter_specific_name parameter_specific_schema parser partial '
    'partition pascal passing passthrough password path percent percent_rank '
    'percentile_cont percentile_disc perform period permission pg_context '
    'pg_datatype_name pg_exception_context pg_exception_detail '
    'pg_exception_hint placing plans pli policy portion position '
    'position_regex power precedes preceding precision prepare prepared '
    'preserve primary print_strict_params prior privileges procedural '
    'procedure procedures program public publication query quote raise range '
    'rank read reads real reassign recheck recovery recursive ref references '
    'referencing refresh regr_avgx regr_avgy regr_count regr_intercept regr_r2 '
    'regr_slope regr_sxx regr_sxy regr_syy reindex relative release rename '
    'repeatable replace replica requiring reset respect restart restore '
    'restrict result result_oid return returned_cardinality returned_length '
    'returned_octet_length returned_sqlstate returning returns reverse revoke '
    'right role rollback rollup routine routine_catalog routine_name '
    'routine_schema routines row row_count row_number rows rowtype rule '
    'savepoint scale schema schema_name schemas scope scope_catalog scope_name '
    'scope_schema scroll search second section security select selective self '
    'sensitive sequence sequences serializable server server_name session '
    'session_user set setof sets share show similar simple size skip slice '
    'smallint snapshot some source space specific specific_name specifictype '
    'sql sqlcode sqlerror sqlexception sqlstate sqlwarning sqrt stable stacked '
    'standalone start state statement static statistics stddev_pop stddev_samp '
    'stdin stdout storage strict strip structure style subclass_origin '
    'submultiset subscription substring substring_regex succeeds sum symmetric '
    'sysid system system_time system_user t table table_name tables '
    'tablesample tablespace temp template temporary text then ties time '
    'timestamp timezone_hour timezone_minute to token top_level_count trailing '
    'transaction transaction_active transactions_committed '
    'transactions_rolled_back transform transforms translate translate_regex '
    'translation treat trigger trigger_catalog trigger_name trigger_schema '
    'trim trim_array true truncate trusted type types uescape unbounded '
    'uncommitted under unencrypted union unique unknown unlink unlisten '
    'unlogged unnamed unnest until untyped update upper uri usage use_column '
    'use_variable user user_defined_type_catalog user_defined_type_code '
    'user_defined_type_name user_defined_type_schema using vacuum valid '
    'validate validator value value_of values var_pop var_samp varbinary '
    'varchar variable_conflict variadic varying verbose version versioning '
    'view views volatile warning when whenever where while whitespace '
    'width_bucket window with within without work wrapper write xml xmlagg '
    'xmlattributes xmlbinary xmlcast xmlcomment xmlconcat xmldeclaration '
    'xmldocument xmlelement xmlexists xmlforest xmliterate xmlnamespaces '
    'xmlparse xmlpi xmlquery xmlroot xmlschema xmlserialize xmltable xmltext '
    'xmlvalidate year yes zone',
  ),
  builtin: _set(
    'bigint int8 bigserial serial8 bit varying varbit boolean bool box bytea '
    'character char varchar cidr circle date double precision float8 inet '
    'integer int int4 interval json jsonb line lseg macaddr macaddr8 money '
    'numeric decimal path pg_lsn point polygon real float4 smallint int2 '
    'smallserial serial2 serial serial4 text time without zone with timetz '
    'timestamp timestamptz tsquery tsvector txid_snapshot uuid xml',
  ),
  atoms: _set('false true null unknown'),
  operatorChars: '*/+-%<>!=&|^/#@?~',
  backslashStringEscapes: false,
  dateSQL: _set('date time timestamp'),
  support: _set(
    'ODBCdotTable decimallessFloat zerolessFloat binaryNumber hexNumber '
    'nCharCast charsetCast escapeConstant',
  ),
);

/// A parenthesized or bracketed context.
final class _Context {
  _Context(this.prev, this.indent, this.col, this.type);
  final _Context? prev;
  final int indent, col;
  final String type;

  /// Unset until a token follows the opening one. Upstream sets it in
  /// place, so copies of a state share it.
  bool? align;
}

final class SqlState {
  SqlState._(this._tokenize, this._context);
  _Tokenizer _tokenize;
  _Context? _context;

  /// The upstream default copy: contexts are shared.
  SqlState copy() => SqlState._(_tokenize, _context);
}

/// SQL: the upstream `sql` stream parser in one of its configurations.
final class SqlMode extends Mode<SqlState> {
  SqlMode._(super.config, this._dialect);

  /// `standardSQL`: generic SQL.
  SqlMode.standardSql([ModeConfig config = const ModeConfig()])
    : this._(config, _standardSql);

  /// `pgSQL`: PostgreSQL.
  SqlMode.pgSql([ModeConfig config = const ModeConfig()])
    : this._(config, _pgSql);

  /// `mySQL`: MySQL.
  SqlMode.mySql([ModeConfig config = const ModeConfig()])
    : this._(config, _mySql);

  final _Dialect _dialect;

  late final _Tokenizer _tokenBaseT = _tokenBase;

  String? _tokenBase(StringStream stream, SqlState state) {
    final ch = stream.next()!;

    // call hooks from the mime type
    final hook = _dialect.hooks[ch];
    if (hook != null) return hook(stream);

    final support = _dialect.support;
    if (support.contains('hexNumber') &&
        ((ch == '0' && stream.match(_hexAfterZero) != null) ||
            (ch == 'x' || ch == 'X') && stream.match(_hexQuoted) != null)) {
      // hex
      return 'number';
    } else if (support.contains('binaryNumber') &&
        (((ch == 'b' || ch == 'B') && stream.match(_binaryQuoted) != null) ||
            (ch == '0' && stream.match(_binaryAfterZero) != null))) {
      // bitstring
      return 'number';
    } else if (ch.codeUnitAt(0) > 47 && ch.codeUnitAt(0) < 58) {
      // numbers
      stream.match(_numberRest);
      if (support.contains('decimallessFloat')) stream.match(_trailingDot);
      return 'number';
    } else if (ch == '?' &&
        (stream.eatSpace() || stream.eol() || stream.eat(';') != null)) {
      // placeholders
      return 'macroName';
    } else if (ch == "'" || (ch == '"' && support.contains('doubleQuote'))) {
      // strings
      state._tokenize = _tokenLiteral(ch);
      return state._tokenize(stream, state);
    } else if (((support.contains('nCharCast') && (ch == 'n' || ch == 'N')) ||
            (support.contains('charsetCast') &&
                ch == '_' &&
                stream.match(_charsetName) != null)) &&
        (stream.peek() == "'" || stream.peek() == '"')) {
      // charset casting: _utf8'str', N'str', n'str'
      return 'keyword';
    } else if (support.contains('escapeConstant') &&
        (ch == 'e' || ch == 'E') &&
        (stream.peek() == "'" ||
            (stream.peek() == '"' && support.contains('doubleQuote')))) {
      // escape constant: E'str', e'str'
      state._tokenize = (stream, state) {
        final literal = state._tokenize = _tokenLiteral(stream.next()!, true);
        return literal(stream, state);
      };
      return 'keyword';
    } else if (support.contains('commentSlashSlash') &&
        ch == '/' &&
        stream.eat('/') != null) {
      // 1-line comment
      stream.skipToEnd();
      return 'comment';
    } else if ((support.contains('commentHash') && ch == '#') ||
        (ch == '-' &&
            stream.eat('-') != null &&
            (!support.contains('commentSpaceRequired') ||
                stream.eat(' ') != null))) {
      // 1-line comments
      stream.skipToEnd();
      return 'comment';
    } else if (ch == '/' && stream.eat('*') != null) {
      // multi-line comments
      state._tokenize = _tokenComment(1);
      return state._tokenize(stream, state);
    } else if (ch == '.') {
      // .1 for 0.1
      if (support.contains('zerolessFloat') &&
          stream.match(_zerolessFloat) != null) {
        return 'number';
      }
      if (stream.match(_dots) != null) return null;
      // .table_name (ODBC)
      if (support.contains('ODBCdotTable') &&
          stream.match(_odbcTable) != null) {
        return 'type';
      }
    } else if (_dialect.operatorChars.contains(ch)) {
      // operators
      final operatorChars = _dialect.operatorChars;
      stream.eatWhile((String ch) => operatorChars.contains(ch));
      return 'operator';
    } else if (_brackets.contains(ch)) {
      // brackets
      return 'bracket';
    } else if (_punctuation.contains(ch)) {
      // punctuation
      stream.eatWhile((String ch) => _punctuation.contains(ch));
      return 'punctuation';
    } else if (ch == '{' &&
        (stream.match(_odbcDateSingle) != null ||
            stream.match(_odbcDateDouble) != null)) {
      // dates (weird ODBC syntax)
      return 'number';
    } else {
      stream.eatWhileCode(_isWordUnit);
      final word = _lowerCase(stream.current());
      // dates (standard SQL syntax)
      if (_dialect.dateSQL.contains(word) &&
          (stream.match(_dateSingle) != null ||
              stream.match(_dateDouble) != null)) {
        return 'number';
      }
      if (_dialect.atoms.contains(word)) return 'atom';
      if (_dialect.builtin.contains(word)) return 'type';
      if (_dialect.keywords.contains(word)) return 'keyword';
      if (_dialect.client.contains(word)) return 'builtin';
      return null;
    }
    return null;
  }

  // 'string', with char specified in quote escaped by '\'
  _Tokenizer _tokenLiteral(String quote, [bool backslashEscapes = false]) =>
      (stream, state) {
        var escaped = false;
        String? ch;
        while ((ch = stream.next()) != null) {
          if (ch == quote && !escaped) {
            state._tokenize = _tokenBaseT;
            break;
          }
          escaped =
              (_dialect.backslashStringEscapes || backslashEscapes) &&
              !escaped &&
              ch == r'\';
        }
        return 'string';
      };

  _Tokenizer _tokenComment(int depth) => (stream, state) {
    final m = stream.match(_commentMark);
    if (m == null) {
      stream.skipToEnd();
    } else if (m[1] == '/*') {
      state._tokenize = _tokenComment(depth + 1);
    } else if (depth > 1) {
      state._tokenize = _tokenComment(depth - 1);
    } else {
      state._tokenize = _tokenBaseT;
    }
    return 'comment';
  };

  void _pushContext(StringStream stream, SqlState state, String type) {
    state._context = _Context(
      state._context,
      stream.indentation(),
      stream.column(),
      type,
    );
  }

  // Upstream also records the popped context's indentation in the state,
  // which nothing reads.
  void _popContext(SqlState state) {
    state._context = state._context!.prev;
  }

  @override
  SqlState startState([int baseColumn = 0]) => SqlState._(_tokenBaseT, null);

  @override
  SqlState copyState(SqlState state) => state.copy();

  @override
  String? token(StringStream stream, SqlState state) {
    if (stream.sol()) {
      final context = state._context;
      if (context != null && context.align == null) context.align = false;
    }
    if (identical(state._tokenize, _tokenBaseT) && stream.eatSpace()) {
      return null;
    }

    final style = state._tokenize(stream, state);
    if (style == 'comment') return style;

    final context = state._context;
    if (context != null && context.align == null) context.align = true;

    final tok = stream.current();
    if (tok == '(') {
      _pushContext(stream, state, ')');
    } else if (tok == '[') {
      _pushContext(stream, state, ']');
    } else if (context != null && context.type == tok) {
      _popContext(state);
    }
    return style;
  }

  @override
  bool get hasIndent => true;

  @override
  int? indent(SqlState state, String textAfter, String line) {
    final cx = state._context;
    if (cx == null) return null;
    final closing = (textAfter.isEmpty ? '' : textAfter[0]) == cx.type;
    if (cx.align == true) return cx.col + (closing ? 0 : 1);
    return cx.indent + (closing ? 0 : config.indentUnit);
  }
}
