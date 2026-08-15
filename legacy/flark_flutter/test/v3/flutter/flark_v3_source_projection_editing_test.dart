import 'package:flark/flark_adapter.dart';
import 'package:flark_flutter/src/v3/flutter/flutter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('generic lease paints parser-authored display literals', () {
    final lease = FlarkV3ProjectedInputLease.fromSourceProjection(
      FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: '\u0000\r\n',
        pieces: const [
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 0,
            sourceEndUtf16: 1,
            displayText: '\uFFFD',
          ),
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 1,
            sourceEndUtf16: 3,
            displayText: '\n',
          ),
        ],
      ),
    );

    expect(lease.displayText, '\uFFFD\n');
    final span = lease.buildTextSpan(
      baseStyle: const TextStyle(),
      composing: TextRange.empty,
    );
    expect(
      span.children!.cast<TextSpan>().map((child) => child.text).join(),
      '\uFFFD\n',
    );
  });

  test(
    'partial cooked replacement edits canonicalize the complete source token',
    () {
      const source = '&NotEqualTilde;';
      const cooked = '\u2242\u0338';
      final lease = FlarkV3ProjectedInputLease.fromSourceProjection(
        FlarkV3SourceProjection.fromSource(
          sourceStartUtf16: 5,
          sourceText: source,
          pieces: const [
            FlarkV3SourceProjectionPiece.replace(
              sourceStartUtf16: 5,
              sourceEndUtf16: 20,
              displayText: cooked,
            ),
          ],
        ),
      );

      final replacement = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 1,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: 'x\u0338',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 12),
        preferredSourceComposing: TextRange.empty,
      );

      expect(
        (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
        (5, 20),
      );
      expect(replacement.sourceReplacement, 'x\u0338');
      expect(replacement.displayReplacement, 'x\u0338');
      expect(replacement.nextLease.displayText, 'x\u0338');
      expect(
        replacement.sourceSelection,
        const TextSelection.collapsed(offset: 6),
      );
      expect(replacement.sourceComposing, const TextRange(start: 5, end: 6));

      final insertion = lease.applyDisplayEdit(
        displayStartUtf16: 1,
        displayEndUtf16: 1,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: '\u2242x\u0338',
          selection: TextSelection.collapsed(offset: 2),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 12),
        preferredSourceComposing: TextRange.empty,
      );
      expect((insertion.sourceStartUtf16, insertion.sourceEndUtf16), (5, 20));
      expect(insertion.sourceReplacement, '\u2242x\u0338');
      expect(insertion.nextLease.displayText, '\u2242x\u0338');
      expect(
        insertion.sourceSelection,
        const TextSelection.collapsed(offset: 7),
      );
    },
  );

  test('replacement boundary edits preserve adjacent CRLF source', () {
    final lease = FlarkV3ProjectedInputLease.fromSourceProjection(
      FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: '&amp;\r\n',
        pieces: const [
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 0,
            sourceEndUtf16: 5,
            displayText: '&',
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 5,
            sourceEndUtf16: 7,
          ),
        ],
      ),
    );

    final deletion = lease.applyDisplayEdit(
      displayStartUtf16: 0,
      displayEndUtf16: 1,
      replacement: '',
      nextDisplayValue: const TextEditingValue(
        text: '\r\n',
        selection: TextSelection.collapsed(offset: 0),
      ),
      preferredSourceSelection: const TextSelection(
        baseOffset: 0,
        extentOffset: 5,
      ),
      preferredSourceComposing: TextRange.empty,
    );
    expect((deletion.sourceStartUtf16, deletion.sourceEndUtf16), (0, 5));
    expect(deletion.sourceReplacement, isEmpty);
    expect(deletion.nextLease.displayText, '\r\n');

    final insertion = lease.applyDisplayEdit(
      displayStartUtf16: 1,
      displayEndUtf16: 1,
      replacement: 'x',
      nextDisplayValue: const TextEditingValue(
        text: '&x\r\n',
        selection: TextSelection.collapsed(offset: 2),
      ),
      preferredSourceSelection: const TextSelection.collapsed(offset: 5),
      preferredSourceComposing: TextRange.empty,
    );
    expect((insertion.sourceStartUtf16, insertion.sourceEndUtf16), (5, 5));
    expect(insertion.sourceReplacement, 'x');
    expect(insertion.nextLease.displayText, '&x\r\n');
  });

  test('source carets inside an entity never split a cooked emoji', () {
    const source = '&#x1F600;';
    final lease = FlarkV3ProjectedInputLease.fromSourceProjection(
      FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: source,
        pieces: [
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 0,
            sourceEndUtf16: source.length,
            displayText: '\u{1F600}',
          ),
        ],
      ),
    );

    for (var sourceOffset = 0; sourceOffset < source.length; sourceOffset++) {
      expect(
        lease.sourceSelectionToDisplay(
          TextSelection.collapsed(offset: sourceOffset),
        ),
        const TextSelection.collapsed(offset: 0),
      );
    }
    expect(
      lease.sourceSelectionToDisplay(
        TextSelection.collapsed(offset: source.length),
      ),
      const TextSelection.collapsed(offset: 2),
    );
  });

  test('policy emits canonical indentation without echoing it in display', () {
    final lease = _indentedLease(editPolicy: const _IndentedBlockEditPolicy());
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
    expect(edit.replacement, edit.sourceReplacement);
    expect(edit.nextLease.displayText, 'one\ntwo\n');
    expect(edit.sourceSelection, const TextSelection.collapsed(offset: 20));
    expect(edit.sourceComposing, const TextRange(start: 15, end: 16));
  });

  test('cross-line hidden-prefix deletion requires an explicit policy', () {
    final sourceBackedLease = _indentedLease();
    expect(
      () => sourceBackedLease.applyDisplayEdit(
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
      ),
      throwsStateError,
    );

    final policyLease = _indentedLease(
      editPolicy: const _IndentedBlockEditPolicy(),
    );
    final edit = policyLease.applyDisplayEdit(
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
    expect(edit.sourceSelection, const TextSelection.collapsed(offset: 6));
  });

  test(
    'hidden insertion, replacement, and deletion require distinct authority',
    () {
      final projection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: r'\*',
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 0,
            sourceEndUtf16: 1,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 1,
            sourceEndUtf16: 2,
          ),
        ],
      );
      const policy = FlarkV3SourceBackedProjectionEditPolicy();

      FlarkV3SourceProjectionEditRequest request({
        required String replacement,
        bool authorizeDeletion = false,
        bool authorizeReplacement = false,
      }) => FlarkV3SourceProjectionEditRequest(
        projection: projection,
        sourceStartUtf16: 0,
        sourceEndUtf16: 2,
        displayStartUtf16: 0,
        displayEndUtf16: 1,
        displayReplacement: replacement,
        preauthorizedHiddenDeletion: authorizeDeletion,
        preauthorizedHiddenReplacement: authorizeReplacement,
      );

      expect(
        () => policy.planEdit(request(replacement: 'x')),
        throwsStateError,
      );
      expect(
        () =>
            policy.planEdit(request(replacement: 'x', authorizeDeletion: true)),
        throwsStateError,
        reason: 'deletion authority must not authorize a hidden replacement',
      );
      final replacement = policy.planEdit(
        request(replacement: 'x', authorizeReplacement: true),
      );
      expect(
        (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
        (0, 2),
      );
      expect(replacement.replacement.sourceReplacement, 'x');

      expect(
        () => policy.planEdit(
          request(replacement: '', authorizeReplacement: true),
        ),
        throwsStateError,
        reason: 'replacement authority must not authorize hidden deletion',
      );
      final deletion = policy.planEdit(
        request(replacement: '', authorizeDeletion: true),
      );
      expect((deletion.sourceStartUtf16, deletion.sourceEndUtf16), (0, 2));
      expect(deletion.replacement.sourceReplacement, isEmpty);

      final mergedHiddenProjection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: r'**\*',
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 0,
            sourceEndUtf16: 3,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 3,
            sourceEndUtf16: 4,
          ),
        ],
      );
      FlarkV3SourceProjectionEditRequest insertion({
        bool authorizeInsertion = false,
        bool authorizeReplacement = false,
      }) => FlarkV3SourceProjectionEditRequest(
        projection: mergedHiddenProjection,
        sourceStartUtf16: 2,
        sourceEndUtf16: 2,
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        displayReplacement: 'a',
        preauthorizedHiddenInsertion: authorizeInsertion,
        preauthorizedHiddenReplacement: authorizeReplacement,
      );

      expect(() => policy.planEdit(insertion()), throwsStateError);
      expect(
        () => policy.planEdit(insertion(authorizeReplacement: true)),
        throwsStateError,
        reason: 'replacement authority must not authorize a hidden insertion',
      );
      final inserted = policy.planEdit(insertion(authorizeInsertion: true));
      expect((inserted.sourceStartUtf16, inserted.sourceEndUtf16), (2, 2));
      expect(inserted.replacement.sourceReplacement, 'a');
    },
  );
}

FlarkV3ProjectedInputLease _indentedLease({
  FlarkV3SourceProjectionEditPolicy editPolicy =
      const FlarkV3SourceBackedProjectionEditPolicy(),
}) {
  return FlarkV3ProjectedInputLease.fromSourceProjection(
    FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: 0,
      sourceText: '    one\n    two',
      pieces: const [
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: 0,
          sourceEndUtf16: 4,
        ),
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: 4,
          sourceEndUtf16: 8,
        ),
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: 8,
          sourceEndUtf16: 12,
        ),
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: 12,
          sourceEndUtf16: 15,
        ),
      ],
    ),
    editPolicy: editPolicy,
  );
}

final class _IndentedBlockEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  const _IndentedBlockEditPolicy();

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) {
    if (request.sourceStartUtf16 == request.sourceEndUtf16 &&
        request.displayReplacement == '\n') {
      return FlarkV3SourceProjectionEditPlan(
        sourceStartUtf16: request.sourceStartUtf16,
        sourceEndUtf16: request.sourceEndUtf16,
        replacement: FlarkV3SourceProjectionReplacement.projected(
          sourceReplacement: '\n    ',
          pieces: const [
            FlarkV3SourceProjectionPiece.copy(
              sourceStartUtf16: 0,
              sourceEndUtf16: 1,
            ),
            FlarkV3SourceProjectionPiece.hide(
              sourceStartUtf16: 1,
              sourceEndUtf16: 5,
            ),
          ],
        ),
      );
    }
    if (request.displayReplacement.isEmpty && request.intersectsHiddenSource) {
      return FlarkV3SourceProjectionEditPlan.identity(request);
    }
    return const FlarkV3SourceBackedProjectionEditPolicy().planEdit(request);
  }
}
