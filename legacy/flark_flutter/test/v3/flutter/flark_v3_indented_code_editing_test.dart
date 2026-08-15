import 'package:flark/flark_adapter.dart';
import 'package:flark_flutter/src/v3/flutter/flutter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('FlarkV3IndentedCodeEditPolicy', () {
    test('Enter commits a hidden canonical continuation prefix', () {
      final lease = _topLevelLease();
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 7,
        displayEndUtf16: 7,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'one\ntwo\n',
          selection: TextSelection.collapsed(offset: 8),
          composing: TextRange(start: 7, end: 8),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 15),
        preferredSourceComposing: TextRange.empty,
      );

      expect(edit.sourceStartUtf16, 15);
      expect(edit.sourceEndUtf16, 15);
      expect(edit.sourceReplacement, '\n    ');
      expect(edit.displayReplacement, '\n');
      expect(edit.nextLease.displayText, 'one\ntwo\n');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 20));
      expect(edit.sourceComposing, const TextRange(start: 15, end: 16));

      // The mechanically derived lease is provisional but must remain live
      // while parser recertification is in flight.
      expect(edit.nextLease.isCertified, isFalse);
      final second = edit.nextLease.applyDisplayEdit(
        displayStartUtf16: 8,
        displayEndUtf16: 8,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'one\ntwo\n\n',
          selection: TextSelection.collapsed(offset: 9),
        ),
        preferredSourceSelection: edit.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );
      expect(second.sourceReplacement, '\n    ');
      expect(second.displayReplacement, '\n');
    });

    test('configured CRLF remains source-only while display inserts LF', () {
      final lease = FlarkV3ProjectedInputLease.fromSourceProjection(
        _unicodeCrLfProjection(),
        editPolicy: FlarkV3IndentedCodeEditPolicy(canonicalLineEnding: '\r\n'),
      );

      expect(lease.displayText, 'α🌍\nβ');
      expect(
        lease.displayToSourceOffset(
          3,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        7,
      );

      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 3,
        displayEndUtf16: 3,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'α🌍\n\nβ',
          selection: TextSelection.collapsed(offset: 4),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 7),
        preferredSourceComposing: TextRange.empty,
      );

      expect(edit.sourceStartUtf16, 7);
      expect(edit.sourceReplacement, '\r\n    ');
      expect(edit.displayReplacement, '\n');
      expect(edit.nextLease.displayText, 'α🌍\n\nβ');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 13));
    });

    test('backspace over a line ending consumes the exact hidden prefix', () {
      final lease = _topLevelLease();
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 3,
        displayEndUtf16: 4,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'onetwo',
          selection: TextSelection.collapsed(offset: 3),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 12),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (7, 12));
      expect(edit.sourceReplacement, '');
      expect(edit.displayReplacement, '');
      expect(edit.nextLease.displayText, 'onetwo');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 7));
    });

    test('cross-line selections consume only complete hidden prefixes', () {
      final lease = _topLevelLease();
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 2,
        displayEndUtf16: 5,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'onwo',
          selection: TextSelection.collapsed(offset: 2),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 6,
          extentOffset: 13,
        ),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (6, 13));
      expect(edit.sourceReplacement, '');
      expect(edit.displayReplacement, '');
      expect(edit.nextLease.displayText, 'onwo');
    });

    test('ordinary visible edits preserve source exactly', () {
      final lease = _topLevelLease();
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 1,
        displayEndUtf16: 2,
        replacement: '🌍',
        nextDisplayValue: const TextEditingValue(
          text: 'o🌍e\ntwo',
          selection: TextSelection.collapsed(offset: 3),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 5,
          extentOffset: 6,
        ),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (5, 6));
      expect(edit.sourceReplacement, '🌍');
      expect(edit.displayReplacement, '🌍');
    });

    test('other hidden-source intersections fail closed', () {
      final policy = FlarkV3IndentedCodeEditPolicy();
      final inlineLikeProjection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: 'a**b',
        pieces: const [
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 0,
            sourceEndUtf16: 1,
          ),
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 1,
            sourceEndUtf16: 3,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 3,
            sourceEndUtf16: 4,
          ),
        ],
      );

      expect(
        () => policy.planEdit(
          FlarkV3SourceProjectionEditRequest(
            projection: inlineLikeProjection,
            sourceStartUtf16: 0,
            sourceEndUtf16: 4,
            displayStartUtf16: 0,
            displayEndUtf16: 2,
            displayReplacement: '',
          ),
        ),
        throwsStateError,
      );
      expect(
        () => policy.planEdit(
          FlarkV3SourceProjectionEditRequest(
            projection: _topLevelProjection(),
            sourceStartUtf16: 7,
            sourceEndUtf16: 12,
            displayStartUtf16: 3,
            displayEndUtf16: 4,
            displayReplacement: 'x',
          ),
        ),
        throwsStateError,
      );
    });

    test('continuation configuration is explicit and bounded', () {
      expect(
        () => FlarkV3IndentedCodeEditPolicy(canonicalContinuationPrefix: ''),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3IndentedCodeEditPolicy(
          canonicalContinuationPrefix:
              'x' *
              (FlarkV3IndentedCodeEditPolicy
                      .maximumCanonicalContinuationPrefixUtf16 +
                  1),
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3IndentedCodeEditPolicy(canonicalLineEnding: '\n\r'),
        throwsArgumentError,
      );
    });
  });
}

FlarkV3ProjectedInputLease _topLevelLease() =>
    FlarkV3ProjectedInputLease.fromSourceProjection(
      _topLevelProjection(),
      editPolicy: FlarkV3IndentedCodeEditPolicy(),
    );

FlarkV3SourceProjection _topLevelProjection() {
  const source = '    one\n    two';
  return FlarkV3SourceProjection.fromSource(
    sourceStartUtf16: 0,
    sourceText: source,
    pieces: const [
      FlarkV3SourceProjectionPiece.hide(sourceStartUtf16: 0, sourceEndUtf16: 4),
      FlarkV3SourceProjectionPiece.copy(sourceStartUtf16: 4, sourceEndUtf16: 8),
      FlarkV3SourceProjectionPiece.hide(
        sourceStartUtf16: 8,
        sourceEndUtf16: 12,
      ),
      FlarkV3SourceProjectionPiece.copy(
        sourceStartUtf16: 12,
        sourceEndUtf16: 15,
      ),
    ],
    certifiedSourceVersion: _sourceVersion(source.length),
  );
}

FlarkV3SourceProjection _unicodeCrLfProjection() {
  const source = '    α🌍\r\n    β';
  return FlarkV3SourceProjection.fromSource(
    sourceStartUtf16: 0,
    sourceText: source,
    pieces: const [
      FlarkV3SourceProjectionPiece.hide(sourceStartUtf16: 0, sourceEndUtf16: 4),
      FlarkV3SourceProjectionPiece.copy(sourceStartUtf16: 4, sourceEndUtf16: 7),
      FlarkV3SourceProjectionPiece.replace(
        sourceStartUtf16: 7,
        sourceEndUtf16: 9,
        displayText: '\n',
      ),
      FlarkV3SourceProjectionPiece.hide(
        sourceStartUtf16: 9,
        sourceEndUtf16: 13,
      ),
      FlarkV3SourceProjectionPiece.copy(
        sourceStartUtf16: 13,
        sourceEndUtf16: 14,
      ),
    ],
    certifiedSourceVersion: _sourceVersion(source.length),
  );
}

FlarkV3SourceVersion _sourceVersion(int utf16Length) => FlarkV3SourceVersion(
  documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
  revision: 9,
  metric: FlarkV3SourceMetric(bytes: utf16Length * 4, utf16: utf16Length),
  contentHash: const FlarkV3ContentHash128(5, 6, 7, 8),
);
