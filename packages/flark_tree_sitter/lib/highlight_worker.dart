/// Experimental asynchronous decoration lane. See PERFORMANCE_REVIEW.md.
library;

import 'src/highlight_queue.dart';
import 'src/highlight_native.dart'
    if (dart.library.js_interop) 'src/highlight_web.dart'
    as platform;
import 'src/model.dart';

/// Optional coloring worker; input, Markdown, selection and undo stay in Flark.
/// A result is valid only for its exact source and language. Paint current text
/// with the plain code style while
/// waiting. Never reuse ranges from a previous source revision.
final class CodeHighlightWorker {
  CodeHighlightWorker._(this._queue);
  final HighlightQueue _queue;

  /// Web assets must be served from the application's origin. Flutter bundles
  /// the defaults. Other web hosts pass URLs relative to their document or
  /// absolute URLs. Native uses an owned Dart isolate.
  static Future<CodeHighlightWorker> start({
    Uri? workerUri,
    Uri? wasmUri,
  }) async => CodeHighlightWorker._(
    HighlightQueue(
      await platform.startHighlightTransport(
        workerUri: workerUri,
        wasmUri: wasmUri,
      ),
    ),
  );

  /// New requests supersede older ones, which complete with null. At most one
  /// job runs and one waits; rapid typing cannot create an unbounded backlog.
  Future<CodeAnalysis?> analyze(
    String source, {
    required CodeLanguage language,
  }) => _queue.analyze(source, language);

  /// Resolves outstanding requests with null and terminates the worker.
  void dispose() => _queue.dispose();
}
