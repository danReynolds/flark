import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('ingest preserves lines and aggregates exact coordinates', () {
    const raw = 'alpha 😀 café\r\nβeta 𝄞\rlast';
    final document = FlarkV3SourceDocument.fromString(raw, chunkSize: 7);

    expect(document.toString(), raw);
    expect(document.utf16Length, raw.length);
    expect(document.utf8Length, utf8.encode(raw).length);
    expect(document.lineCount, 3);
    expect(document.lineStartUtf16(0), 0);
    expect(document.lineStartUtf16(1), raw.indexOf('\n') + 1);
    expect(document.lineStartUtf16(2), raw.lastIndexOf('\r') + 1);

    for (var offset = 0; offset <= raw.length; offset += 1) {
      if (!_isScalarBoundary(raw, offset)) continue;
      final expectedUtf8 = utf8.encode(raw.substring(0, offset)).length;
      expect(document.utf16ToUtf8(offset), expectedUtf8);
      expect(document.utf8ToUtf16(expectedUtf8), offset);
      expect(document.lineAtUtf16(offset), _logicalLineAt(raw, offset));
    }

    const fingerprintGoldens = <String, FlarkV3ContentHash128>{
      'a😀 café β\n': FlarkV3ContentHash128(
        0xB991EDD9,
        0x5FB57C47,
        0x88732115,
        0x2292A46B,
      ),
      'a🌍b\n': FlarkV3ContentHash128(
        0xCC6C28F6,
        0x0AA80A4C,
        0xDF5F6342,
        0x250AFFB0,
      ),
      'aé🌍b\n': FlarkV3ContentHash128(
        0x9CFB81CC,
        0x8CB1DEFA,
        0x6F97A348,
        0x6F98E8EE,
      ),
      'aéb\n': FlarkV3ContentHash128(
        0x91674F8C,
        0x5D5AB6CE,
        0xB9ACAB58,
        0x359E37F2,
      ),
    };
    for (final MapEntry(key: source, value: expected)
        in fingerprintGoldens.entries) {
      expect(
        FlarkV3SourceDocument.fromString(source).contentHash128,
        expected,
        reason: 'all four UTF-8 polynomial lanes are protocol goldens',
      );
    }
    final fingerprintGolden = FlarkV3SourceDocument.fromString('a😀 café β\n');
    expect(
      FlarkV3SourceDocument.fromString(
        'a😀 café β\n',
        chunkSize: 3,
      ).contentHash128,
      fingerprintGolden.contentHash128,
      reason: 'all 128 bits are independent of tree chunking',
    );
  });

  test('CRLF stays one line break across every source-tree boundary', () {
    const source = 'a\r\nb\rc\nd';
    for (final chunkSize in [2, 3, 4, 7]) {
      final document = FlarkV3SourceDocument.fromString(
        source,
        chunkSize: chunkSize,
      );
      expect(document.toString(), source, reason: 'chunkSize=$chunkSize');
      expect(document.lineCount, 4, reason: 'chunkSize=$chunkSize');
      expect(
        [for (var line = 0; line < 4; line += 1) document.lineStartUtf16(line)],
        [0, 3, 5, 7],
        reason: 'chunkSize=$chunkSize',
      );
      for (var offset = 0; offset <= source.length; offset += 1) {
        expect(
          document.lineAtUtf16(offset),
          _logicalLineAt(source, offset),
          reason: 'chunkSize=$chunkSize offset=$offset',
        );
      }
    }
  });

  test('edits can split and re-form CRLF without changing source spelling', () {
    var document = FlarkV3SourceDocument.fromString('a\r\nb', chunkSize: 2);
    var applied = document.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: document.revision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 2,
          endUtf16: 3,
          replacement: '',
        ),
      ),
    );
    document = applied.document;
    expect(document.toString(), 'a\rb');
    expect(document.lineCount, 2);
    expect(document.lineStartUtf16(1), 2);

    applied = document.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: document.revision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 2,
          endUtf16: 2,
          replacement: '\n',
        ),
      ),
    );
    document = applied.document;
    expect(document.toString(), 'a\r\nb');
    expect(document.lineCount, 2);
    expect(document.lineStartUtf16(1), 3);
    expect(
      utf8.decode(applied.parserBatch!.operations.single.replacementUtf8),
      '\n',
    );
  });

  test('random CR LF edits preserve exact line aggregates', () {
    var oracle = List.filled(40, 'a\r\nb\rc\nd').join('|');
    var document = FlarkV3SourceDocument.fromString(oracle, chunkSize: 3);
    var seed = 0x4C494E45;
    const replacements = ['', '\r', '\n', '\r\n', 'x', '\n\r'];

    for (var iteration = 0; iteration < 1200; iteration += 1) {
      seed = _next(seed);
      final start = seed % (oracle.length + 1);
      seed = _next(seed);
      final deleted = oracle.isEmpty ? 0 : seed % 3;
      final end = math.min(start + deleted, oracle.length);
      seed = _next(seed);
      final replacement = replacements[seed % replacements.length];

      document = document
          .apply(
            FlarkV3SourceTransaction.single(
              baseRevision: document.revision,
              operation: FlarkV3SourceEdit(
                startUtf16: start,
                endUtf16: end,
                replacement: replacement,
              ),
            ),
          )
          .document;
      oracle = oracle.replaceRange(start, end, replacement);

      final lineBreakEnds = _logicalLineBreakEnds(oracle);
      expect(
        document.lineCount,
        lineBreakEnds.length + 1,
        reason:
            'iteration=$iteration source=${oracle.replaceAll('\n', r'\n').replaceAll('\r', r'\r')}',
      );
      expect(
        [
          for (var line = 0; line < document.lineCount; line += 1)
            document.lineStartUtf16(line),
        ],
        [0, ...lineBreakEnds],
        reason: 'iteration=$iteration',
      );
      for (var offset = 0; offset <= oracle.length; offset += 1) {
        expect(
          document.lineAtUtf16(offset),
          _logicalLineAt(oracle, offset),
          reason: 'iteration=$iteration offset=$offset',
        );
      }
      if (iteration % 100 == 0 || iteration == 1199) {
        expect(document.toString(), oracle, reason: 'iteration=$iteration');
      }
    }
  });

  test(
    'atomic edits preserve original coordinates and stable insertion order',
    () {
      final document = FlarkV3SourceDocument.fromString('abcd', chunkSize: 2);
      final applied = document.apply(
        FlarkV3SourceTransaction(
          baseRevision: 0,
          operations: const [
            FlarkV3SourceEdit(startUtf16: 1, endUtf16: 1, replacement: 'A'),
            FlarkV3SourceEdit(startUtf16: 1, endUtf16: 1, replacement: 'B'),
            FlarkV3SourceEdit(startUtf16: 2, endUtf16: 3, replacement: 'X'),
          ],
        ),
      );

      expect(applied.document.toString(), 'aABbXd');
      expect(applied.document.revision, 1);
      final batch = applied.parserBatch!;
      expect(batch.baseRevision, 0);
      expect(batch.revision, 1);
      expect(batch.beforeHash32, document.contentHash32);
      expect(batch.afterHash32, applied.document.contentHash32);
      expect(batch.beforeHash128, document.contentHash128);
      expect(batch.afterHash128, applied.document.contentHash128);
      expect(batch.operations.map((edit) => edit.startUtf8), [1, 1, 2]);
      expect(batch.operations.map((edit) => edit.endUtf8), [1, 1, 3]);
      expect(
        batch.operations.map((edit) => utf8.decode(edit.replacementUtf8)),
        ['A', 'B', 'X'],
      );
    },
  );

  test('no-op transactions retain identity and do not advance revision', () {
    final document = FlarkV3SourceDocument.fromString('same');
    final applied = document.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 4,
          replacement: 'same',
        ),
      ),
    );

    expect(applied.changed, isFalse);
    expect(identical(applied.document, document), isTrue);
    expect(applied.document.revision, 0);
    expect(applied.sourceWork.noOpComparedUtf16, 4);
    expect(applied.sourceWork.replacementUtf8BytesEncoded, 0);
  });

  test('stale, overlapping, and invalid scalar edits fail closed', () {
    final document = FlarkV3SourceDocument.fromString('a😀b');
    expect(
      () => document.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: 1,
          operation: const FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: 0,
            replacement: 'x',
          ),
        ),
      ),
      throwsA(isA<FlarkV3RevisionMismatch>()),
    );
    expect(
      () => document.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: 0,
          operation: const FlarkV3SourceEdit(
            startUtf16: 2,
            endUtf16: 2,
            replacement: 'x',
          ),
        ),
      ),
      throwsFormatException,
    );
    expect(
      () => FlarkV3SourceDocument.fromString('abcd').apply(
        FlarkV3SourceTransaction(
          baseRevision: 0,
          operations: const [
            FlarkV3SourceEdit(startUtf16: 0, endUtf16: 2, replacement: ''),
            FlarkV3SourceEdit(startUtf16: 1, endUtf16: 3, replacement: ''),
          ],
        ),
      ),
      throwsStateError,
    );
    expect(
      () => FlarkV3SourceDocument.fromString(String.fromCharCode(0xD800)),
      throwsFormatException,
    );
  });

  test('bounded grapheme lookup is exact or explicitly uncertified', () {
    const line = 'flag 🇨🇦 family 👨‍👩‍👧 decomposed é';
    final document = FlarkV3SourceDocument.fromString('$line\nnext');

    var caret = line.indexOf('🇨🇦') + '🇨🇦'.length;
    var lookup = document.graphemeBefore(caret);
    expect(lookup.isCertified, isTrue);
    expect(document.readRange(lookup.startUtf16!, lookup.endUtf16!), '🇨🇦');

    caret = line.indexOf('👨‍👩‍👧') + '👨‍👩‍👧'.length;
    lookup = document.graphemeBefore(caret);
    expect(
      document.readRange(lookup.startUtf16!, lookup.endUtf16!),
      '👨‍👩‍👧',
    );

    caret = line.length;
    lookup = document.graphemeBefore(caret);
    expect(document.readRange(lookup.startUtf16!, lookup.endUtf16!), 'é');

    final oversizedLine = FlarkV3SourceDocument.fromString(
      '${List.filled(5000, 'a').join()}😀',
    );
    lookup = oversizedLine.graphemeBefore(
      oversizedLine.utf16Length,
      maxContextUtf16: 128,
    );
    expect(lookup.status, FlarkV3GraphemeLookupStatus.needsMoreContext);
    expect(lookup.requiredStartUtf16, 0);
  });

  test('sequential edits stay equivalent to a String oracle', () {
    var oracle = _largeUnicodeText(400000);
    var document = FlarkV3SourceDocument.fromString(oracle);
    var expectedUtf8Length = utf8.encode(oracle).length;
    var seed = 0x51CED15C;

    for (var iteration = 0; iteration < 1500; iteration += 1) {
      seed = _next(seed);
      var start = 8 + seed % (oracle.length - 16);
      while (!_isScalarBoundary(oracle, start)) {
        start += 1;
      }
      seed = _next(seed);
      var end = iteration % 5 == 0 ? start : math.min(start + 1, oracle.length);
      while (!_isScalarBoundary(oracle, end)) {
        end += 1;
      }
      final replacement = switch (iteration % 7) {
        0 => 'x',
        1 => '',
        2 => '*',
        3 => '\n',
        4 => '😀',
        5 => 'β',
        _ => '`',
      };
      final applied = document.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: document.revision,
          operation: FlarkV3SourceEdit(
            startUtf16: start,
            endUtf16: end,
            replacement: replacement,
          ),
        ),
      );
      expectedUtf8Length +=
          utf8.encode(replacement).length -
          utf8.encode(oracle.substring(start, end)).length;
      oracle = oracle.replaceRange(start, end, replacement);
      document = applied.document;

      expect(document.utf16Length, oracle.length);
      expect(document.utf8Length, expectedUtf8Length);
      if (iteration % 100 == 0 || iteration == 1499) {
        expect(document.toString(), oracle, reason: 'iteration=$iteration');
        expect(
          document.contentHash128,
          FlarkV3SourceDocument.fromString(
            oracle,
            chunkSize: 997,
          ).contentHash128,
          reason: 'iteration=$iteration',
        );
      }
    }
  });

  test('large deletion does not leave oversized source leaves', () {
    final source = '${List.filled(1000000, 'a').join()}tail';
    final document = FlarkV3SourceDocument.fromString(source);
    expect(document.diagnostics.largestLeafUtf16, lessThanOrEqualTo(4096));

    final applied = document.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: FlarkV3SourceEdit(
          startUtf16: 1,
          endUtf16: source.length - 4,
          replacement: '',
        ),
      ),
    );

    expect(applied.document.toString(), 'atail');
    expect(applied.document.diagnostics.leafCount, 1);
    expect(applied.document.diagnostics.largestLeafUtf16, 5);
    expect(
      applied.sourceWork.noOpComparedUtf16,
      0,
      reason: 'length mismatch must reject no-op without reading the deletion',
    );
    expect(applied.sourceWork.replacementUtf8BytesEncoded, 0);
  });

  test('large same-length change rejects no-op on the first unequal unit', () {
    final source = List.filled(1000000, 'a').join();
    final replacement = 'b${source.substring(1)}';
    final document = FlarkV3SourceDocument.fromString(source);
    final applied = document.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: source.length,
          replacement: replacement,
        ),
      ),
    );

    expect(applied.sourceWork.noOpComparedUtf16, 1);
    expect(
      applied.sourceWork.replacementUtf8BytesEncoded,
      applied.parserBatch!.operations.single.replacementUtf8.length,
      reason: 'replacement bytes are encoded once and reused by tree + batch',
    );
    expect(applied.document.readRange(0, 1), 'b');
  });

  test(
    '10 MB local edit stays logarithmic and sends a compact parser batch',
    () {
      final source = _largeUnicodeText(10000000);
      final document = FlarkV3SourceDocument.fromString(source);
      var offset = source.length ~/ 2;
      while (!_isScalarBoundary(source, offset)) {
        offset += 1;
      }

      final stopwatch = Stopwatch()..start();
      final applied = document.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: 0,
          operation: FlarkV3SourceEdit(
            startUtf16: offset,
            endUtf16: offset,
            replacement: 'x',
          ),
        ),
      );
      stopwatch.stop();

      expect(applied.document.utf16Length, document.utf16Length + 1);
      expect(applied.parserBatch!.wireBytes, lessThan(96));
      expect(applied.sourceWork.noOpComparedUtf16, 0);
      expect(applied.sourceWork.replacementUtf8BytesEncoded, 1);
      expect(applied.sourceWork.replacementChunksEncoded, 1);
      expect(
        applied.document.diagnostics.largestLeafUtf16,
        lessThanOrEqualTo(4096),
      );
      stdout.writeln(
        'flark_v3_source size=${document.utf16Length} '
        'edit_us=${stopwatch.elapsedMicroseconds} '
        'height=${document.diagnostics.treeHeight} '
        'leaves=${document.diagnostics.leafCount} '
        'wire_bytes=${applied.parserBatch!.wireBytes}',
      );
    },
  );

  test('provisional source edits and undo stay exact before certification', () {
    final bulk = '${List.filled(200000, 'a').join()}tail';
    final session = FlarkV3SourceSession.fromProvisionalString(bulk);

    expect(session.document.isFullyIndexed, isFalse);
    expect(session.document.utf16Length, bulk.length);
    expect(session.document.readRange(bulk.length - 4, bulk.length), 'tail');
    expect(
      () => session.document.fingerprint,
      throwsA(isA<FlarkV3SourceFactsPending>()),
    );

    final edit = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: bulk.length - 1,
          endUtf16: bulk.length,
          replacement: '',
        ),
      ),
    );
    expect(edit.provisional, isTrue);
    expect(edit.parserBatch, isNull);
    expect(edit.sourceWork.replacementUtf8BytesEncoded, 0);
    expect(session.document.readRange(bulk.length - 4, bulk.length - 1), 'tai');
    expect(session.workerSyncDiagnostics.retainedJournalEntries, 1);
    expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);

    final undo = session.undo();
    expect(undo, isNotNull);
    expect(session.document.readRange(bulk.length - 4, bulk.length), 'tail');
    expect(session.uiRevision, 3, reason: 'undo is a new forward revision');
    expect(session.workerSyncDiagnostics.retainedJournalEntries, 2);
    expect(session.canUndo, isFalse);
  });

  test(
    '10 MB provisional adoption retains one backing without UI scanning',
    () {
      final bulk = 'x' * 10000000;
      final initial = FlarkV3SourceSession.fromProvisionalString(bulk);
      expect(initial.document.isFullyIndexed, isFalse);
      expect(initial.document.diagnostics.leafCount, 1);
      expect(initial.document.diagnostics.largestLeafUtf16, bulk.length);
      expect(initial.document.diagnostics.uniqueBackingCount, 1);
      expect(initial.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      expect(initial.workerSyncDiagnostics.retainedJournalEntries, 0);
      expect(initial.workerSyncDiagnostics.snapshotInstallPathNodesVisited, 0);
      expect(initial.workerSyncDiagnostics.snapshotInstallUtf16Copied, 0);
      expect(initial.workerSyncDiagnostics.pageUtf16Copied, 0);
      expect(initial.document.readRange(bulk.length - 4, bulk.length), 'xxxx');

      final replacement = FlarkV3SourceSession.fromString('');
      final applied = replacement.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: replacement.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: 0,
            replacement: bulk,
          ),
        ),
      );
      expect(applied.provisional, isTrue);
      expect(applied.parserBatch, isNull);
      expect(applied.sourceWork.replacementUtf8BytesEncoded, 0);
      expect(applied.sourceWork.replacementChunksEncoded, 0);
      expect(replacement.document.diagnostics.leafCount, 1);
      expect(replacement.document.diagnostics.largestLeafUtf16, bulk.length);
      expect(replacement.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      expect(replacement.workerSyncDiagnostics.retainedJournalEntries, 0);
      expect(
        replacement.workerSyncDiagnostics.snapshotInstallPathNodesVisited,
        0,
      );
      expect(replacement.workerSyncDiagnostics.snapshotInstallUtf16Copied, 0);
      expect(replacement.workerSyncDiagnostics.pageUtf16Copied, 0);
    },
  );

  test('malformed provisional source can be deleted before certification', () {
    final malformed = String.fromCharCodes([0x61, 0x61, 0xD800, 0x62, 0x62]);
    final rejected = FlarkV3SourceSession.fromProvisionalString(malformed);
    _acknowledgeAllWorkerSync(rejected);
    final rejectedRequest = rejected.beginCertification();
    expect(
      () => FlarkV3SourceCertificationReceipt.scan(
        rejectedRequest,
        sourceReplica: rejected.document,
      ),
      throwsA(
        isA<FlarkV3SourceCertificationFailure>().having(
          (failure) => failure.utf16Offset,
          'utf16Offset',
          2,
        ),
      ),
    );
    expect(rejected.document.isFullyIndexed, isFalse);
    expect(rejected.workerRevision, rejected.uiRevision);

    final session = FlarkV3SourceSession.fromProvisionalString(malformed);
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 2,
          endUtf16: 3,
          replacement: '',
        ),
      ),
    );
    expect(session.document.toString(), 'aabb');

    _acknowledgeAllWorkerSync(session);
    final request = session.beginCertification();
    final promotion = session.applyCertification(
      FlarkV3SourceCertificationReceipt.scan(
        request,
        sourceReplica: session.document,
      ),
    );
    expect(promotion.disposition, FlarkV3SourcePromotionDisposition.promoted);
    expect(session.document.isFullyIndexed, isTrue);
    final oracle = FlarkV3SourceDocument.fromString('aabb');
    expect(session.document.utf16Length, oracle.utf16Length);
    expect(session.document.utf8Length, oracle.utf8Length);
    expect(session.document.contentHash128, oracle.contentHash128);
  });

  test('stale certification cannot rewrite a newer exact UI revision', () {
    final session = FlarkV3SourceSession.fromProvisionalString(
      List.filled(50000, 'a').join(),
    );
    _acknowledgeAllWorkerSync(session);
    final staleRequest = session.beginCertification();
    final staleScanner = FlarkV3SourceFactScanner(
      staleRequest,
      sourceReplica: session.acknowledgedSourceReplica(),
    );

    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 1,
          replacement: 'b',
        ),
      ),
    );
    final currentSource = session.document.toString();
    FlarkV3SourceFactCheckpointPage? stalePage;
    while (stalePage == null) {
      stalePage = staleScanner
          .poll(
            const FlarkV3SourceFactScanCredit(
              maximumSourceUtf16: 4096,
              maximumSourceNodes: 8,
              maximumOutputCheckpoints: 4,
              maximumWireBytes: 512,
            ),
          )
          .page;
    }
    expect(
      session.stageCertificationCheckpointPage(stalePage).disposition,
      FlarkV3SourceFactStageDisposition.stale,
    );
    expect(session.document.toString(), currentSource);
    expect(session.document.isFullyIndexed, isFalse);

    _acknowledgeAllWorkerSync(session);
    final currentRequest = session.beginCertification();
    final run = _stageAllSourceFacts(
      session,
      currentRequest,
      checkpointSpacingUtf16: 512,
    );
    expect(
      run.promotion.disposition,
      FlarkV3SourcePromotionDisposition.promoted,
    );
    expect(session.document.toString(), currentSource);
    expect(session.workerRevision, session.uiRevision);
    expect(session.hasPendingWorkerSync, isFalse);
  });

  test('sparse certification promotes without rechunking the bulk backing', () {
    final source = List.filled(20000, 'a\r\n😀b').join();
    final session = FlarkV3SourceSession.fromProvisionalString(source);
    _acknowledgeAllWorkerSync(session);
    final request = session.beginCertification();
    final run = _stageAllSourceFacts(
      session,
      request,
      checkpointSpacingUtf16: 256,
      credit: const FlarkV3SourceFactScanCredit(
        maximumSourceUtf16: 4096,
        maximumSourceNodes: 8,
        maximumOutputCheckpoints: 8,
        maximumWireBytes: 512,
      ),
    );
    expect(run.completion.pieceCount, 1);
    expect(run.completion.checkpointCount, greaterThan(100));
    expect(run.wholeSourceUtf16Copied, 0);

    final promotion = run.promotion;
    expect(promotion.disposition, FlarkV3SourcePromotionDisposition.promoted);
    expect(promotion.piecesAttached, 1);
    expect(
      promotion.pathNodesVisited,
      0,
      reason: 'final publication is one candidate-root pointer swap',
    );
    expect(session.document.diagnostics.leafCount, 1);
    expect(session.document.diagnostics.largestLeafUtf16, source.length);
    expect(session.document.isFullyIndexed, isTrue);
    expect(session.document.toString(), source);
    expect(
      session.document.contentHash128,
      FlarkV3SourceDocument.fromString(source).contentHash128,
    );
    expect(
      session.document.lineStartUtf16(1234),
      FlarkV3SourceDocument.fromString(source).lineStartUtf16(1234),
    );

    final offset = source.length ~/ 2;
    final edited = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: offset,
          endUtf16: offset,
          replacement: 'x',
        ),
      ),
    );
    expect(edited.provisional, isFalse);
    expect(edited.parserBatch, isNotNull);
    final oracle = source.replaceRange(offset, offset, 'x');
    expect(
      session.document.contentHash128,
      FlarkV3SourceDocument.fromString(oracle).contentHash128,
    );
  });

  test(
    'canonical global facts promote atomically without certifying pieces',
    () {
      const source = 'aé🌍\r\nb\n';
      final session = FlarkV3SourceSession.fromProvisionalString(
        source,
        chunkSize: 2,
      );
      _acknowledgeAllWorkerSync(session);
      final lineage = _canonicalLineage(session, requestId: 91);
      final facts = [
        _canonicalFact(source, 4),
        _canonicalFact(source, source.length),
      ];
      final before = session.document;
      final page = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        pageCount: 1,
        checkpointCount: facts.length,
        checkpointSpacingUtf16: 4,
        checkpoints: facts,
      );
      final staged = session.stageCanonicalSourceFactCheckpointPage(page);
      expect(staged.disposition, FlarkV3SourceFactStageDisposition.staged);
      expect(page.isConsumed, isTrue);
      expect(identical(session.document, before), isTrue);
      expect(session.document.hasCertifiedFacts, isFalse);

      final completion = _canonicalCompletion(
        session,
        lineage: lineage,
        source: source,
        facts: facts,
        checkpointSpacingUtf16: 4,
      );
      final promoted = session.commitCanonicalSourceFactCertification(
        completion,
      );
      expect(promoted.disposition, FlarkV3SourcePromotionDisposition.promoted);
      expect(promoted.canonicalProof?.lineage, lineage);
      expect(identical(session.document, before), isFalse);
      expect(session.document.revision, before.revision);
      expect(session.document.hasCertifiedFacts, isTrue);
      expect(
        session.document.isFullyIndexed,
        isFalse,
        reason: 'the global overlay does not fabricate per-piece certification',
      );

      final oracle = FlarkV3SourceDocument.fromString(source);
      final stamp = session.document.sourceStamp as FlarkV3KnownSourceStamp;
      expect(stamp.revision, session.uiRevision);
      expect(stamp.utf16Length, oracle.utf16Length);
      expect(stamp.utf8Length, oracle.utf8Length);
      expect(stamp.contentHash128, oracle.contentHash128);
      expect(session.document.lineCount, oracle.lineCount);
      for (var offset = 0; offset <= source.length; offset += 1) {
        if (!_isScalarBoundary(source, offset)) continue;
        final utf8Offset = utf8.encode(source.substring(0, offset)).length;
        expect(session.document.utf16ToUtf8(offset), utf8Offset);
        expect(session.document.utf8ToUtf16(utf8Offset), offset);
        expect(
          session.document.lineAtUtf16(offset),
          _logicalLineAt(source, offset),
        );
      }
      expect(
        [
          for (var line = 0; line < session.document.lineCount; line += 1)
            session.document.lineStartUtf16(line),
        ],
        [0, 6, 8],
      );

      final applied = session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: source.length,
            endUtf16: source.length,
            replacement: 'x',
          ),
        ),
      );
      expect(applied.provisional, isTrue);
      expect(session.document.hasCertifiedFacts, isFalse);
      expect(session.document.toString(), '${source}x');
    },
  );

  test(
    'canonical staging rejects malformed order, totals, and terminal proof',
    () {
      const source = 'abcdefgh';

      FlarkV3SourceSession freshSession() {
        final session = FlarkV3SourceSession.fromProvisionalString(source);
        _acknowledgeAllWorkerSync(session);
        return session;
      }

      final facts = [_canonicalFact(source, 4), _canonicalFact(source, 8)];

      var session = freshSession();
      var lineage = _canonicalLineage(session, requestId: 101);
      var malformed = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 1,
        pageCount: 1,
        checkpointCount: 2,
        checkpointSpacingUtf16: 4,
        checkpoints: facts,
      );
      expect(
        session.stageCanonicalSourceFactCheckpointPage(malformed).disposition,
        FlarkV3SourceFactStageDisposition.rejected,
      );
      expect(session.document.hasCertifiedFacts, isFalse);

      session = freshSession();
      lineage = _canonicalLineage(session, requestId: 102);
      malformed = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        pageCount: 2,
        checkpointCount: 2,
        checkpointSpacingUtf16: 4,
        checkpoints: facts,
      );
      expect(
        session.stageCanonicalSourceFactCheckpointPage(malformed).disposition,
        FlarkV3SourceFactStageDisposition.rejected,
      );

      session = freshSession();
      lineage = _canonicalLineage(session, requestId: 103);
      malformed = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        pageCount: 1,
        checkpointCount: 2,
        checkpointSpacingUtf16: 2,
        checkpoints: facts,
      );
      expect(
        session.stageCanonicalSourceFactCheckpointPage(malformed).disposition,
        FlarkV3SourceFactStageDisposition.rejected,
      );

      session = freshSession();
      lineage = _canonicalLineage(session, requestId: 104);
      final validPage = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        pageCount: 1,
        checkpointCount: 2,
        checkpointSpacingUtf16: 4,
        checkpoints: facts,
      );
      expect(
        session.stageCanonicalSourceFactCheckpointPage(validPage).disposition,
        FlarkV3SourceFactStageDisposition.staged,
      );
      final validCompletion = _canonicalCompletion(
        session,
        lineage: lineage,
        source: source,
        facts: facts,
        checkpointSpacingUtf16: 4,
      );
      final crossedCompletion = FlarkV3CanonicalSourceFactCompletion(
        lineage: validCompletion.lineage,
        fingerprintAlgorithm: validCompletion.fingerprintAlgorithm,
        fingerprint: validCompletion.fingerprint,
        logicalLineBreaks: validCompletion.logicalLineBreaks,
        checkpointSpacingUtf16: validCompletion.checkpointSpacingUtf16,
        checkpointCount: validCompletion.checkpointCount,
        pageCount: validCompletion.pageCount,
        checkpointHash128: FlarkV3ContentHash128(
          validCompletion.checkpointHash128.word0 ^ 1,
          validCompletion.checkpointHash128.word1,
          validCompletion.checkpointHash128.word2,
          validCompletion.checkpointHash128.word3,
        ),
      );
      expect(
        session
            .commitCanonicalSourceFactCertification(crossedCompletion)
            .disposition,
        FlarkV3SourcePromotionDisposition.rejected,
      );
      expect(session.document.hasCertifiedFacts, isFalse);
      expect(session.hasActiveCertification, isFalse);
    },
  );

  test(
    'canonical completion is stale after an edit and empty roots promote',
    () {
      const source = 'abcdefgh';
      final session = FlarkV3SourceSession.fromProvisionalString(source);
      _acknowledgeAllWorkerSync(session);
      final lineage = _canonicalLineage(session, requestId: 111);
      final facts = [_canonicalFact(source, 4), _canonicalFact(source, 8)];
      final page = FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        pageCount: 1,
        checkpointCount: 2,
        checkpointSpacingUtf16: 4,
        checkpoints: facts,
      );
      expect(
        session.stageCanonicalSourceFactCheckpointPage(page).disposition,
        FlarkV3SourceFactStageDisposition.staged,
      );
      final completion = _canonicalCompletion(
        session,
        lineage: lineage,
        source: source,
        facts: facts,
        checkpointSpacingUtf16: 4,
      );
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: 1,
            replacement: 'z',
          ),
        ),
      );
      expect(
        session.commitCanonicalSourceFactCertification(completion).disposition,
        FlarkV3SourcePromotionDisposition.stale,
      );
      expect(session.document.toString(), 'zbcdefgh');
      expect(session.document.hasCertifiedFacts, isFalse);

      final empty = FlarkV3SourceSession.fromProvisionalString('');
      _acknowledgeAllWorkerSync(empty);
      final emptyLineage = _canonicalLineage(empty, requestId: 112);
      final emptyCompletion = FlarkV3CanonicalSourceFactCompletion(
        lineage: emptyLineage,
        fingerprintAlgorithm: 1,
        fingerprint: const FlarkV3SourceFingerprint(
          revision: 0,
          utf16Length: 0,
          utf8Length: 0,
          contentHash128: FlarkV3ContentHash128.zero,
        ),
        logicalLineBreaks: 0,
        checkpointSpacingUtf16: 4,
        checkpointCount: 0,
        pageCount: 0,
        checkpointHash128: FlarkV3ContentHash128.zero,
      );
      final promoted = empty.commitCanonicalSourceFactCertification(
        emptyCompletion,
      );
      expect(promoted.disposition, FlarkV3SourcePromotionDisposition.promoted);
      expect(promoted.canonicalProof?.pageCount, 0);
      expect(
        empty.document.sourceStamp,
        const FlarkV3KnownSourceStamp(
          revision: 0,
          utf16Length: 0,
          utf8Length: 0,
          contentHash128: FlarkV3ContentHash128.zero,
        ),
      );
    },
  );

  test('certification alone cannot mint a structural delta base', () {
    const source = 'abcdefgh';
    final session = FlarkV3SourceSession.fromProvisionalString(source);
    _acknowledgeAllWorkerSync(session);
    _installCanonicalGlobalFacts(
      session,
      source: source,
      requestId: 120,
      checkpointSpacingUtf16: 4,
    );
    expect(session.retainedCanonicalSourceFactDeltaBase, isNull);

    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 4,
          endUtf16: 5,
          replacement: 'Z',
        ),
      ),
    );
    expect(session.retainedCanonicalSourceFactDeltaBase, isNull);
  });

  test(
    'canonical delta transfers one changed page and structurally reuses both sides',
    () {
      final baseSource = List.filled(600, 'a').join();
      final session = FlarkV3SourceSession.fromProvisionalString(baseSource);
      _acknowledgeAllWorkerSync(session);
      _installCanonicalGlobalFacts(
        session,
        source: baseSource,
        requestId: 121,
        checkpointSpacingUtf16: 4,
      );
      expect(session.retainedCanonicalSourceFactDeltaBase, isNull);
      session.commitInstalledCanonicalSourceFactStructuralBase();
      final baseAuthority = session.installedCanonicalSourceFactAuthority!;
      expect(baseAuthority.pageCount, 3);
      expect(baseAuthority.checkpointCount, 150);

      const editOffset = 300;
      final targetSource =
          '${baseSource.substring(0, editOffset)}b'
          '${baseSource.substring(editOffset + 1)}';
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: editOffset,
            endUtf16: editOffset + 1,
            replacement: 'b',
          ),
        ),
      );
      _acknowledgeAllWorkerSync(session);
      expect(
        identical(session.retainedCanonicalSourceFactDeltaBase, baseAuthority),
        isTrue,
      );

      final lineage = _canonicalLineage(session, requestId: 122);
      final targetFacts = _canonicalFacts(targetSource, 4);
      final replacementFacts = targetFacts.sublist(64, 128);
      final delta = FlarkV3CanonicalSourceFactDelta(
        lineage: lineage,
        baseAuthority: baseAuthority,
        baseFingerprint: baseAuthority.fingerprint,
        baseCheckpointRootGuard128: baseAuthority.checkpointHash128,
        baseCheckpointCount: baseAuthority.checkpointCount,
        basePageCount: baseAuthority.pageCount,
        baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
        basePageStart: 1,
        basePageEnd: 2,
        targetPageStart: 1,
        targetPageEnd: 2,
        targetCheckpointCount: targetFacts.length,
        targetPageCount: 3,
        targetCheckpointRootGuardAlgorithm:
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
        targetCheckpointRootGuard128: _portableFactsHash(targetFacts),
        replacementCheckpointCount: replacementFacts.length,
      );
      final opened = session.beginCanonicalSourceFactDelta(delta);
      expect(
        opened.disposition,
        FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted,
      );
      expect(opened.reusedPageCount, 2);
      expect(opened.reusedCheckpointCount, 86);
      expect(opened.checkpointFactsCopied, 0);

      final page = FlarkV3CanonicalSourceFactDeltaCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        checkpoints: replacementFacts,
      );
      final staged = session.stageCanonicalSourceFactDeltaCheckpointPage(page);
      expect(staged.disposition, FlarkV3SourceFactStageDisposition.staged);
      expect(page.isConsumed, isTrue);
      final promoted = session.commitCanonicalSourceFactDeltaCertification(
        _canonicalDeltaCompletion(
          session,
          lineage: lineage,
          source: targetSource,
          facts: targetFacts,
          replacementFacts: replacementFacts,
          checkpointSpacingUtf16: 4,
        ),
      );
      expect(promoted.disposition, FlarkV3SourcePromotionDisposition.promoted);
      expect(promoted.reusedPageCount, 2);
      expect(promoted.reusedCheckpointCount, 86);
      expect(promoted.transferredPageCount, 1);
      expect(promoted.transferredCheckpointCount, 64);
      expect(promoted.checkpointFactsCopied, 0);
      expect(promoted.pathNodesAllocated, lessThan(32));
      expect(
        identical(session.retainedCanonicalSourceFactDeltaBase, baseAuthority),
        isTrue,
      );

      final oracle = FlarkV3SourceDocument.fromString(targetSource);
      expect(session.document.contentHash128, oracle.contentHash128);
      for (var offset = 0; offset <= targetSource.length; offset += 1) {
        expect(session.document.utf16ToUtf8(offset), offset);
        expect(session.document.utf8ToUtf16(offset), offset);
      }

      final uncommittedFirstTarget =
          session.installedCanonicalSourceFactAuthority!;
      expect(identical(uncommittedFirstTarget, baseAuthority), isFalse);
      const secondEditOffset = 304;
      final secondTarget =
          '${targetSource.substring(0, secondEditOffset)}c'
          '${targetSource.substring(secondEditOffset + 1)}';
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: secondEditOffset,
            endUtf16: secondEditOffset + 1,
            replacement: 'c',
          ),
        ),
      );
      _acknowledgeAllWorkerSync(session);
      final secondLineage = _canonicalLineage(session, requestId: 123);
      final secondFacts = _canonicalFacts(secondTarget, 4);
      final secondReplacement = secondFacts.sublist(64, 128);
      final secondDelta = FlarkV3CanonicalSourceFactDelta(
        lineage: secondLineage,
        baseAuthority: baseAuthority,
        baseFingerprint: baseAuthority.fingerprint,
        baseCheckpointRootGuard128: baseAuthority.checkpointHash128,
        baseCheckpointCount: baseAuthority.checkpointCount,
        basePageCount: baseAuthority.pageCount,
        baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
        basePageStart: 1,
        basePageEnd: 2,
        targetPageStart: 1,
        targetPageEnd: 2,
        targetCheckpointCount: secondFacts.length,
        targetPageCount: 3,
        targetCheckpointRootGuardAlgorithm:
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
        targetCheckpointRootGuard128: _portableFactsHash(secondFacts),
        replacementCheckpointCount: secondReplacement.length,
      );
      expect(
        session.beginCanonicalSourceFactDelta(secondDelta).disposition,
        FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted,
      );
      expect(
        session
            .stageCanonicalSourceFactDeltaCheckpointPage(
              FlarkV3CanonicalSourceFactDeltaCheckpointPage(
                lineage: secondLineage,
                pageOrdinal: 0,
                checkpoints: secondReplacement,
              ),
            )
            .disposition,
        FlarkV3SourceFactStageDisposition.staged,
      );
      final secondPromotion = session
          .commitCanonicalSourceFactDeltaCertification(
            _canonicalDeltaCompletion(
              session,
              lineage: secondLineage,
              source: secondTarget,
              facts: secondFacts,
              replacementFacts: secondReplacement,
              checkpointSpacingUtf16: 4,
            ),
          );
      expect(
        secondPromotion.disposition,
        FlarkV3SourcePromotionDisposition.promoted,
      );
      expect(secondPromotion.reusedPageCount, 2);
      expect(secondPromotion.reusedCheckpointCount, 86);
      expect(secondPromotion.transferredCheckpointCount, 64);
      expect(
        session.document.contentHash128,
        FlarkV3SourceDocument.fromString(secondTarget).contentHash128,
      );
      final committedSecondTarget =
          session.installedCanonicalSourceFactAuthority!;
      expect(
        identical(session.retainedCanonicalSourceFactDeltaBase, baseAuthority),
        isTrue,
      );
      session.commitInstalledCanonicalSourceFactStructuralBase();
      expect(
        identical(
          session.retainedCanonicalSourceFactDeltaBase,
          committedSecondTarget,
        ),
        isTrue,
      );
    },
  );

  test(
    'canonical delta authenticates its base and fails closed on crossing',
    () {
      final baseSource = List.filled(600, 'a').join();
      final session = FlarkV3SourceSession.fromProvisionalString(baseSource);
      _acknowledgeAllWorkerSync(session);
      _installCanonicalGlobalFacts(
        session,
        source: baseSource,
        requestId: 131,
        checkpointSpacingUtf16: 4,
      );
      session.commitInstalledCanonicalSourceFactStructuralBase();
      final baseAuthority = session.installedCanonicalSourceFactAuthority!;
      final targetSource =
          '${baseSource.substring(0, 300)}b'
          '${baseSource.substring(301)}';
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 300,
            endUtf16: 301,
            replacement: 'b',
          ),
        ),
      );
      _acknowledgeAllWorkerSync(session);
      final targetFacts = _canonicalFacts(targetSource, 4);
      final replacementFacts = targetFacts.sublist(64, 128);
      final lineage = _canonicalLineage(session, requestId: 132);

      FlarkV3CanonicalSourceFactDelta header({
        required FlarkV3ContentHash128 baseHash,
      }) => FlarkV3CanonicalSourceFactDelta(
        lineage: lineage,
        baseAuthority: baseAuthority,
        baseFingerprint: baseAuthority.fingerprint,
        baseCheckpointRootGuard128: baseHash,
        baseCheckpointCount: baseAuthority.checkpointCount,
        basePageCount: baseAuthority.pageCount,
        baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
        basePageStart: 1,
        basePageEnd: 2,
        targetPageStart: 1,
        targetPageEnd: 2,
        targetCheckpointCount: targetFacts.length,
        targetPageCount: 3,
        targetCheckpointRootGuardAlgorithm:
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
        targetCheckpointRootGuard128: _portableFactsHash(targetFacts),
        replacementCheckpointCount: replacementFacts.length,
      );

      expect(
        session
            .beginCanonicalSourceFactDelta(
              header(
                baseHash: FlarkV3ContentHash128(
                  baseAuthority.checkpointHash128.word0 ^ 1,
                  baseAuthority.checkpointHash128.word1,
                  baseAuthority.checkpointHash128.word2,
                  baseAuthority.checkpointHash128.word3,
                ),
              ),
            )
            .disposition,
        FlarkV3CanonicalSourceFactDeltaBeginDisposition.stale,
      );
      expect(
        session
            .beginCanonicalSourceFactDelta(
              header(baseHash: baseAuthority.checkpointHash128),
            )
            .disposition,
        FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted,
      );
      session.stageCanonicalSourceFactDeltaCheckpointPage(
        FlarkV3CanonicalSourceFactDeltaCheckpointPage(
          lineage: lineage,
          pageOrdinal: 0,
          checkpoints: replacementFacts,
        ),
      );
      final valid = _canonicalDeltaCompletion(
        session,
        lineage: lineage,
        source: targetSource,
        facts: targetFacts,
        replacementFacts: replacementFacts,
        checkpointSpacingUtf16: 4,
      );
      final crossed = FlarkV3CanonicalSourceFactDeltaCompletion(
        lineage: valid.lineage,
        fingerprintAlgorithm: valid.fingerprintAlgorithm,
        fingerprint: valid.fingerprint,
        logicalLineBreaks: valid.logicalLineBreaks,
        checkpointSpacingUtf16: valid.checkpointSpacingUtf16,
        checkpointCount: valid.checkpointCount,
        pageCount: valid.pageCount,
        checkpointRootGuardAlgorithm: valid.checkpointRootGuardAlgorithm,
        checkpointRootGuard128: FlarkV3ContentHash128(
          valid.checkpointRootGuard128.word0 ^ 1,
          valid.checkpointRootGuard128.word1,
          valid.checkpointRootGuard128.word2,
          valid.checkpointRootGuard128.word3,
        ),
        replacementCheckpointHash128: valid.replacementCheckpointHash128,
      );
      expect(
        session
            .commitCanonicalSourceFactDeltaCertification(crossed)
            .disposition,
        FlarkV3SourcePromotionDisposition.rejected,
      );
      expect(session.document.hasCertifiedFacts, isFalse);
      expect(session.hasActiveCertification, isFalse);
      expect(
        identical(session.retainedCanonicalSourceFactDeltaBase, baseAuthority),
        isTrue,
      );
    },
  );

  test('inverse history keeps slices, not snapshots, and evicts by bytes', () {
    final session = FlarkV3SourceSession.fromString(
      List.filled(1000, 'a').join(),
      historyEntryLimit: 8,
      historyByteLimit: 64,
    );
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 50,
          endUtf16: 950,
          replacement: '',
        ),
      ),
    );
    expect(session.undoEntryCount, 1);
    expect(session.undoRetainedUtf16Bytes, greaterThan(64));

    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'b',
        ),
      ),
    );
    expect(session.undoEntryCount, 1);
    expect(session.undoRetainedUtf16Bytes, lessThanOrEqualTo(64));
  });

  test('large chunked delete leases one subtree with logarithmic setup', () {
    final source = List.filled(300000, 'abcd\n').join();
    final session = FlarkV3SourceSession.fromString(
      source,
      chunkSize: 32,
      historyByteLimit: source.length * 3,
    );
    final before = session.document.fingerprint;
    final diagnostics = session.document.diagnostics;
    expect(diagnostics.leafCount, greaterThan(10000));

    final applied = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: 127,
          endUtf16: source.length - 129,
          replacement: '',
        ),
      ),
    );
    expect(
      applied.inverseLeasePathNodesVisited,
      lessThanOrEqualTo(diagnostics.treeHeight * 2 + 4),
      reason: 'deleted-range setup visits split paths, not 46k leaves',
    );
    expect(
      applied.inverseLeasePathNodesVisited,
      lessThan(diagnostics.leafCount ~/ 100),
    );
    expect(
      session.document.toString(),
      '${source.substring(0, 127)}${source.substring(source.length - 129)}',
    );

    session.undo();
    expect(session.document.utf16Length, before.utf16Length);
    expect(session.document.utf8Length, before.utf8Length);
    expect(session.document.contentHash128, before.contentHash128);
  });

  test('pending discovery prunes a large certified tree to one path', () {
    final source = List.filled(200000, 'certified\n').join();
    final session = FlarkV3SourceSession.fromString(source, chunkSize: 64);
    final midpoint = source.length ~/ 2;
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: midpoint,
          endUtf16: midpoint,
          replacement: List.filled(9000, 'p').join(),
        ),
      ),
    );

    _acknowledgeAllWorkerSync(session);
    final request = session.beginCertification();
    expect(request.firstPiecePage.pieces, hasLength(1));
    expect(request.firstPiecePage.hasMore, isFalse);
    expect(
      request.firstPiecePage.nodesVisited,
      lessThanOrEqualTo(session.document.diagnostics.treeHeight + 1),
      reason: 'certified siblings are pruned without visiting their leaves',
    );
    expect(
      () => session.beginCertification(maximumDiscoveryNodes: 1),
      throwsRangeError,
      reason: 'a caller cannot choose fuel that livelocks before one path',
    );
  });

  test('thousands of source pieces stage through one atomic candidate', () {
    final session = FlarkV3SourceSession.fromProvisionalString(
      'aa',
      chunkSize: 2,
    );
    for (var index = 0; index < 1000; index += 1) {
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: session.document.utf16Length,
            endUtf16: session.document.utf16Length,
            replacement: 'bb',
          ),
        ),
      );
    }
    _acknowledgeAllWorkerSync(session);
    expect(
      () => session.beginCertification(maximumPieceDescriptors: 65),
      throwsRangeError,
      reason: 'descriptor pages are capped before any oversized copy',
    );
    final request = session.beginCertification(maximumPieceDescriptors: 8);
    expect(request.firstPiecePage.pieces.length, lessThanOrEqualTo(8));
    expect(request.firstPiecePage.hasMore, isTrue);
    expect(
      () => FlarkV3SourceCertificationReceipt.scan(
        request,
        sourceReplica: session.document,
      ),
      throwsStateError,
      reason: 'the local scan helper cannot hide staged source-fact adoption',
    );
    final workerBefore = session.workerRevision;
    final run = _stageAllSourceFacts(
      session,
      request,
      checkpointSpacingUtf16: 2,
      credit: const FlarkV3SourceFactScanCredit(
        maximumSourceUtf16: 1,
        maximumSourceNodes: 1,
        maximumOutputCheckpoints: 1,
        maximumWireBytes: 128,
      ),
    );
    expect(run.completion.pieceCount, 1001);
    expect(run.completion.checkpointCount, 1001);
    expect(run.maximumCheckpointsPerPoll, 1);
    expect(run.maximumSourceNodesPerPoll, 1);
    expect(run.wholeSourceUtf16Copied, 0);
    expect(run.promotion.pathNodesVisited, 0);
    expect(session.workerRevision, workerBefore);
    expect(session.certificationDiagnostics.candidateRootCount, 0);
    expect(session.document.toString(), 'aa${'bb' * 1000}');
    expect(
      session.document.contentHash128,
      FlarkV3SourceDocument.fromString('aa${'bb' * 1000}').contentHash128,
    );
  });

  test('fuel one stays sparse and never splits Unicode or CRLF pages', () {
    final source = 'a😀\r\nb' * 2000;
    final session = FlarkV3SourceSession.fromProvisionalString(source);
    _acknowledgeAllWorkerSync(session);
    final request = session.beginCertification();
    final scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 64,
    );
    const credit = FlarkV3SourceFactScanCredit(
      maximumSourceUtf16: 1,
      maximumSourceNodes: 1,
      maximumOutputCheckpoints: 1,
      maximumWireBytes: 128,
    );
    FlarkV3SourceFactCompletion? completion;
    var polls = 0;
    var pages = 0;
    var copied = 0;
    while (completion == null) {
      final poll = scanner.poll(credit);
      polls += 1;
      copied += poll.work.wholeSourceUtf16Copied;
      expect(poll.work.utf8ScratchCollectionsAllocated, 0);
      expect(poll.work.sourceNodesVisited, lessThanOrEqualTo(1));
      expect(poll.work.sourceUtf16Examined, lessThanOrEqualTo(2));
      expect(poll.work.checkpointsEmitted, lessThanOrEqualTo(1));
      expect(poll.work.wireBytesEmitted, lessThanOrEqualTo(128));
      if (poll.page case final page?) {
        pages += 1;
        final end = page.piece.globalStartUtf16 + page.relativeEndUtf16;
        expect(_isScalarBoundary(source, end), isTrue);
        if (end > 0 && end < source.length) {
          expect(
            source.codeUnitAt(end - 1) == 0x0D &&
                source.codeUnitAt(end) == 0x0A,
            isFalse,
            reason: 'checkpoint page cannot split CRLF',
          );
        }
        expect(
          session.stageCertificationCheckpointPage(page).disposition,
          FlarkV3SourceFactStageDisposition.staged,
        );
      }
      completion = poll.completion;
    }
    expect(polls, greaterThan(source.length ~/ 2));
    expect(copied, 0);
    expect(completion.pageCount, pages);
    expect(
      completion.checkpointCount,
      lessThanOrEqualTo((source.length / 64).ceil() + 1),
      reason: 'fuel schedule must not determine checkpoint density',
    );
    expect(
      session.commitSourceFactCertification(completion).disposition,
      FlarkV3SourcePromotionDisposition.promoted,
    );
    final oracle = FlarkV3SourceDocument.fromString(source, chunkSize: 3);
    expect(session.document.utf16Length, oracle.utf16Length);
    expect(session.document.utf8Length, oracle.utf8Length);
    expect(session.document.contentHash128, oracle.contentHash128);
    expect(session.document.lineCount, oracle.lineCount);
    for (final line in [0, 1, 999, oracle.lineCount - 1]) {
      expect(
        session.document.lineStartUtf16(line),
        oracle.lineStartUtf16(line),
      );
    }
  });

  test('CRLF split across provisional pieces remains one logical break', () {
    final session = FlarkV3SourceSession.fromProvisionalString(
      'a\r',
      chunkSize: 2,
    );
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 2,
          endUtf16: 2,
          replacement: '\nb',
        ),
      ),
    );
    _acknowledgeAllWorkerSync(session);
    final run = _stageAllSourceFacts(
      session,
      session.beginCertification(),
      checkpointSpacingUtf16: 2,
      credit: const FlarkV3SourceFactScanCredit(
        maximumSourceUtf16: 1,
        maximumSourceNodes: 1,
        maximumOutputCheckpoints: 1,
        maximumWireBytes: 128,
      ),
    );
    expect(run.completion.pieceCount, 2);
    expect(run.completion.logicalLineBreaks, 1);
    expect(session.document.lineCount, 2);
    expect(session.document.lineStartUtf16(1), 3);
    expect(session.document.toString(), 'a\r\nb');
  });

  test('late malformed scalar failure drops every staged candidate fact', () {
    final malformed = String.fromCharCodes([
      ...List.filled(130, 0x61),
      0xD800,
      0x62,
    ]);
    final session = FlarkV3SourceSession.fromProvisionalString(malformed);
    _acknowledgeAllWorkerSync(session);
    final request = session.beginCertification();
    final scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 32,
    );
    const credit = FlarkV3SourceFactScanCredit(
      maximumSourceUtf16: 1,
      maximumSourceNodes: 1,
      maximumOutputCheckpoints: 1,
      maximumWireBytes: 128,
    );
    FlarkV3SourceFactCheckpointPage? latePage;
    FlarkV3SourceCertificationFailure? failure;
    while (failure == null) {
      try {
        final poll = scanner.poll(credit);
        if (poll.page case final page?) {
          latePage ??= FlarkV3SourceFactCheckpointPage(
            lineage: page.lineage,
            piece: page.piece,
            pageOrdinal: page.pageOrdinal,
            piecePageOrdinal: page.piecePageOrdinal,
            relativeStartUtf16: page.relativeStartUtf16,
            relativeEndUtf16: page.relativeEndUtf16,
            checkpointSpacingUtf16: page.checkpointSpacingUtf16,
            isLast: page.isLast,
            checkpoints: page.checkpoints,
          );
          expect(
            session.stageCertificationCheckpointPage(page).disposition,
            FlarkV3SourceFactStageDisposition.staged,
          );
        }
      } on FlarkV3SourceCertificationFailure catch (caught) {
        failure = caught;
      }
    }
    expect(failure.utf16Offset, 130);
    expect(failure.lineage, request.lineage);
    expect(session.document.isFullyIndexed, isFalse);
    expect(
      session.certificationDiagnostics.checkpointsAccepted,
      greaterThan(0),
    );
    final rejected = session.rejectSourceFactCertification(failure);
    expect(rejected.cancelled, isTrue);
    expect(rejected.candidateRootsReleased, 1);
    expect(rejected.pathNodesVisited, 0);
    expect(session.certificationDiagnostics.candidateRootCount, 0);
    expect(
      session.stageCertificationCheckpointPage(latePage!).disposition,
      FlarkV3SourceFactStageDisposition.stale,
    );
    expect(session.document.toString(), malformed);
    expect(session.document.isFullyIndexed, isFalse);
  });

  test('cancel, no-op, edit, and late pages obey the candidate barrier', () {
    final session = FlarkV3SourceSession.fromProvisionalString('a' * 1000);
    _acknowledgeAllWorkerSync(session);
    var request = session.beginCertification();
    var scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 64,
    );
    const credit = FlarkV3SourceFactScanCredit(
      maximumSourceUtf16: 64,
      maximumSourceNodes: 8,
      maximumOutputCheckpoints: 1,
      maximumWireBytes: 128,
    );
    final firstPage = scanner.poll(credit).page!;
    final lateAfterCancel = FlarkV3SourceFactCheckpointPage(
      lineage: firstPage.lineage,
      piece: firstPage.piece,
      pageOrdinal: firstPage.pageOrdinal,
      piecePageOrdinal: firstPage.piecePageOrdinal,
      relativeStartUtf16: firstPage.relativeStartUtf16,
      relativeEndUtf16: firstPage.relativeEndUtf16,
      checkpointSpacingUtf16: firstPage.checkpointSpacingUtf16,
      isLast: firstPage.isLast,
      checkpoints: firstPage.checkpoints,
    );
    expect(
      session.stageCertificationCheckpointPage(firstPage).disposition,
      FlarkV3SourceFactStageDisposition.staged,
    );
    final noOp = session.apply(
      FlarkV3SourceTransaction(
        baseRevision: session.uiRevision,
        operations: const [],
      ),
    );
    expect(noOp.changed, isFalse);
    expect(session.certificationDiagnostics.candidateRootCount, 1);
    final cancelled = session.cancelSourceFactCertification(request.requestId);
    expect(cancelled.cancelled, isTrue);
    expect(cancelled.candidateRootsReleased, 1);
    expect(cancelled.pathNodesVisited, 0);
    expect(scanner.cancel(), isTrue);
    expect(scanner.poll(credit).isCancelled, isTrue);
    expect(
      session.stageCertificationCheckpointPage(lateAfterCancel).disposition,
      FlarkV3SourceFactStageDisposition.stale,
    );

    request = session.beginCertification();
    scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 64,
    );
    final beforeEdit = scanner.poll(credit).page!;
    expect(
      session.stageCertificationCheckpointPage(beforeEdit).disposition,
      FlarkV3SourceFactStageDisposition.staged,
    );
    final lateAfterEdit = scanner.poll(credit).page!;
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: session.document.utf16Length,
          endUtf16: session.document.utf16Length,
          replacement: 'b',
        ),
      ),
    );
    expect(session.certificationDiagnostics.candidateRootCount, 0);
    expect(
      session.stageCertificationCheckpointPage(lateAfterEdit).disposition,
      FlarkV3SourceFactStageDisposition.stale,
    );
    expect(session.document.isFullyIndexed, isFalse);
  });

  test('foreign, duplicate, and corrupted fact pages fail closed', () {
    final session = FlarkV3SourceSession.fromProvisionalString('a' * 300);
    _acknowledgeAllWorkerSync(session);
    var request = session.beginCertification();
    var scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 64,
    );
    const credit = FlarkV3SourceFactScanCredit(
      maximumSourceUtf16: 64,
      maximumSourceNodes: 8,
      maximumOutputCheckpoints: 1,
      maximumWireBytes: 128,
    );
    final first = scanner.poll(credit).page!;
    final foreignLineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: request.sourceSessionIdentity,
      requestId: request.requestId + 99,
      workerGeneration: request.workerGeneration,
      workerReplicaRevision: request.workerReplicaRevision,
      uiRevision: request.uiRevision,
      utf16Length: request.utf16Length,
      intentHighWater: request.intentHighWater,
    );
    final foreign = FlarkV3SourceFactCheckpointPage(
      lineage: foreignLineage,
      piece: first.piece,
      pageOrdinal: first.pageOrdinal,
      piecePageOrdinal: first.piecePageOrdinal,
      relativeStartUtf16: first.relativeStartUtf16,
      relativeEndUtf16: first.relativeEndUtf16,
      checkpointSpacingUtf16: first.checkpointSpacingUtf16,
      isLast: first.isLast,
      checkpoints: first.checkpoints,
    );
    expect(
      session.stageCertificationCheckpointPage(foreign).disposition,
      FlarkV3SourceFactStageDisposition.stale,
    );
    expect(session.certificationDiagnostics.candidateRootCount, 1);

    final duplicate = FlarkV3SourceFactCheckpointPage(
      lineage: first.lineage,
      piece: first.piece,
      pageOrdinal: first.pageOrdinal,
      piecePageOrdinal: first.piecePageOrdinal,
      relativeStartUtf16: first.relativeStartUtf16,
      relativeEndUtf16: first.relativeEndUtf16,
      checkpointSpacingUtf16: first.checkpointSpacingUtf16,
      isLast: first.isLast,
      checkpoints: first.checkpoints,
    );
    expect(
      session.stageCertificationCheckpointPage(first).disposition,
      FlarkV3SourceFactStageDisposition.staged,
    );
    expect(
      session.stageCertificationCheckpointPage(duplicate).disposition,
      FlarkV3SourceFactStageDisposition.rejected,
    );
    expect(session.certificationDiagnostics.candidateRootCount, 0);
    expect(session.document.isFullyIndexed, isFalse);

    request = session.beginCertification();
    scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 64,
    );
    final original = scanner.poll(credit).page!;
    final originalFacts = original.checkpoints;
    final firstFact = originalFacts.first;
    final corruptFacts = <FlarkV3SourcePrefixFacts>[
      FlarkV3SourcePrefixFacts(
        utf16Offset: firstFact.utf16Offset,
        utf8Offset: firstFact.utf8Offset,
        newlines: firstFact.newlines,
        hash: FlarkV3ContentHash128(
          firstFact.hash.word0 ^ 1,
          firstFact.hash.word1,
          firstFact.hash.word2,
          firstFact.hash.word3,
        ),
      ),
      ...originalFacts.skip(1),
    ];
    final corrupt = FlarkV3SourceFactCheckpointPage(
      lineage: original.lineage,
      piece: original.piece,
      pageOrdinal: original.pageOrdinal,
      piecePageOrdinal: original.piecePageOrdinal,
      relativeStartUtf16: original.relativeStartUtf16,
      relativeEndUtf16: original.relativeEndUtf16,
      checkpointSpacingUtf16: original.checkpointSpacingUtf16,
      isLast: original.isLast,
      checkpoints: corruptFacts,
    );
    expect(
      session.stageCertificationCheckpointPage(corrupt).disposition,
      FlarkV3SourceFactStageDisposition.staged,
      reason: 'transport digest is checked against final completion',
    );
    FlarkV3SourceFactCompletion? completion;
    while (completion == null) {
      final poll = scanner.poll(credit);
      if (poll.page case final page?) {
        expect(
          session.stageCertificationCheckpointPage(page).disposition,
          FlarkV3SourceFactStageDisposition.staged,
        );
      }
      completion = poll.completion;
    }
    expect(
      session.commitSourceFactCertification(completion).disposition,
      FlarkV3SourcePromotionDisposition.rejected,
      reason: 'corrupted retained checkpoints cannot be published',
    );
    expect(session.document.isFullyIndexed, isFalse);
    expect(session.certificationDiagnostics.candidateRootCount, 0);

    final recovered = _stageAllSourceFacts(
      session,
      session.beginCertification(),
      checkpointSpacingUtf16: 64,
    );
    expect(
      recovered.promotion.disposition,
      FlarkV3SourcePromotionDisposition.promoted,
    );
  });

  test('aggregate mismatch rejects a complete hidden root in O(1)', () {
    final session = FlarkV3SourceSession.fromProvisionalString(
      'alpha 😀\r\nbeta',
    );
    _acknowledgeAllWorkerSync(session);
    final completion = _stageSourceFactsWithoutCommit(
      session,
      session.beginCertification(),
      checkpointSpacingUtf16: 4,
    );
    expect(session.certificationDiagnostics.candidateRootCount, 1);
    expect(session.document.isFullyIndexed, isFalse);
    final bad = FlarkV3SourceFactCompletion(
      lineage: completion.lineage,
      fingerprint: FlarkV3SourceFingerprint(
        revision: completion.fingerprint.revision,
        utf16Length: completion.fingerprint.utf16Length,
        utf8Length: completion.fingerprint.utf8Length + 1,
        contentHash128: completion.fingerprint.contentHash128,
      ),
      logicalLineBreaks: completion.logicalLineBreaks,
      pieceCount: completion.pieceCount,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      descriptorHash128: completion.descriptorHash128,
      checkpointHash128: completion.checkpointHash128,
    );
    final rejected = session.commitSourceFactCertification(bad);
    expect(rejected.disposition, FlarkV3SourcePromotionDisposition.rejected);
    expect(rejected.pathNodesVisited, 0);
    expect(rejected.piecesAttached, 0);
    expect(session.certificationDiagnostics.candidateRootCount, 0);
    expect(session.document.isFullyIndexed, isFalse);
  });

  test('source-fact callers cannot select unbounded poll or path work', () {
    final session = FlarkV3SourceSession.fromProvisionalString('pending');
    _acknowledgeAllWorkerSync(session);
    expect(
      () => session.beginCertification(maximumDiscoveryNodes: 4097),
      throwsRangeError,
    );
    final request = session.beginCertification();
    final scanner = FlarkV3SourceFactScanner(
      request,
      sourceReplica: session.acknowledgedSourceReplica(),
      checkpointSpacingUtf16: 2,
    );
    expect(
      () => scanner.poll(
        const FlarkV3SourceFactScanCredit(
          maximumSourceUtf16: 8193,
          maximumSourceNodes: 1,
          maximumOutputCheckpoints: 1,
          maximumWireBytes: 128,
        ),
      ),
      throwsRangeError,
    );
    expect(
      () => scanner.poll(
        const FlarkV3SourceFactScanCredit(
          maximumSourceUtf16: 1,
          maximumSourceNodes: 1025,
          maximumOutputCheckpoints: 1,
          maximumWireBytes: 128,
        ),
      ),
      throwsRangeError,
    );
    const normal = FlarkV3SourceFactScanCredit(
      maximumSourceUtf16: 2,
      maximumSourceNodes: 8,
      maximumOutputCheckpoints: 1,
      maximumWireBytes: 128,
    );
    final page = scanner.poll(normal).page!;
    expect(
      () => session.stageCertificationCheckpointPage(
        page,
        maximumPathNodes: 4097,
      ),
      throwsRangeError,
    );
    expect(page.isConsumed, isFalse);
    expect(session.certificationDiagnostics.candidateRootCount, 1);
    expect(
      session.cancelSourceFactCertification(request.requestId).cancelled,
      isTrue,
    );
  });

  test(
    'restart invalidates facts and certification never grants worker credit',
    () {
      final session = FlarkV3SourceSession.fromProvisionalString('pending');
      _acknowledgeAllWorkerSync(session);
      final request = session.beginCertification();
      final scanner = FlarkV3SourceFactScanner(
        request,
        sourceReplica: session.acknowledgedSourceReplica(),
        checkpointSpacingUtf16: 2,
      );
      const credit = FlarkV3SourceFactScanCredit(
        maximumSourceUtf16: 2,
        maximumSourceNodes: 8,
        maximumOutputCheckpoints: 1,
        maximumWireBytes: 128,
      );
      final first = scanner.poll(credit).page!;
      expect(
        session.stageCertificationCheckpointPage(first).disposition,
        FlarkV3SourceFactStageDisposition.staged,
      );
      final late = scanner.poll(credit).page!;
      session.restartWorker();

      expect(
        session.stageCertificationCheckpointPage(late).disposition,
        FlarkV3SourceFactStageDisposition.stale,
      );
      expect(session.document.isFullyIndexed, isFalse);
      expect(session.workerRevision, 0);
      expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      expect(session.hasPendingWorkerSync, isTrue);

      _acknowledgeAllWorkerSync(session);
      final workerBefore = session.workerRevision;
      final current = session.beginCertification();
      final run = _stageAllSourceFacts(session, current);
      expect(
        run.promotion.disposition,
        FlarkV3SourcePromotionDisposition.promoted,
      );
      expect(session.workerRevision, workerBefore);
    },
  );

  test('source-fact receipts reject a different same-shaped session', () {
    final first = FlarkV3SourceSession.fromProvisionalString('first!!');
    _acknowledgeAllWorkerSync(first);
    final firstRequest = first.beginCertification();
    final firstReceipt = FlarkV3SourceCertificationReceipt.scan(
      firstRequest,
      sourceReplica: first.document,
    );

    final second = FlarkV3SourceSession.fromProvisionalString('second!');
    _acknowledgeAllWorkerSync(second);
    second.beginCertification();
    expect(
      () => FlarkV3SourceFactScanner(
        firstRequest,
        sourceReplica: second.acknowledgedSourceReplica(),
      ),
      throwsStateError,
      reason: 'revision and UTF-16 length are not replica authority',
    );
    expect(
      () => FlarkV3SourceCertificationReceipt.scan(
        firstRequest,
        sourceReplica: second.document,
      ),
      throwsStateError,
      reason: 'the bounded helper also requires the request root identity',
    );
    expect(
      second.applyCertification(firstReceipt).disposition,
      FlarkV3SourcePromotionDisposition.stale,
    );
    expect(second.document.isFullyIndexed, isFalse);
  });

  test(
    'source stamps keep known and provisional targets explicitly tagged',
    () {
      final known = FlarkV3SourceDocument.fromString('é').sourceStamp;
      expect(known, isA<FlarkV3KnownSourceStamp>());
      expect((known as FlarkV3KnownSourceStamp).revision, 0);
      expect(known.utf16Length, 1);
      expect(known.utf8Length, 2);

      final session = FlarkV3SourceSession.fromProvisionalString('é');
      final provisional = session.document.sourceStamp;
      expect(provisional, isA<FlarkV3ProvisionalSourceStamp>());
      expect(provisional.revision, 1);
      expect(provisional.utf16Length, 1);
      final lease = session.beginWorkerSync() as FlarkV3SourceSnapshotSyncLease;
      expect(lease.baseUiRevision, 1);
      expect(lease.throughIntentSequence, 1);
      expect(lease.targetStamp, provisional);
      expect(lease.source, 'é');
    },
  );

  test(
    'snapshot credit does not install a replica without final observation',
    () {
      final session = FlarkV3SourceSession.fromString('abcd');
      final first =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 2)
              as FlarkV3SourceSnapshotSyncLease;
      expect(first.isLast, isFalse);
      expect(
        session
            .acknowledgeWorkerSync(first.acknowledgement(observedReplica: null))
            .disposition,
        FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
      );
      expect(
        session.observedWorkerReplica,
        FlarkV3ObservedSourceReplicaVersion.empty,
      );
      expect(session.acknowledgedSourceReplica, throwsStateError);

      final last =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 2)
              as FlarkV3SourceSnapshotSyncLease;
      expect(last.isLast, isTrue);
      expect(
        session
            .acknowledgeWorkerSync(last.acknowledgement(observedReplica: null))
            .disposition,
        FlarkV3SourceWorkerSyncAckDisposition.stale,
      );
      expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      expect(session.acknowledgedSourceReplica, throwsStateError);
      _acknowledgeAllWorkerSync(session);
      expect(session.acknowledgedSourceReplica().utf8Length, 4);
    },
  );

  test('source facts cannot upgrade a mismatched observed UTF-8 dimension', () {
    final session = FlarkV3SourceSession.fromProvisionalString('é');
    final lease = session.beginWorkerSync() as FlarkV3SourceSnapshotSyncLease;
    final target = lease.targetStamp;
    session.acknowledgeWorkerSync(
      lease.acknowledgement(
        observedReplica: FlarkV3ObservedSourceReplicaVersion(
          revision: target.revision,
          utf16Length: target.utf16Length,
          utf8Length: 1,
          intentHighWater: lease.throughIntentSequence,
        ),
      ),
    );
    expect(session.acknowledgedSourceReplica().utf8Length, 1);

    final request = session.beginCertification();
    final completion = _stageSourceFactsWithoutCommit(session, request);
    expect(completion.fingerprint.utf8Length, 2);
    expect(
      session.commitSourceFactCertification(completion).disposition,
      FlarkV3SourcePromotionDisposition.rejected,
    );
    expect(session.document.isFullyIndexed, isFalse);
  });

  test('worker credit drops only an exactly acknowledged journal prefix', () {
    final session = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(session);
    for (final replacement in ['a', 'bb', 'c']) {
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: session.document.utf16Length,
            endUtf16: session.document.utf16Length,
            replacement: replacement,
          ),
        ),
      );
    }

    final first =
        session.beginWorkerSync(maximumEntries: 2, maximumPayloadUtf16: 3)
            as FlarkV3SourceIntentSyncLease;
    expect(first.intents, hasLength(2));
    expect(first.payloadUtf16, 3);
    final firstAck = session.acknowledgeWorkerSync(
      _acknowledgementFor(session, first),
    );
    expect(
      firstAck.disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
    expect(firstAck.droppedIntentEntries, 2);
    expect(firstAck.droppedPayloadUtf16, 3);
    expect(session.workerRevision, 2);
    expect(session.workerSyncDiagnostics.retainedJournalEntries, 1);

    final last = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    expect(last.intents.single.sequence, greaterThan(first.lastSequence));
    session.acknowledgeWorkerSync(_acknowledgementFor(session, last));
    expect(session.workerRevision, session.uiRevision);
    expect(session.hasPendingWorkerSync, isFalse);
  });

  test('malformed current completion poisons credit and forces reseed', () {
    final session = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(session);
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'x',
        ),
      ),
    );
    final lease = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    final malformed = FlarkV3SourceIntentSyncAcknowledgement(
      sourceSessionIdentity: lease.sourceSessionIdentity,
      leaseId: lease.leaseId,
      workerGeneration: lease.workerGeneration,
      firstSequence: lease.firstSequence,
      lastSequence: lease.lastSequence + 1,
      entryCount: lease.intents.length,
      payloadUtf16: lease.payloadUtf16,
      observedReplica: _observedFor(session, lease),
    );

    expect(
      session.acknowledgeWorkerSync(malformed).disposition,
      FlarkV3SourceWorkerSyncAckDisposition.stale,
    );
    expect(session.workerSyncDiagnostics.liveLeaseCount, 0);
    expect(session.workerSyncDiagnostics.retainedJournalEntries, 0);
    expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
    expect(
      session
          .acknowledgeWorkerSync(_acknowledgementFor(session, lease))
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.stale,
    );
    _acknowledgeAllWorkerSync(session);
    expect(session.workerRevision, session.uiRevision);
  });

  test('every initial source owns a scalar-safe seed snapshot', () {
    final provisional = FlarkV3SourceSession.fromProvisionalString('a😀b');
    final cases =
        <
          ({
            FlarkV3SourceSession session,
            String source,
            int throughIntentSequence,
          })
        >[
          (
            session: FlarkV3SourceSession.fromString(''),
            source: '',
            throughIntentSequence: 0,
          ),
          (
            session: FlarkV3SourceSession.fromString('a😀b'),
            source: 'a😀b',
            throughIntentSequence: 0,
          ),
          (
            session: FlarkV3SourceSession.fromProvisionalString(''),
            source: '',
            throughIntentSequence: 0,
          ),
          (session: provisional, source: 'a😀b', throughIntentSequence: 1),
        ];

    for (final candidate in cases) {
      final session = candidate.session;
      expect(session.hasPendingWorkerSync, isTrue);
      expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      final replica = StringBuffer();
      while (session.hasPendingWorkerSync) {
        final page =
            session.beginWorkerSync(maximumSnapshotPageUtf16: 2)
                as FlarkV3SourceSnapshotSyncLease;
        expect(_isScalarBoundary(candidate.source, page.startUtf16), isTrue);
        expect(_isScalarBoundary(candidate.source, page.endUtf16), isTrue);
        expect(page.throughIntentSequence, candidate.throughIntentSequence);
        replica.write(page.source);
        session.acknowledgeWorkerSync(_acknowledgementFor(session, page));
      }
      expect(replica.toString(), candidate.source);
      expect(session.workerRevision, session.uiRevision);
    }

    provisional.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: provisional.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 4,
          endUtf16: 4,
          replacement: '!',
        ),
      ),
    );
    final next = provisional.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    expect(next.firstSequence, 2);
  });

  test('operation credit rebases instead of retaining or deadlocking', () {
    final capped = FlarkV3SourceSession.fromString(
      '',
      workerJournalOperationLimit: 2,
    );
    _acknowledgeAllWorkerSync(capped);
    capped.apply(
      FlarkV3SourceTransaction(
        baseRevision: 0,
        operations: const [
          FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'a'),
          FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'b'),
          FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'c'),
        ],
      ),
    );
    expect(capped.workerSyncDiagnostics.retainedJournalOperationCount, 0);
    expect(capped.workerSyncDiagnostics.retainedSnapshotRootCount, 1);

    final credited = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(credited);
    credited.apply(
      FlarkV3SourceTransaction(
        baseRevision: 0,
        operations: const [
          FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'a'),
          FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'b'),
        ],
      ),
    );
    final lease = credited.beginWorkerSync(maximumOperations: 1);
    expect(lease, isA<FlarkV3SourceSnapshotSyncLease>());
    expect(credited.workerSyncDiagnostics.retainedJournalOperationCount, 0);
    expect(credited.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
  });

  test('released leases retry and duplicate acknowledgements stay stale', () {
    final session = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(session);
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'x',
        ),
      ),
    );
    final first = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    expect(session.releaseWorkerSyncLease(first.leaseId + 1), isFalse);
    expect(session.releaseWorkerSyncLease(first.leaseId), isTrue);

    final retry = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    expect(retry.firstSequence, first.firstSequence);
    expect(
      session
          .acknowledgeWorkerSync(_acknowledgementFor(session, retry))
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
    expect(
      session
          .acknowledgeWorkerSync(_acknowledgementFor(session, retry))
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.stale,
    );
  });

  test(
    'snapshot rebase pages while UI edits and then converges by intents',
    () {
      final session = FlarkV3SourceSession.fromString(
        'abcdefghij',
        workerJournalEntryLimit: 2,
        workerJournalRetainedPayloadByteLimit: 64,
      );
      for (final replacement in ['x', 'y', 'z']) {
        session.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: session.uiRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: session.document.utf16Length,
              endUtf16: session.document.utf16Length,
              replacement: replacement,
            ),
          ),
        );
      }
      expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      expect(session.workerSyncDiagnostics.retainedJournalEntries, 0);

      final snapshotBuffer = StringBuffer();
      final first =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 4)
              as FlarkV3SourceSnapshotSyncLease;
      snapshotBuffer.write(first.source);
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: session.document.utf16Length,
            endUtf16: session.document.utf16Length,
            replacement: '!',
          ),
        ),
      );
      expect(session.workerSyncDiagnostics.retainedJournalEntries, 1);
      session.acknowledgeWorkerSync(_acknowledgementFor(session, first));
      expect(session.workerRevision, 0);

      while (session.workerSyncDiagnostics.retainedSnapshotRootCount == 1) {
        final page =
            session.beginWorkerSync(maximumSnapshotPageUtf16: 4)
                as FlarkV3SourceSnapshotSyncLease;
        snapshotBuffer.write(page.source);
        session.acknowledgeWorkerSync(_acknowledgementFor(session, page));
      }
      var replica = snapshotBuffer.toString();
      expect(replica, 'abcdefghijxyz');
      expect(session.workerRevision, 3);

      final intents = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
      replica = _applyWorkerIntents(replica, intents.intents);
      session.acknowledgeWorkerSync(_acknowledgementFor(session, intents));
      expect(replica, session.document.toString());
      expect(session.workerRevision, session.uiRevision);
      expect(session.hasPendingWorkerSync, isFalse);
    },
  );

  test(
    'giant change replaces a retained old snapshot in constant root state',
    () {
      final session = FlarkV3SourceSession.fromString(
        'a' * 100000,
        workerJournalEntryLimit: 8,
        workerJournalRetainedPayloadByteLimit: 1024,
      );
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 50000,
            endUtf16: 50000,
            replacement: 'x',
          ),
        ),
      );
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: session.document.utf16Length - 8,
            replacement: '',
          ),
        ),
      );
      expect(session.workerSyncDiagnostics.retainedSnapshotRootCount, 1);
      final oldLease =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 128)
              as FlarkV3SourceSnapshotSyncLease;

      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: 0,
            endUtf16: session.document.utf16Length,
            replacement: 'b' * 9000,
          ),
        ),
      );
      final diagnostics = session.workerSyncDiagnostics;
      expect(diagnostics.retainedSnapshotRootCount, 1);
      expect(diagnostics.liveLeaseCount, 1);
      expect(diagnostics.invalidatedLeaseAwaitingDrainCount, 1);
      expect(diagnostics.replacedSnapshotCount, greaterThanOrEqualTo(1));
      expect(diagnostics.snapshotInstallUtf16Copied, 0);
      expect(
        diagnostics.retainedSnapshotBackingBytesUpperBound,
        lessThanOrEqualTo(18000),
        reason: 'the superseded 100k backing is not retained by worker sync',
      );
      expect(
        session
            .acknowledgeWorkerSync(_acknowledgementFor(session, oldLease))
            .disposition,
        FlarkV3SourceWorkerSyncAckDisposition.stale,
      );
      expect(session.workerSyncDiagnostics.liveLeaseCount, 0);
    },
  );

  test('worker restart and session identity reject late acknowledgements', () {
    final firstSession = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(firstSession);
    firstSession.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'x',
        ),
      ),
    );
    final oldLease =
        firstSession.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    final oldGeneration = firstSession.workerGeneration;
    expect(firstSession.restartWorker(), oldGeneration + 1);
    expect(
      firstSession
          .acknowledgeWorkerSync(_acknowledgementFor(firstSession, oldLease))
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.stale,
    );

    final secondSession = FlarkV3SourceSession.fromString('');
    _acknowledgeAllWorkerSync(secondSession);
    secondSession.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: 0,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'x',
        ),
      ),
    );
    final secondLease =
        secondSession.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
    expect(secondLease.leaseId, oldLease.leaseId);
    expect(secondLease.workerGeneration, oldLease.workerGeneration);
    expect(
      secondSession
          .acknowledgeWorkerSync(_acknowledgementFor(firstSession, oldLease))
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.stale,
    );
    expect(
      secondSession
          .acknowledgeWorkerSync(
            _acknowledgementFor(secondSession, secondLease),
          )
          .disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  });

  test(
    '100k no-worker edits bound journal, snapshot roots, and leaf shape',
    () {
      final session = FlarkV3SourceSession.fromProvisionalString(
        'a' * 8192,
        workerJournalEntryLimit: 64,
        workerJournalRetainedPayloadByteLimit: 4096,
      );
      for (var index = 0; index < 100000; index += 1) {
        session.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: session.uiRevision,
            operation: const FlarkV3SourceEdit(
              startUtf16: 4096,
              endUtf16: 4096,
              replacement: 'x',
            ),
          ),
        );
      }
      final sync = session.workerSyncDiagnostics;
      final tree = session.document.diagnostics;
      expect(sync.retainedJournalEntries, lessThanOrEqualTo(64));
      expect(
        sync.retainedJournalSyncDebtBytesUpperBound,
        lessThanOrEqualTo(4096),
      );
      expect(sync.retainedSnapshotRootCount, 1);
      expect(sync.liveLeaseCount, 0);
      expect(tree.leafCount, lessThanOrEqualTo(32));
      expect(tree.uniqueBackingCount, lessThanOrEqualTo(32));
      expect(tree.treeHeight, lessThanOrEqualTo(8));
    },
  );

  test(
    'adversarial-position provisional typing coalesces by document size',
    () {
      final session = FlarkV3SourceSession.fromProvisionalString('a' * 8192);
      var oracle = 'a' * 8192;
      var state = 1;
      for (var index = 0; index < 10000; index += 1) {
        state = _next(state);
        final offset = state % (session.document.utf16Length + 1);
        session.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: session.uiRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: offset,
              endUtf16: offset,
              replacement: 'x',
            ),
          ),
        );
        oracle = oracle.replaceRange(offset, offset, 'x');
      }
      final tree = session.document.diagnostics;
      expect(session.document.toString(), oracle);
      expect(tree.leafCount, lessThanOrEqualTo(16));
      expect(tree.uniqueBackingCount, lessThanOrEqualTo(16));
      expect(tree.treeHeight, lessThanOrEqualTo(8));
    },
  );

  test('pending grapheme lookup is bounded exact-or-needs-more-context', () {
    const line = 'flag 🇨🇦 family 👨‍👩‍👧 decomposed é';
    final document = FlarkV3SourceDocument.fromProvisionalString(
      'previous\r\n$line',
    );
    final caret = document.utf16Length;
    final lookup = document.graphemeBefore(caret, maxContextUtf16: 128);
    expect(lookup.isCertified, isTrue);
    expect(document.readRange(lookup.startUtf16!, lookup.endUtf16!), 'é');

    final oversized = FlarkV3SourceDocument.fromProvisionalString(
      '${List.filled(10000, 'a').join()}😀',
    );
    expect(
      oversized
          .graphemeBefore(oversized.utf16Length, maxContextUtf16: 128)
          .status,
      FlarkV3GraphemeLookupStatus.needsMoreContext,
    );
    expect(
      () => oversized.graphemeBefore(
        oversized.utf16Length,
        maxContextUtf16: 9000,
      ),
      throwsRangeError,
    );
    expect(
      () => FlarkV3SourceDocument.fromString('x', chunkSize: 9000),
      throwsRangeError,
    );
    final pendingSession = FlarkV3SourceSession.fromProvisionalString(
      'pending',
    );
    _acknowledgeAllWorkerSync(pendingSession);
    final request = pendingSession.beginCertification();
    expect(
      () => FlarkV3CertifiedSourcePiece.scan(
        request.pieces.single,
        sourceFragment: 'pending',
        checkpointSpacingUtf16: 9000,
      ),
      throwsRangeError,
    );

    final splitCrLf = FlarkV3SourceDocument.fromProvisionalString('ab\r\nx');
    expect(
      splitCrLf.graphemeBefore(3).status,
      FlarkV3GraphemeLookupStatus.needsMoreContext,
    );
    final malformed = FlarkV3SourceDocument.fromProvisionalString(
      String.fromCharCodes([0x61, 0xD800, 0x62]),
    );
    expect(
      malformed.graphemeBefore(3).status,
      FlarkV3GraphemeLookupStatus.needsMoreContext,
    );
  });

  test('history and compaction charge unique backing identity', () {
    final source = List.filled(200000, 'a').join();
    final tinyDelete = FlarkV3SourceSession.fromProvisionalString(source);
    tinyDelete.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: tinyDelete.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 100000,
          endUtf16: 100001,
          replacement: '',
        ),
      ),
    );
    expect(
      tinyDelete.undoRetainedUtf16Bytes,
      greaterThanOrEqualTo(source.length * 2),
      reason: 'a one-unit slice can retain the entire provisional backing',
    );

    final session = FlarkV3SourceSession.fromProvisionalString(
      source,
      historyByteLimit: 64,
    );
    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: 4096,
          endUtf16: source.length - 4096,
          replacement: '',
        ),
      ),
    );
    expect(session.pendingCompactionObligations, hasLength(1));
    expect(
      session.compactionRetainedBackingUtf16Bytes,
      source.length * 2,
      reason: 'two survivor slices charge one shared backing once',
    );
    expect(
      session.pendingCompactionObligations.single.blockedByUndoLease,
      isTrue,
    );
    for (var poll = 0; poll < 100; poll += 1) {
      expect(session.takeCompactionObligations().obligations, isEmpty);
    }
    expect(
      session.activeCompactionLeaseCount,
      0,
      reason: 'blocked polls do not allocate durable empty leases',
    );

    session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 0,
          endUtf16: 0,
          replacement: 'x',
        ),
      ),
    );
    expect(
      session.pendingCompactionObligations.single.blockedByUndoLease,
      isFalse,
      reason: 'the next transaction evicts the oversized inverse lease',
    );
    final lease = session.takeCompactionObligations();
    expect(lease.obligations, hasLength(1));
    expect(session.pendingCompactionObligations, hasLength(1));
    session.releaseCompactionLease(lease.leaseId);
    expect(session.pendingCompactionObligations, hasLength(1));
    final accepted = session.takeCompactionObligations();
    session.acknowledgeCompactionLease(accepted.leaseId);
    expect(session.pendingCompactionObligations, isEmpty);
  });

  test('unbounded atomic operation lists fail before sorting', () {
    final document = FlarkV3SourceDocument.fromString('x');
    final operations = [
      for (var index = 0; index < 257; index += 1)
        const FlarkV3SourceEdit(startUtf16: 0, endUtf16: 0, replacement: 'x'),
    ];
    expect(
      () => document.apply(
        FlarkV3SourceTransaction(baseRevision: 0, operations: operations),
      ),
      throwsA(isA<FlarkV3SourceBulkOperationRequired>()),
    );
  });
}

void _acknowledgeAllWorkerSync(FlarkV3SourceSession session) {
  while (session.hasPendingWorkerSync) {
    final lease = session.beginWorkerSync();
    final acknowledgement = _acknowledgementFor(session, lease);
    final receipt = session.acknowledgeWorkerSync(acknowledgement);
    expect(
      receipt.disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  }
}

FlarkV3SourceWorkerSyncAcknowledgement _acknowledgementFor(
  FlarkV3SourceSession session,
  FlarkV3SourceWorkerSyncLease lease,
) => switch (lease) {
  FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
    observedReplica: lease.isLast ? _observedFor(session, lease) : null,
  ),
  FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
    observedReplica: _observedFor(session, lease),
  ),
};

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkV3SourceSession session,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  final utf8Length = switch (target) {
    FlarkV3KnownSourceStamp() => target.utf8Length,
    FlarkV3ProvisionalSourceStamp() =>
      utf8.encode(session.document.toString()).length,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: utf8Length,
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

FlarkV3SourceCertificationLineage _canonicalLineage(
  FlarkV3SourceSession session, {
  required int requestId,
}) {
  final observed = session.observedWorkerReplica;
  return FlarkV3SourceCertificationLineage(
    sourceSessionIdentity: session.sourceSessionIdentity,
    requestId: requestId,
    workerGeneration: session.workerGeneration,
    workerReplicaRevision: observed.revision,
    uiRevision: session.uiRevision,
    utf16Length: session.document.utf16Length,
    intentHighWater: observed.intentHighWater,
  );
}

FlarkV3SourcePrefixFacts _canonicalFact(String source, int utf16Offset) {
  final prefix = source.substring(0, utf16Offset);
  return FlarkV3SourcePrefixFacts(
    utf16Offset: utf16Offset,
    utf8Offset: utf8.encode(prefix).length,
    newlines: _logicalLineBreakEnds(prefix).length,
    hash: FlarkV3SourceDocument.fromString(prefix).contentHash128,
  );
}

FlarkV3CanonicalSourceFactCompletion _canonicalCompletion(
  FlarkV3SourceSession session, {
  required FlarkV3SourceCertificationLineage lineage,
  required String source,
  required List<FlarkV3SourcePrefixFacts> facts,
  required int checkpointSpacingUtf16,
}) {
  final oracle = FlarkV3SourceDocument.fromString(source);
  return FlarkV3CanonicalSourceFactCompletion(
    lineage: lineage,
    fingerprintAlgorithm: 1,
    fingerprint: FlarkV3SourceFingerprint(
      revision: session.uiRevision,
      utf16Length: source.length,
      utf8Length: utf8.encode(source).length,
      contentHash128: oracle.contentHash128,
    ),
    logicalLineBreaks: oracle.lineCount - 1,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
    checkpointCount: facts.length,
    pageCount: facts.isEmpty ? 0 : (facts.length + 63) ~/ 64,
    checkpointHash128: _portableFactsHash(facts),
  );
}

List<FlarkV3SourcePrefixFacts> _canonicalFacts(
  String source,
  int checkpointSpacingUtf16,
) {
  if (source.isEmpty) return const [];
  final facts = <FlarkV3SourcePrefixFacts>[];
  var offset = checkpointSpacingUtf16;
  while (offset < source.length) {
    facts.add(_canonicalFact(source, offset));
    offset += checkpointSpacingUtf16;
  }
  facts.add(_canonicalFact(source, source.length));
  return facts;
}

FlarkV3CanonicalSourcePromotionProof _installCanonicalGlobalFacts(
  FlarkV3SourceSession session, {
  required String source,
  required int requestId,
  required int checkpointSpacingUtf16,
}) {
  final lineage = _canonicalLineage(session, requestId: requestId);
  final facts = _canonicalFacts(source, checkpointSpacingUtf16);
  final pageCount = facts.isEmpty ? 0 : (facts.length + 63) ~/ 64;
  for (var pageOrdinal = 0; pageOrdinal < pageCount; pageOrdinal += 1) {
    final start = pageOrdinal * 64;
    final end = math.min(facts.length, start + 64);
    final receipt = session.stageCanonicalSourceFactCheckpointPage(
      FlarkV3CanonicalSourceFactCheckpointPage(
        lineage: lineage,
        pageOrdinal: pageOrdinal,
        pageCount: pageCount,
        checkpointCount: facts.length,
        checkpointSpacingUtf16: checkpointSpacingUtf16,
        checkpoints: facts.sublist(start, end),
      ),
    );
    expect(receipt.disposition, FlarkV3SourceFactStageDisposition.staged);
  }
  final promotion = session.commitCanonicalSourceFactCertification(
    _canonicalCompletion(
      session,
      lineage: lineage,
      source: source,
      facts: facts,
      checkpointSpacingUtf16: checkpointSpacingUtf16,
    ),
  );
  expect(promotion.disposition, FlarkV3SourcePromotionDisposition.promoted);
  return promotion.canonicalProof!;
}

FlarkV3CanonicalSourceFactDeltaCompletion _canonicalDeltaCompletion(
  FlarkV3SourceSession session, {
  required FlarkV3SourceCertificationLineage lineage,
  required String source,
  required List<FlarkV3SourcePrefixFacts> facts,
  required List<FlarkV3SourcePrefixFacts> replacementFacts,
  required int checkpointSpacingUtf16,
}) {
  final full = _canonicalCompletion(
    session,
    lineage: lineage,
    source: source,
    facts: facts,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
  );
  return FlarkV3CanonicalSourceFactDeltaCompletion(
    lineage: lineage,
    fingerprintAlgorithm: full.fingerprintAlgorithm,
    fingerprint: full.fingerprint,
    logicalLineBreaks: full.logicalLineBreaks,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
    checkpointCount: facts.length,
    pageCount: facts.isEmpty ? 0 : (facts.length + 63) ~/ 64,
    checkpointRootGuardAlgorithm:
        flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
    checkpointRootGuard128: full.checkpointHash128,
    replacementCheckpointHash128: _portableFactsHash(replacementFacts),
  );
}

FlarkV3ContentHash128 _portableFactsHash(List<FlarkV3SourcePrefixFacts> facts) {
  const mask32 = 0xFFFFFFFF;
  const bases = [0x00100193, 0x9E3779B1, 0x85EBCA77, 0xC2B2AE3D];
  var words = [0, 0, 0, 0];
  for (final fact in facts) {
    for (final value in [
      fact.utf16Offset,
      fact.utf8Offset,
      fact.newlines,
      fact.hash.word0,
      fact.hash.word1,
      fact.hash.word2,
      fact.hash.word3,
    ]) {
      for (var shift = 0; shift < 64; shift += 8) {
        final term = ((value >>> shift) & 0xFF) + 1;
        words = [
          for (var lane = 0; lane < 4; lane += 1)
            (words[lane] * bases[lane] + term) & mask32,
        ];
      }
    }
  }
  return FlarkV3ContentHash128(words[0], words[1], words[2], words[3]);
}

final class _StagedCertificationRun {
  const _StagedCertificationRun({
    required this.completion,
    required this.promotion,
    required this.pollCount,
    required this.maximumSourceUtf16PerPoll,
    required this.maximumSourceNodesPerPoll,
    required this.maximumCheckpointsPerPoll,
    required this.wholeSourceUtf16Copied,
  });

  final FlarkV3SourceFactCompletion completion;
  final FlarkV3SourcePromotionReceipt promotion;
  final int pollCount;
  final int maximumSourceUtf16PerPoll;
  final int maximumSourceNodesPerPoll;
  final int maximumCheckpointsPerPoll;
  final int wholeSourceUtf16Copied;
}

FlarkV3SourceFactCompletion _stageSourceFactsWithoutCommit(
  FlarkV3SourceSession session,
  FlarkV3SourceCertificationRequest request, {
  int checkpointSpacingUtf16 = 4096,
  FlarkV3SourceFactScanCredit credit = const FlarkV3SourceFactScanCredit(
    maximumSourceUtf16: 8192,
    maximumSourceNodes: 64,
    maximumOutputCheckpoints: 16,
    maximumWireBytes: 1024,
  ),
}) {
  final scanner = FlarkV3SourceFactScanner(
    request,
    sourceReplica: session.acknowledgedSourceReplica(),
    checkpointSpacingUtf16: checkpointSpacingUtf16,
  );
  while (true) {
    final poll = scanner.poll(credit);
    if (poll.page case final page?) {
      expect(
        session.stageCertificationCheckpointPage(page).disposition,
        FlarkV3SourceFactStageDisposition.staged,
      );
    }
    if (poll.completion case final completion?) return completion;
  }
}

_StagedCertificationRun _stageAllSourceFacts(
  FlarkV3SourceSession session,
  FlarkV3SourceCertificationRequest request, {
  int checkpointSpacingUtf16 = 4096,
  FlarkV3SourceFactScanCredit credit = const FlarkV3SourceFactScanCredit(
    maximumSourceUtf16: 8192,
    maximumSourceNodes: 64,
    maximumOutputCheckpoints: 16,
    maximumWireBytes: 1024,
  ),
}) {
  final replica = session.acknowledgedSourceReplica();
  final workerRevisionBefore = session.workerRevision;
  final scanner = FlarkV3SourceFactScanner(
    request,
    sourceReplica: replica,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
  );
  var polls = 0;
  var maximumSourceUtf16 = 0;
  var maximumSourceNodes = 0;
  var maximumCheckpoints = 0;
  var wholeSourceUtf16Copied = 0;
  while (true) {
    final poll = scanner.poll(credit);
    polls += 1;
    expect(polls, lessThan(10000000), reason: 'scanner must make progress');
    maximumSourceUtf16 = math.max(
      maximumSourceUtf16,
      poll.work.sourceUtf16Examined,
    );
    maximumSourceNodes = math.max(
      maximumSourceNodes,
      poll.work.sourceNodesVisited,
    );
    maximumCheckpoints = math.max(
      maximumCheckpoints,
      poll.work.checkpointsEmitted,
    );
    wholeSourceUtf16Copied += poll.work.wholeSourceUtf16Copied;
    if (poll.page case final page?) {
      final staged = session.stageCertificationCheckpointPage(page);
      expect(staged.disposition, FlarkV3SourceFactStageDisposition.staged);
      expect(staged.piecesAttached, lessThanOrEqualTo(1));
      expect(staged.pathNodesVisited, lessThanOrEqualTo(512));
      expect(page.isConsumed, isTrue);
      expect(
        session.document.isFullyIndexed,
        isFalse,
        reason: 'candidate facts must remain non-authoritative',
      );
    }
    if (poll.completion case final completion?) {
      expect(session.document.isFullyIndexed, isFalse);
      final promotion = session.commitSourceFactCertification(completion);
      expect(session.workerRevision, workerRevisionBefore);
      return _StagedCertificationRun(
        completion: completion,
        promotion: promotion,
        pollCount: polls,
        maximumSourceUtf16PerPoll: maximumSourceUtf16,
        maximumSourceNodesPerPoll: maximumSourceNodes,
        maximumCheckpointsPerPoll: maximumCheckpoints,
        wholeSourceUtf16Copied: wholeSourceUtf16Copied,
      );
    }
  }
}

String _applyWorkerIntents(String source, List<FlarkV3SourceIntent> intents) {
  var current = source;
  for (final intent in intents) {
    for (final operation in intent.operations.reversed) {
      current = current.replaceRange(
        operation.startUtf16,
        operation.endUtf16,
        operation.replacement.readRange(0, operation.replacement.utf16Length),
      );
    }
  }
  return current;
}

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

bool _isScalarBoundary(String source, int offset) {
  if (offset <= 0 || offset >= source.length) return true;
  final previous = source.codeUnitAt(offset - 1);
  final next = source.codeUnitAt(offset);
  return !(previous >= 0xD800 &&
      previous <= 0xDBFF &&
      next >= 0xDC00 &&
      next <= 0xDFFF);
}

int _logicalLineAt(String source, int offset) {
  var lines = 0;
  var cursor = 0;
  while (cursor < offset) {
    final codeUnit = source.codeUnitAt(cursor);
    if (codeUnit == 0x0D) {
      if (cursor + 1 < source.length && source.codeUnitAt(cursor + 1) == 0x0A) {
        if (cursor + 2 > offset) break;
        cursor += 2;
      } else {
        cursor += 1;
      }
      lines += 1;
      continue;
    }
    if (codeUnit == 0x0A) lines += 1;
    cursor += 1;
  }
  return lines;
}

List<int> _logicalLineBreakEnds(String source) {
  final ends = <int>[];
  var cursor = 0;
  while (cursor < source.length) {
    final codeUnit = source.codeUnitAt(cursor);
    if (codeUnit == 0x0D) {
      if (cursor + 1 < source.length && source.codeUnitAt(cursor + 1) == 0x0A) {
        cursor += 2;
      } else {
        cursor += 1;
      }
      ends.add(cursor);
      continue;
    }
    cursor += 1;
    if (codeUnit == 0x0A) ends.add(cursor);
  }
  return ends;
}

String _largeUnicodeText(int targetLength) {
  final output = StringBuffer();
  var index = 0;
  while (output.length < targetLength) {
    output.writeln(
      'Paragraph $index has 😀, café, βeta, **markdown**, and [link][shared].',
    );
    index += 1;
  }
  output.writeln('[shared]: https://example.com');
  return output.toString();
}
