import 'dart:async';

import 'package:flark/flark_v3.dart';

Future<void> main() async {
  final runtime = await FlarkV3DocumentRuntime.open(
    '# Archive consumer\n\n**Live** Markdown.\n',
  );
  try {
    await runtime.initialReady.timeout(const Duration(seconds: 30));
    final status = runtime.status.structureCurrent
        ? runtime.status
        : await runtime.statuses
              .firstWhere((candidate) => candidate.structureCurrent)
              .timeout(const Duration(seconds: 30));
    if (!status.sourceCurrent ||
        status.structureRevision != runtime.sourceRevision) {
      throw StateError('The archive-backed native runtime did not converge.');
    }
    final previousRevision = runtime.sourceRevision;
    runtime.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: previousRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: '> ',
        ),
      ),
    );
    final edited = await runtime.statuses
        .firstWhere(
          (candidate) =>
              candidate.structureCurrent &&
              candidate.structureRevision == runtime.sourceRevision,
        )
        .timeout(const Duration(seconds: 30));
    if (runtime.sourceRevision != previousRevision + 1 ||
        !edited.sourceCurrent ||
        runtime.readSourceRange(0, 2) != '> ' ||
        !runtime.exportMarkdown().startsWith('> # Archive consumer')) {
      throw StateError('The archive-backed native runtime did not edit.');
    }
    print('Archive-backed native Flark runtime passed.');
  } finally {
    await runtime.close().timeout(const Duration(seconds: 30));
  }
}
