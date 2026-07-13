import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// RFC 022 (parser grammar monopoly): grammar belongs to the parser;
/// everything else is geometry. The textual-analysis scanner libraries are
/// the sanctioned exception — proposal generators and mapping-layer synthesis
/// that Phases 1–4 progressively demote or delete — so the set of modules
/// allowed to import them is pinned here, both directions:
///
/// * a NEW import site fails this test until it is deliberately added to the
///   allowlist AND to the disposition table in
///   `docs/architecture/rfc/rfc_022_parser_grammar_monopoly.md`;
/// * a REMOVED import site fails until the pin shrinks, so the RFC table
///   stays honest as phases land.
void main() {
  const scannerLibraries = [
    'src/v2/markdown/inline/flark_inline_delimiter_placement.dart',
    'src/v2/markdown/inline/flark_inline_flanking.dart',
    'src/v2/markdown/inline/flark_inline_run_scanner.dart',
    'src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart',
  ];

  // Keep in sync with the disposition table in RFC 022 §3.
  const allowedImporters = {
    'lib/src/v2/flutter/flark_flutter_controller.dart',
    'lib/src/v2/flutter/flark_live_code_fence_input_policy.dart',
    'lib/src/v2/flutter/flark_live_edit_classifier.dart',
    'lib/src/v2/flutter/flark_markdown_input_policy.dart',
    'lib/src/v2/flutter/flark_projected_editable_text.dart',
    'lib/src/v2/markdown/commands/flark_markdown_command_capabilities.dart',
    'lib/src/v2/markdown/commands/flark_markdown_inline_commands.dart',
    // NOTE: the parse backend imports ONLY the fenced-code scanner now
    // (fence synthesis; Phase 4 deletes it). Its inline-flanking import was
    // removed by bridge protocol v2 — link/image markup now arrives as
    // AST-derived per-token ranges instead of Dart source scanning.
    'lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart',
    'lib/src/v2/markdown/source/flark_markdown_fenced_code_policy.dart',
    'lib/src/v2/markdown/source/flark_markdown_input_engine.dart',
    'lib/src/v2/projection/flark_projected_text_edit_adapter.dart',
    'lib/src/v2/projection/flark_projection.dart',
  };

  test('grammar scanner imports match the RFC 022 allowlist exactly', () {
    final scannerFileNames = scannerLibraries
        .map((path) => path.split('/').last)
        .toSet();
    final importers = <String>{};

    final dartFiles = Directory('lib')
        .listSync(recursive: true)
        .whereType<File>()
        .where((file) => file.path.endsWith('.dart'));
    for (final file in dartFiles) {
      final normalized = file.path.replaceAll(r'\', '/');
      // The scanner libraries themselves may import each other freely.
      if (scannerFileNames.contains(normalized.split('/').last)) continue;
      final source = file.readAsStringSync();
      final importsScanner = scannerFileNames.any(
        (name) => RegExp(
          '''import\\s+['"][^'"]*$name['"]''',
        ).hasMatch(source),
      );
      if (importsScanner) importers.add(normalized);
    }

    final unexpected = importers.difference(allowedImporters);
    final missing = allowedImporters.difference(importers);
    expect(
      unexpected,
      isEmpty,
      reason:
          'New grammar-scanner import site(s). Grammar questions belong to '
          'the parser (RFC 022); if this use is genuinely a sanctioned '
          'proposal generator, add it to this allowlist AND the RFC 022 §3 '
          'disposition table in the same change.',
    );
    expect(
      missing,
      isEmpty,
      reason:
          'Import site(s) no longer use the scanners — shrink this allowlist '
          'and the RFC 022 §3 table so the sanctioned surface stays honest.',
    );
  });
}
