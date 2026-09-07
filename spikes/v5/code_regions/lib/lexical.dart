import 'dart:convert';

import 'package:shiki_flutter/engine.dart';
import 'package:shiki_flutter/langs.dart';
// Deliberate research-only use of unsupported internals. This is an adoption
// gate, not an endorsed dependency boundary for the production packages.
import 'package:shiki_flutter/src/textmate/basic_scopes.dart';
import 'package:shiki_flutter/src/textmate/encoded_token_metadata.dart';
import 'package:shiki_flutter/src/textmate/grammar.dart';
import 'package:shiki_flutter/src/textmate/raw_grammar.dart';
import 'package:shiki_flutter/src/textmate/registry.dart';
import 'package:shiki_flutter/src/textmate/theme.dart';

final class LexicalSpan {
  const LexicalSpan(this.start, this.end, this.type, this.scopes);
  final int start, end, type;
  final List<String> scopes;
  bool get literal => type != StandardTokenType.other;
  Map<String, Object> toJson() => {
    'start': start,
    'end': end,
    'type': type,
    'scopes': scopes,
  };
}

final class LexicalDocument {
  LexicalDocument(this.source, this.spans, this.lineEndTypes);
  final String source;
  final List<LexicalSpan> spans;
  final Map<int, int> lineEndTypes;
  int typeAt(int offset) {
    for (final span in spans) {
      if (span.start <= offset && offset < span.end) return span.type;
    }
    return StandardTokenType.other;
  }

  bool commentEndedAt(int lastCharacter, int lineEnd) =>
      typeAt(lastCharacter) == StandardTokenType.comment &&
      lineEndTypes[lineEnd] == StandardTokenType.other;
  bool literalAt(int offset) =>
      spans.any((s) => s.start <= offset && offset < s.end && s.literal);

  // Preserve coordinates and line endings while excluding literal delimiters.
  String get codeOnly {
    final units = source.codeUnits.toList();
    for (final span in spans.where((s) => s.literal)) {
      for (var i = span.start; i < span.end; i++) {
        if (units[i] != 10 && units[i] != 13) units[i] = 32;
      }
    }
    return String.fromCharCodes(units);
  }
}

final class TextMateProbe {
  TextMateProbe() {
    for (final language in languages.values) {
      _load(language);
    }
  }
  static final languages = {
    'dart': CodeLanguages.dart,
    'javascript': CodeLanguages.javascript,
    'typescript': CodeLanguages.typescript,
    'python': CodeLanguages.python,
    'yaml': CodeLanguages.yaml,
  };
  final _registry = SyncRegistry(
    Theme.createFromRawTheme(RawTheme(settings: [])),
    const ShikiHighlighterEmbeddedEngine(),
  );
  final _loaded = <String>{};
  final _attributes = BasicScopeAttributesProvider(0, null);

  int _type(List<String> scopes) {
    var type = StandardTokenType.other;
    for (final scope in scopes) {
      final next = _attributes.getBasicScopeAttributes(scope).tokenType;
      if (next != OptionalStandardTokenType.notSet) type = next;
    }
    return type;
  }

  void _load(CodeLanguage language) {
    if (!_loaded.add(language.scopeName)) return;
    final decoded = jsonDecode(language.json);
    for (final raw in decoded is List ? decoded : [decoded]) {
      _registry.addGrammar(RawGrammar.fromJson((raw as Map).cast()));
    }
    for (final embedded in language.embeddedLanguages()) {
      _load(embedded);
    }
  }

  // This changes theme data deliberately: lexical ranges and type must remain
  // identical. No public themed-token explanations are used for decisions.
  void changeTheme() => _registry.setTheme(
    Theme.createFromRawTheme(
      RawTheme(
        settings: [
          RawThemeSetting(
            scope: 'keyword',
            settings: ThemeSettingStyle(foreground: '#FF0000'),
          ),
        ],
      ),
    ),
  );

  LexicalDocument? tokenize(String language, String source) {
    final definition = languages[language];
    if (definition == null || source.length > 8192) return null;
    final grammar = _registry.grammarForScopeName(
      definition.scopeName,
      0,
      null,
      null,
      null,
    )!;
    StateStack? state;
    var offset = 0;
    final spans = <LexicalSpan>[];
    final lineEndTypes = <int, int>{};
    // Reparse the bounded snippet, including blank lines, to avoid inventing an
    // incremental invalidation engine before measuring whether one is needed.
    for (final line in source.split('\n')) {
      final result = grammar.tokenizeLine(line, state, 1000);
      if (result.stoppedEarly) return null;
      state = result.ruleStack;
      lineEndTypes[offset + line.length] = _type(
        state.contentNameScopesList?.getScopeNames() ?? const [],
      );
      for (final token in result.tokens) {
        final end = token.endIndex.clamp(0, line.length);
        if (token.startIndex >= end) continue;
        spans.add(
          LexicalSpan(
            offset + token.startIndex,
            offset + end,
            _type(token.scopes),
            token.scopes,
          ),
        );
      }
      if (offset + line.length < source.length) {
        spans.add(
          LexicalSpan(
            offset + line.length,
            offset + line.length + 1,
            StandardTokenType.other,
            const [],
          ),
        );
      }
      offset += line.length + 1;
    }
    return LexicalDocument(source, spans, lineEndTypes);
  }
}
