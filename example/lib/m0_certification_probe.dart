// M0 — certification probe.
//
//   cd example && dart run lib/m0_certification_probe.dart
//   cd example && taskpolicy -b dart run lib/m0_certification_probe.dart   # E-cores
//
// RFC 024 §4.4 requires that semantic formatting is applied ONLY to structure
// certified for the CURRENT source revision. The counterexample that forced
// that rule:
//
//     *hello*   →  delete the closing *  →  *hello
//
// If the engine keeps serving the old emphasis facts for the edited region
// after the edit, a caller that trusts them paints emphasis over text the
// parser is about to call literal — which is exactly the v2 failure class.
//
// This probe asks one question per case: IMMEDIATELY after an invalidating
// edit, and BEFORE convergence, what does the engine say about
//   (a) the edited region, and
//   (b) a distant, genuinely untouched region?
//
// The ideal answer is (a) uncertified/pending and (b) still certified — that is
// surgical certification. An acceptable answer is that BOTH go uncertified —
// correct but coarse. The bad answer is that (a) still reports the old
// semantics stamped with the old revision, leaving correctness to caller
// discipline.

import 'dart:async';
import 'dart:io';

import 'package:flark/flark_v3.dart';

class _Case {
  const _Case(this.label, this.source, this.editStart, this.editEnd,
      this.replacement, this.editedProbe, this.distantProbe);

  final String label;
  final String source;
  final int editStart;
  final int editEnd;
  final String replacement;

  /// A UTF-16 offset inside the construct the edit invalidates.
  final int editedProbe;

  /// A UTF-16 offset in a region the edit does not touch.
  final int distantProbe;
}

const String _tail = '\n\nDistant paragraph that the edit never touches.';

final List<_Case> _cases = <_Case>[
  // *hello* -> delete the closing '*' -> emphasis must stop.
  _Case('emphasis-opener-orphaned', '*hello* rest of line.$_tail', 6, 7, '', 2,
      30),
  // **bold** -> delete one '*' -> strong becomes emphasis.
  _Case('strong-demoted', '**bold** rest of line.$_tail', 7, 8, '', 3, 30),
  // `code` -> delete the closing backtick.
  _Case('code-span-orphaned', '`code` rest of line.$_tail', 5, 6, '', 2, 28),
  // Setext heading -> break the underline.
  _Case('setext-broken', 'Heading text\n====$_tail', 13, 17, 'x', 3, 25),
];

String _describe(FlarkV3DocumentQueryResult q) => switch (q) {
  FlarkV3DocumentPendingQuery() => 'PENDING',
  FlarkV3DocumentSourceGapQuery(:final structureRevision) =>
    'SOURCE-GAP(rev=$structureRevision)',
  FlarkV3RecursiveGreenPointQuery(:final structureRevision) =>
    'RECURSIVE-GREEN(rev=$structureRevision)',
  FlarkV3DocumentStructuralQuery(:final structureRevision) =>
    'STRUCTURAL(rev=$structureRevision)',
};

Future<void> _run(_Case c) async {
  FlarkV3DocumentRuntime? runtime;
  try {
    runtime = await FlarkV3DocumentRuntime.open(c.source);
    await runtime.initialReady.timeout(const Duration(seconds: 30));

    final beforeEdited = _describe(runtime.queryAtUtf16(c.editedProbe));
    final beforeRevision = runtime.status.structureRevision;

    runtime.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: runtime.sourceRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: c.editStart,
          endUtf16: c.editEnd,
          replacement: c.replacement,
        ),
      ),
    );

    // The critical moment: edited, not yet converged.
    final structureCurrentNow = runtime.status.structureCurrent;
    final afterEdited = _describe(runtime.queryAtUtf16(c.editedProbe));
    final afterDistant = _describe(runtime.queryAtUtf16(c.distantProbe));

    stdout.writeln(
      'cert ${c.label.padRight(26)} '
      'before=$beforeEdited(rev$beforeRevision) '
      '| structureCurrent=$structureCurrentNow '
      'edited=$afterEdited distant=$afterDistant',
    );
  } catch (error) {
    final text = error.toString().replaceAll('\n', ' ');
    stdout.writeln(
      'cert ${c.label.padRight(26)} THREW '
      '${text.length > 110 ? '${text.substring(0, 110)}…' : text}',
    );
  } finally {
    try {
      await runtime?.close().timeout(const Duration(seconds: 10));
    } catch (_) {}
  }
}

Future<void> main() async {
  stdout.writeln(
    '--- RFC 024 §4.4: is a query against an invalidated region still '
    'served old semantics? ---',
  );
  for (final c in _cases) {
    await _run(c);
  }
  exit(0);
}
