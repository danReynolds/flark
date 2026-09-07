import 'dart:async';

import 'highlight_transport.dart';
import 'model.dart';

/// One running job and at most one queued replacement. Superseded requests
/// resolve to null immediately; their output can never be adopted later.
final class HighlightQueue {
  HighlightQueue(this.transport);
  final HighlightTransport transport;
  _Job? _active, _pending;
  bool _disposed = false;

  Future<CodeAnalysis?> analyze(String source, CodeLanguage language) {
    if (_disposed) throw StateError('Highlight worker used after dispose.');
    validateCodeSource(source);
    _active?.cancel();
    _pending?.cancel();
    _pending = null;
    if (language == CodeLanguage.plain || source.length > 8192) {
      return Future.value(
        plainCodeAnalysis(
          source,
          language,
          source.length > 8192
              ? CodeAnalysisStatus.limit
              : CodeAnalysisStatus.plain,
        ),
      );
    }
    final job = _pending = _Job(source, language);
    if (_active == null) unawaited(_pump());
    return job.result.future;
  }

  Future<void> _pump() async {
    while (!_disposed && _pending != null) {
      final job = _active = _pending!;
      _pending = null;
      try {
        final bytes = await transport.analyze(job.source, job.language.index);
        if (!job.result.isCompleted) {
          job.result.complete(
            decodeCodeAnalysis(bytes, job.source, job.language),
          );
        }
      } catch (error, stack) {
        if (!job.result.isCompleted) job.result.completeError(error, stack);
      }
      _active = null;
    }
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _active?.cancel();
    _pending?.cancel();
    _pending = null;
    transport.dispose();
  }
}

final class _Job {
  _Job(this.source, this.language);
  final String source;
  final CodeLanguage language;
  final result = Completer<CodeAnalysis?>();
  void cancel() {
    if (!result.isCompleted) result.complete(null);
  }
}
