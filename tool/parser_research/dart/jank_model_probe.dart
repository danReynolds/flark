import 'dart:io';

import 'persistent_document.dart';

/// Desktop-VM stress receipt for the size dependence and long-tail allocation
/// behavior of the disposable Dart source tree.
///
/// This is deliberately not a Flutter frame benchmark. It isolates the work
/// that a source edit must do synchronously before parser or layout work.
void main(List<String> arguments) {
  final sizesMiB = arguments.isEmpty
      ? const <int>[1, 10, 100]
      : arguments.map(int.parse).toList(growable: false);

  for (final sizeMiB in sizesMiB) {
    final source = _sourceOfSize(sizeMiB * 1024 * 1024);
    final build = Stopwatch()..start();
    var document = PrototypePersistentDocument.fromString(source);
    build.stop();

    final cases = sizeMiB >= 100 ? 5000 : 20000;
    final samples = <int>[];
    final retainedUndoSnapshots = <PrototypePersistentDocument>[];
    final editOffset = document.utf16Length ~/ 2;

    for (var index = 0; index < cases; index += 1) {
      final stopwatch = Stopwatch()..start();
      document = document
          .apply(
            PrototypeDocumentEdit(
              baseRevision: document.revision,
              startUtf16: editOffset,
              endUtf16: editOffset + 1,
              replacement: index.isEven ? 'x' : 'y',
            ),
          )
          .document;
      // Exercise the viewport-slice path as well as the edit path.
      document.substring(editOffset - 64, editOffset + 64);
      stopwatch.stop();
      samples.add(stopwatch.elapsedMicroseconds);

      // Retain a bounded number of structurally shared revisions to model an
      // undo window and expose persistent-node retention/GC tails.
      if (index % 20 == 0) {
        retainedUndoSnapshots.add(document);
        if (retainedUndoSnapshots.length > 256) {
          retainedUndoSnapshots.removeAt(0);
        }
      }
    }

    samples.sort();
    stdout.writeln(
      'dart_source_jank size_mib=$sizeMiB cases=$cases '
      'build_ms=${build.elapsedMicroseconds / 1000} '
      'p50_us=${_percentile(samples, 50)} '
      'p95_us=${_percentile(samples, 95)} '
      'p99_us=${_percentile(samples, 99)} '
      'p999_us=${_percentile(samples, 999, scale: 1000)} '
      'max_us=${samples.last} '
      'rss_mib=${(ProcessInfo.currentRss / (1024 * 1024)).round()} '
      'undo_snapshots=${retainedUndoSnapshots.length} '
      'hash=${document.contentHash32}',
    );
  }
}

String _sourceOfSize(int size) {
  const line =
      'A source line with **bold**, *emphasis*, `code`, and a link target.\n';
  final chunk = StringBuffer();
  while (chunk.length < 64 * 1024) {
    chunk.write(line);
  }
  final chunkText = chunk.toString();
  final fullChunks = size ~/ chunkText.length;
  final remainder = size % chunkText.length;
  return '${List<String>.filled(fullChunks, chunkText).join()}'
      '${chunkText.substring(0, remainder)}';
}

int _percentile(List<int> values, int percentile, {int scale = 100}) =>
    values[((values.length - 1) * percentile) ~/ scale];
