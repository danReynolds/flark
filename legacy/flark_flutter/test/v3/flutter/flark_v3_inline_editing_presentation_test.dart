import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark_flutter/src/v3/flutter/flutter.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'projected quote inline styles compose through disjoint physical lines',
    () {
      const source = '> **first\n> second** and `code`\n';
      const projectedText = '**first\nsecond** and `code`\n';
      final document = FlarkV3SourceDocument.fromString(source);
      final version = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(41, 42, 43, 44),
        document: document,
      );
      final physicalSource = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: document.utf8Length,
        startUtf16: 0,
        endUtf16: document.utf16Length,
      );
      final encodedFacts = Uint8List.fromList([
        ..._record(
          kind: 2,
          start: 0,
          length: 16,
          contentStart: 2,
          contentLength: 12,
        ),
        ..._record(
          kind: 3,
          start: 21,
          length: 6,
          contentStart: 22,
          contentLength: 4,
        ),
      ]);
      final facts = FlarkV3ProjectedInlineFactsDecoder.decode(
        sourceDocument: document,
        expectedSource: version,
        factSource: version,
        expectedProfilePartition: 3,
        profilePartition: 3,
        expectedPhysicalSource: physicalSource,
        factPhysicalSource: physicalSource,
        expectedProjectedUtf8Length: projectedText.length,
        expectedProjectedUtf16Length: projectedText.length,
        projectedText: projectedText,
        disposition: FlarkV3ProjectedInlineFactsDisposition.authoritative,
        factCount: 2,
        encodedFacts: encodedFacts,
      );
      final projectedInline =
          FlarkV3ProjectedInlineProjection.fromValidatedFacts(
            projectedText: projectedText,
            facts: facts,
          );
      final quoteProjection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: source,
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 0,
            sourceEndUtf16: 2,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 2,
            sourceEndUtf16: 10,
          ),
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 10,
            sourceEndUtf16: 12,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 12,
            sourceEndUtf16: 32,
          ),
        ],
        certifiedSourceVersion: version,
      );
      final lease =
          FlarkV3ProjectedInputLease.fromSourceProjectionWithProjectedInline(
            quoteProjection,
            projectedInline,
            editPolicy: FlarkV3BlockQuoteEditPolicy(),
          );

      expect(lease.displayText, 'first\nsecond and code\n');
      expect(lease.sourceToDisplayOffset(0), 0);
      expect(lease.sourceToDisplayOffset(4), 0);
      expect(lease.sourceToDisplayOffset(10), 6);
      expect(lease.sourceToDisplayOffset(12), 6);
      expect(lease.sourceToDisplayOffset(20), 12);
      expect(lease.sourceToDisplayOffset(26), 17);

      final span = lease.buildTextSpan(
        baseStyle: const TextStyle(fontSize: 14),
        composing: TextRange.empty,
      );
      final runs = span.children!.cast<TextSpan>().toList();
      expect(runs.map((run) => run.text).join(), lease.displayText);
      expect(
        runs
            .where((run) => run.text!.contains('first'))
            .single
            .style!
            .fontWeight,
        FontWeight.w700,
      );
      expect(
        runs
            .where((run) => run.text!.contains('second'))
            .single
            .style!
            .fontWeight,
        FontWeight.w700,
      );
      expect(
        runs.where((run) => run.text == 'code').single.style!.fontFamily,
        'monospace',
      );

      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 8,
        displayEndUtf16: 8,
        replacement: 'X',
        nextDisplayValue: const TextEditingValue(
          text: 'first\nseXcond and code\n',
          selection: TextSelection.collapsed(offset: 9),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 14),
        preferredSourceComposing: TextRange.empty,
      );
      expect(edit.sourceStartUtf16, 14);
      expect(edit.sourceEndUtf16, 14);
      expect(
        source.replaceRange(
          edit.sourceStartUtf16,
          edit.sourceEndUtf16,
          edit.sourceReplacement,
        ),
        '> **first\n> seXcond** and `code`\n',
      );
      expect(edit.nextLease.displayText, 'first\nseXcond and code\n');
    },
  );

  testWidgets(
    'certified inline styles hide markers and preserve EditableText state',
    (tester) async {
      const source = '**bold** *em* `code` ~~gone~~';
      final document = FlarkV3SourceDocument.fromString(source);
      final version = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(31, 32, 33, 34),
        document: document,
      );
      final leaf = FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: document.utf8Length,
        startUtf16: 0,
        endUtf16: document.utf16Length,
      );
      final records = [
        _record(
          kind: 2,
          start: 0,
          length: 8,
          contentStart: 2,
          contentLength: 4,
        ),
        _record(
          kind: 1,
          start: 9,
          length: 4,
          contentStart: 10,
          contentLength: 2,
        ),
        _record(
          kind: 3,
          start: 14,
          length: 6,
          contentStart: 15,
          contentLength: 4,
        ),
        _record(
          kind: 4,
          start: 21,
          length: 8,
          contentStart: 23,
          contentLength: 4,
        ),
      ];
      final facts = FlarkV3InlineFactsDecoder.decode(
        sourceDocument: document,
        expectedSource: version,
        factSource: version,
        expectedProfilePartition: 3,
        profilePartition: 3,
        expectedLeaf: leaf,
        factLeaf: leaf,
        disposition: FlarkV3InlineFactsDisposition.authoritative,
        factCount: records.length,
        encodedFacts: Uint8List.fromList([
          for (final record in records) ...record,
        ]),
      );
      final decision = FlarkV3InlineIslandPresentation.resolve(
        sourceDocument: document,
        expectedSource: version,
        structuralQuery: FlarkV3DocumentStructuralQuery(
          sourceRevision: version.revision,
          structureRevision: version.revision,
          structure: FlarkV3DocumentStructure(
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: leaf,
            visibleSource: leaf,
            referenceDefinitionCount: 0,
          ),
          projection: FlarkV3DocumentProjection(
            kind: FlarkV3DocumentStructureKind.paragraph,
            source: leaf,
            projectedSource: leaf,
            runCount: 1,
          ),
          inlineFacts: facts,
        ),
        activeIsland: leaf,
      );
      expect(decision, isA<FlarkV3AuthoritativeInlineIslandPresentation>());
      final presentation = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        decision as FlarkV3AuthoritativeInlineIslandPresentation,
      );
      final controller = FlarkV3InlineTextEditingController.fromValue(
        TextEditingValue(text: presentation.text),
      );
      final focusNode = FocusNode();
      addTearDown(controller.dispose);
      addTearDown(focusNode.dispose);
      final editableKey = GlobalKey<EditableTextState>();

      Widget editor() => Directionality(
        textDirection: TextDirection.ltr,
        child: EditableText(
          key: editableKey,
          controller: controller,
          focusNode: focusNode,
          style: const TextStyle(color: Color(0xFF202020), fontSize: 14),
          cursorColor: const Color(0xFF006ADC),
          backgroundCursorColor: const Color(0x00000000),
        ),
      );

      await tester.pumpWidget(editor());
      final editableState = editableKey.currentState;
      controller.adoptProjectedInputLease(presentation.inputLease);
      await tester.pumpWidget(editor());

      expect(editableKey.currentState, same(editableState));
      final span = controller.buildTextSpan(
        context: editableKey.currentContext!,
        style: const TextStyle(color: Color(0xFF202020), fontSize: 14),
        withComposing: false,
      );
      final runs = span.children!.cast<TextSpan>().toList();
      expect(runs.map((run) => run.text).join(), 'bold em code gone');
      expect(runs.map((run) => run.text).join(), isNot(contains('*')));
      expect(runs.map((run) => run.text).join(), isNot(contains('`')));
      expect(runs.map((run) => run.text).join(), isNot(contains('~')));
      expect(
        runs.singleWhere((run) => run.text == 'bold').style!.fontWeight,
        FontWeight.w700,
      );
      expect(
        runs.singleWhere((run) => run.text == 'em').style!.fontStyle,
        FontStyle.italic,
      );
      expect(
        runs
            .singleWhere((run) => run.text == 'gone')
            .style!
            .decoration!
            .contains(TextDecoration.lineThrough),
        isTrue,
      );
      final composingSpan = presentation.inputLease.buildTextSpan(
        baseStyle: const TextStyle(color: Color(0xFF202020), fontSize: 14),
        composing: const TextRange(start: 13, end: 17),
      );
      final composingRun = composingSpan.children!.cast<TextSpan>().singleWhere(
        (run) => run.text == 'gone',
      );
      expect(
        composingRun.style!.decoration!.contains(TextDecoration.lineThrough),
        isTrue,
      );
      expect(
        composingRun.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      expect(
        runs.singleWhere((run) => run.text == 'code').style!.fontFamily,
        'monospace',
      );

      for (final fixture in const [
        (
          sourceCaret: 0,
          expectedSourceCaret: 0,
          expected: 'a**bold** *em* `code` ~~gone~~',
        ),
        (
          sourceCaret: 1,
          expectedSourceCaret: 2,
          expected: '**abold** *em* `code` ~~gone~~',
        ),
        (
          sourceCaret: 2,
          expectedSourceCaret: 2,
          expected: '**abold** *em* `code` ~~gone~~',
        ),
      ]) {
        final edit = presentation.inputLease.applyDisplayEdit(
          displayStartUtf16: 0,
          displayEndUtf16: 0,
          replacement: 'a',
          nextDisplayValue: const TextEditingValue(
            text: 'abold em code gone',
            selection: TextSelection.collapsed(offset: 1),
          ),
          preferredSourceSelection: TextSelection.collapsed(
            offset: fixture.sourceCaret,
          ),
          preferredSourceComposing: TextRange.empty,
        );
        expect(edit.sourceStartUtf16, fixture.expectedSourceCaret);
        expect(edit.sourceEndUtf16, fixture.expectedSourceCaret);
        expect(
          source.replaceRange(
            edit.sourceStartUtf16,
            edit.sourceEndUtf16,
            edit.replacement,
          ),
          fixture.expected,
        );
      }
    },
  );

  testWidgets(
    'active autolinks hide angles and remain ordinary editable text',
    (tester) async {
      const uri = 'https://example.com';
      const email = 'dev@example.com';
      const source = '<$uri> <$email>';
      final uriSourceLength = uri.length + 2;
      final emailSourceStart = uriSourceLength + 1;
      final authoritative = _resolveAuthoritativeInline(
        source,
        records: [
          _record(
            kind: 5,
            start: 0,
            length: uriSourceLength,
            contentStart: 1,
            contentLength: uri.length,
          ),
          _record(
            kind: 6,
            start: emailSourceStart,
            length: email.length + 2,
            contentStart: emailSourceStart + 1,
            contentLength: email.length,
          ),
        ],
      );
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;
      expect(lease.displayText, '$uri $email');
      expect(
        lease.displayComposingToSource(const TextRange(start: 0, end: 5)),
        const TextRange(start: 1, end: 6),
      );

      final controller = FlarkV3InlineTextEditingController.fromValue(
        TextEditingValue(text: lease.displayText),
      );
      final focusNode = FocusNode();
      final editableKey = GlobalKey<EditableTextState>();
      addTearDown(controller.dispose);
      addTearDown(focusNode.dispose);
      final semantics = tester.ensureSemantics();

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: EditableText(
            key: editableKey,
            controller: controller,
            focusNode: focusNode,
            style: const TextStyle(color: Color(0xFF202020), fontSize: 14),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      final editableState = editableKey.currentState;
      controller.adoptProjectedInputLease(lease);
      await tester.pump();

      final initialSpan = controller.buildTextSpan(
        context: editableKey.currentContext!,
        style: const TextStyle(color: Color(0xFF202020), fontSize: 14),
        withComposing: false,
      );
      final initialRuns = initialSpan.children!.cast<TextSpan>().toList();
      expect(initialRuns.map((run) => run.text).join(), '$uri $email');
      expect(initialRuns.map((run) => run.text).join(), isNot(contains('<')));
      expect(initialRuns.map((run) => run.text).join(), isNot(contains('>')));
      for (final target in const [uri, email]) {
        final run = initialRuns.singleWhere(
          (candidate) => candidate.text == target,
        );
        expect(
          run.style!.decoration!.contains(TextDecoration.underline),
          isTrue,
        );
        expect(run.recognizer, isNull);
      }
      expect(find.semantics.byFlag(SemanticsFlag.isTextField), findsOne);
      expect(find.semantics.byFlag(SemanticsFlag.isLink), findsNothing);

      final renderEditable = editableKey.currentState!.renderEditable;
      final uriBox = renderEditable
          .getBoxesForSelection(
            TextSelection(baseOffset: 0, extentOffset: uri.length),
          )
          .first
          .toRect();
      await tester.tapAt(renderEditable.localToGlobal(uriBox.center));
      await tester.pump();

      expect(focusNode.hasFocus, isTrue);
      expect(editableKey.currentState, same(editableState));
      expect(
        controller.selection.extentOffset,
        inInclusiveRange(0, uri.length),
      );

      const insertionOffset = 8;
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: insertionOffset,
        displayEndUtf16: insertionOffset,
        replacement: 'x',
        nextDisplayValue: TextEditingValue(
          text: lease.displayText.replaceRange(
            insertionOffset,
            insertionOffset,
            'x',
          ),
          selection: const TextSelection.collapsed(offset: insertionOffset + 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(
          offset: insertionOffset + 1,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect(edit.sourceStartUtf16, insertionOffset + 1);
      expect(edit.nextLease.isCertified, isFalse);
      final provisionalSpan = edit.nextLease.buildTextSpan(
        baseStyle: const TextStyle(color: Color(0xFF202020), fontSize: 14),
        composing: const TextRange(
          start: insertionOffset,
          end: insertionOffset + 1,
        ),
      );
      expect(provisionalSpan.toPlainText(), 'https://xexample.com $email');
      for (final run in provisionalSpan.children!.cast<TextSpan>()) {
        expect(run.recognizer, isNull);
      }
      final insertedRun = provisionalSpan.children!
          .cast<TextSpan>()
          .singleWhere((run) => run.text == 'x');
      expect(
        insertedRun.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      semantics.dispose();
    },
  );

  test('active markerless autolinks use ordinary identity edits', () {
    const scheme = 'https://e.test';
    const www = 'www.e.test';
    const source = 'pre $scheme post $www tail';
    final schemeStart = source.indexOf(scheme);
    final wwwStart = source.indexOf(www);
    final authoritative = _resolveAuthoritativeInline(
      source,
      records: [
        _record(
          kind: 5,
          start: schemeStart,
          length: scheme.length,
          contentStart: schemeStart,
          contentLength: scheme.length,
        ),
        _record(
          kind: 5,
          flags: 1,
          start: wwwStart,
          length: www.length,
          contentStart: wwwStart,
          contentLength: www.length,
        ),
      ],
    );
    final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
      authoritative,
    ).inputLease;

    expect(lease.displayText, source);
    expect(lease.sourceStartUtf16, 0);
    expect(lease.sourceEndUtf16, source.length);
    expect(authoritative.projection.delimiterTopology.isEmpty, isTrue);
    expect(
      authoritative.projection.runs
          .where((run) => run.linkAnnotation != null)
          .map((run) => run.linkAnnotation!.destination),
      [scheme, 'http://$www'],
    );
    for (var offset = 0; offset <= source.length; offset += 1) {
      expect(lease.sourceToDisplayOffset(offset), offset);
      expect(
        lease.displayToSourceOffset(
          offset,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        offset,
      );
      expect(
        lease.displayToSourceOffset(
          offset,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        offset,
      );
    }

    final insertion = lease.applyDisplayEdit(
      displayStartUtf16: schemeStart + 8,
      displayEndUtf16: schemeStart + 8,
      replacement: 'x',
      nextDisplayValue: TextEditingValue(
        text: source.replaceRange(schemeStart + 8, schemeStart + 8, 'x'),
        selection: TextSelection.collapsed(offset: schemeStart + 9),
      ),
      preferredSourceSelection: TextSelection.collapsed(
        offset: schemeStart + 9,
      ),
      preferredSourceComposing: TextRange.empty,
    );
    expect(
      (insertion.sourceStartUtf16, insertion.sourceEndUtf16),
      (schemeStart + 8, schemeStart + 8),
    );
    expect(insertion.sourceReplacement, 'x');
    expect(insertion.displayReplacement, 'x');
    expect(insertion.nextLease.isCertified, isFalse);
    expect(
      insertion.nextLease.displayText,
      source.replaceRange(schemeStart + 8, schemeStart + 8, 'x'),
    );

    final atFinalLabelBoundary = wwwStart + www.length;
    final boundaryInsertion = lease.applyDisplayEdit(
      displayStartUtf16: atFinalLabelBoundary,
      displayEndUtf16: atFinalLabelBoundary,
      replacement: 'x',
      nextDisplayValue: TextEditingValue(
        text: source.replaceRange(
          atFinalLabelBoundary,
          atFinalLabelBoundary,
          'x',
        ),
        selection: TextSelection.collapsed(offset: atFinalLabelBoundary + 1),
      ),
      preferredSourceSelection: TextSelection.collapsed(
        offset: atFinalLabelBoundary + 1,
      ),
      preferredSourceComposing: TextRange.empty,
    );
    expect(
      (boundaryInsertion.sourceStartUtf16, boundaryInsertion.sourceEndUtf16),
      (atFinalLabelBoundary, atFinalLabelBoundary),
    );
    expect(boundaryInsertion.sourceReplacement, 'x');
    expect(boundaryInsertion.displayReplacement, 'x');
  });

  test(
    'active URI autolink cooks its entity label and edits the source atomically',
    () {
      const entity = '&amp;';
      const source = '<https://e.test/?q=&amp;>';
      const cookedTarget = 'https://e.test/?q=&';
      const editedDisplay = 'https://e.test/?q=x';
      final entityStart = source.indexOf(entity);
      final authoritative = _resolveAuthoritativeInline(
        source,
        records: [
          _record(
            kind: 5,
            start: 0,
            length: source.length,
            contentStart: 1,
            contentLength: source.length - 2,
          ),
          _characterReferenceRecord(
            start: entityStart,
            length: entity.length,
            first: 0x26,
          ),
        ],
      );
      final projection = authoritative.projection;
      expect(projection.runs.last.linkAnnotation?.destination, cookedTarget);
      expect(
        projection.runs.last.linkAnnotation?.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.characterReferenceProjectedContent,
      );
      expect(projection.runs.last.semanticFacts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.autolinkUri,
        FlarkV3InlineFactKind.characterReference,
      ]);

      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;
      expect(lease.displayText, cookedTarget);
      final initialSpan = lease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(initialSpan.toPlainText(), cookedTarget);
      for (final run in initialSpan.children!.cast<TextSpan>()) {
        expect(
          run.style!.decoration!.contains(TextDecoration.underline),
          isTrue,
        );
      }

      final entityDisplayStart = cookedTarget.length - 1;
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: entityDisplayStart,
        displayEndUtf16: cookedTarget.length,
        replacement: 'x',
        nextDisplayValue: TextEditingValue(
          text: editedDisplay,
          selection: TextSelection.collapsed(offset: editedDisplay.length),
        ),
        preferredSourceSelection: TextSelection(
          baseOffset: entityStart,
          extentOffset: entityStart + entity.length,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect(
        (edit.sourceStartUtf16, edit.sourceEndUtf16),
        (entityStart, entityStart + entity.length),
      );
      expect(edit.sourceReplacement, 'x');
      const editedSource = '<https://e.test/?q=x>';
      expect(
        source.replaceRange(
          edit.sourceStartUtf16,
          edit.sourceEndUtf16,
          edit.sourceReplacement,
        ),
        editedSource,
      );
      expect(edit.nextLease.displayText, editedDisplay);
      final provisionalSpan = edit.nextLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      for (final run in provisionalSpan.children!.cast<TextSpan>()) {
        expect(
          run.style!.decoration!.contains(TextDecoration.underline),
          isTrue,
        );
      }

      final recertified = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          editedSource,
          records: [
            _record(
              kind: 5,
              start: 0,
              length: editedSource.length,
              contentStart: 1,
              contentLength: editedSource.length - 2,
            ),
          ],
        ),
      ).inputLease;
      expect(recertified.displayText, edit.nextLease.displayText);
    },
  );

  test(
    'active direct links and image alts stay marker-free and non-actionable',
    () {
      final directLink = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          '[*label*](dest "title")',
          records: [
            _record(
              kind: 10,
              start: 0,
              length: 23,
              contentStart: 1,
              contentLength: 7,
            ),
            _record(
              kind: 1,
              start: 1,
              length: 7,
              contentStart: 2,
              contentLength: 5,
            ),
          ],
          valueEntries: const [
            _InlineValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 10,
              destinationLength: 4,
              titleStart: 15,
              titleLength: 7,
              cookedDestination: 'dest',
              cookedTitle: 'title',
            ),
          ],
        ),
      ).inputLease;
      expect(directLink.displayText, 'label');
      final directSpan = directLink.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(directSpan.toPlainText(), 'label');
      final directRun = directSpan.children!.single as TextSpan;
      expect(directRun.style!.fontStyle, FontStyle.italic);
      expect(
        directRun.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      expect(directRun.recognizer, isNull);

      final imageOnly = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          '![[inside](ignored)](image-only)',
          records: [
            _record(
              kind: 11,
              start: 0,
              length: 32,
              contentStart: 2,
              contentLength: 17,
            ),
            _record(
              kind: 10,
              start: 2,
              length: 17,
              contentStart: 3,
              contentLength: 6,
            ),
          ],
          valueEntries: const [
            _InlineValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 21,
              destinationLength: 10,
              cookedDestination: 'image-only',
            ),
            _InlineValueEntry(
              parentFactOrdinal: 1,
              destinationStart: 11,
              destinationLength: 7,
              cookedDestination: 'ignored',
            ),
          ],
        ),
      ).inputLease;
      expect(imageOnly.displayText, 'inside');
      final imageOnlySpan = imageOnly.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(imageOnlySpan.toPlainText(), 'inside');
      final altRun = imageOnlySpan.children!.single as TextSpan;
      expect(altRun.style!.decoration, isNull);
      expect(altRun.recognizer, isNull);

      final linkedImage = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          '[![hero](hero.png)](outer)',
          records: [
            _record(
              kind: 10,
              start: 0,
              length: 26,
              contentStart: 1,
              contentLength: 17,
            ),
            _record(
              kind: 11,
              start: 1,
              length: 17,
              contentStart: 3,
              contentLength: 4,
            ),
          ],
          valueEntries: const [
            _InlineValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 20,
              destinationLength: 5,
              cookedDestination: 'outer',
            ),
            _InlineValueEntry(
              parentFactOrdinal: 1,
              destinationStart: 9,
              destinationLength: 8,
              cookedDestination: 'hero.png',
            ),
          ],
        ),
      ).inputLease;
      expect(linkedImage.displayText, 'hero');
      final linkedImageSpan = linkedImage.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      final linkedAlt = linkedImageSpan.children!.single as TextSpan;
      expect(
        linkedAlt.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
        reason: 'the surrounding link keeps paint but has no active gesture',
      );
      expect(linkedAlt.recognizer, isNull);

      final emptyAlt = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          '![](empty.png)',
          records: [
            _record(
              kind: 11,
              start: 0,
              length: 14,
              contentStart: 2,
              contentLength: 0,
            ),
          ],
          valueEntries: const [
            _InlineValueEntry(
              parentFactOrdinal: 0,
              destinationStart: 4,
              destinationLength: 9,
              cookedDestination: 'empty.png',
            ),
          ],
        ),
      ).inputLease;
      expect(emptyAlt.displayText, isEmpty);
      expect(
        emptyAlt
            .buildTextSpan(
              baseStyle: const TextStyle(),
              composing: TextRange.empty,
            )
            .toPlainText(),
        isEmpty,
      );
    },
  );

  test(
    'active reference links and image alts use resolved values marker-free',
    () {
      const linkUse = '[*label*][id]';
      const linkSource = '$linkUse\n\n[id]: /resolved "ref title"';
      final linkDestination = linkSource.indexOf('/resolved');
      final linkTitle = linkSource.indexOf('"ref title"');
      final authoritativeLink = _resolveAuthoritativeInline(
        linkSource,
        leafEndUtf16: linkUse.length,
        records: [
          _record(
            kind: 12,
            start: 0,
            length: linkUse.length,
            contentStart: 1,
            contentLength: 7,
          ),
          _record(
            kind: 1,
            start: 1,
            length: 7,
            contentStart: 2,
            contentLength: 5,
          ),
        ],
        valueEntries: [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: linkDestination,
            destinationLength: '/resolved'.length,
            titleStart: linkTitle,
            titleLength: '"ref title"'.length,
            cookedDestination: '/resolved',
            cookedTitle: 'ref title',
          ),
        ],
      );
      final linkAnnotation =
          authoritativeLink.facts.facts.first.linkAnnotation!;
      expect(linkAnnotation.kind, FlarkV3InlineLinkKind.reference);
      expect(linkAnnotation.destination, '/resolved');
      expect(linkAnnotation.title, 'ref title');
      expect(linkAnnotation.destinationSource.startUtf16, linkDestination);
      expect(linkAnnotation.titleSource!.startUtf16, linkTitle);

      final linkLease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritativeLink,
      ).inputLease;
      expect(linkLease.displayText, 'label');
      final linkSpan = linkLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(linkSpan.toPlainText(), 'label');
      final linkRun = linkSpan.children!.single as TextSpan;
      expect(linkRun.style!.fontStyle, FontStyle.italic);
      expect(
        linkRun.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      expect(linkRun.recognizer, isNull);

      const imageUse = '![[inside][inner]][image]';
      const imageSource =
          '$imageUse\n\n[inner]: /ignored\n[image]: /image-only "image title"';
      final innerDestination = imageSource.indexOf('/ignored');
      final imageDestination = imageSource.indexOf('/image-only');
      final imageTitle = imageSource.indexOf('"image title"');
      final imageLease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          imageSource,
          leafEndUtf16: imageUse.length,
          records: [
            _record(
              kind: 13,
              start: 0,
              length: imageUse.length,
              contentStart: 2,
              contentLength: 15,
            ),
            _record(
              kind: 12,
              start: 2,
              length: 15,
              contentStart: 3,
              contentLength: 6,
            ),
          ],
          valueEntries: [
            _InlineValueEntry(
              parentFactOrdinal: 0,
              destinationStart: imageDestination,
              destinationLength: '/image-only'.length,
              titleStart: imageTitle,
              titleLength: '"image title"'.length,
              cookedDestination: '/image-only',
              cookedTitle: 'image title',
            ),
            _InlineValueEntry(
              parentFactOrdinal: 1,
              destinationStart: innerDestination,
              destinationLength: '/ignored'.length,
              cookedDestination: '/ignored',
            ),
          ],
        ),
      ).inputLease;
      expect(imageLease.displayText, 'inside');
      final imageAltSpan = imageLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(imageAltSpan.toPlainText(), 'inside');
      final imageAltRun = imageAltSpan.children!.single as TextSpan;
      expect(
        imageAltRun.style!.decoration,
        isNull,
        reason: 'a reference link nested in image alt is not actionable',
      );
      expect(imageAltRun.recognizer, isNull);

      const linkedImageUse = '[![hero][img]][outer]';
      const linkedImageSource =
          '$linkedImageUse\n\n[img]: /hero.png\n[outer]: /outer';
      final imageValue = linkedImageSource.indexOf('/hero.png');
      final outerValue = linkedImageSource.indexOf('/outer');
      final linkedImageLease =
          FlarkV3FlutterInlinePresentation.fromAuthoritative(
            _resolveAuthoritativeInline(
              linkedImageSource,
              leafEndUtf16: linkedImageUse.length,
              records: [
                _record(
                  kind: 12,
                  start: 0,
                  length: linkedImageUse.length,
                  contentStart: 1,
                  contentLength: 12,
                ),
                _record(
                  kind: 13,
                  start: 1,
                  length: 12,
                  contentStart: 3,
                  contentLength: 4,
                ),
              ],
              valueEntries: [
                _InlineValueEntry(
                  parentFactOrdinal: 0,
                  destinationStart: outerValue,
                  destinationLength: '/outer'.length,
                  cookedDestination: '/outer',
                ),
                _InlineValueEntry(
                  parentFactOrdinal: 1,
                  destinationStart: imageValue,
                  destinationLength: '/hero.png'.length,
                  cookedDestination: '/hero.png',
                ),
              ],
            ),
          ).inputLease;
      expect(linkedImageLease.displayText, 'hero');
      final linkedImageSpan = linkedImageLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      final linkedImageRun = linkedImageSpan.children!.single as TextSpan;
      expect(
        linkedImageRun.style!.decoration!.contains(TextDecoration.underline),
        isTrue,
        reason: 'the enclosing reference link retains its paint',
      );
      expect(linkedImageRun.recognizer, isNull);
    },
  );

  test('nested hidden markers retain source selection and caret affinity', () {
    const source = '***x***';
    final document = FlarkV3SourceDocument.fromString(source);
    final version = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(41, 42, 43, 44),
      document: document,
    );
    final leaf = FlarkV3SourceSpan(
      startUtf8: 0,
      endUtf8: document.utf8Length,
      startUtf16: 0,
      endUtf16: document.utf16Length,
    );
    final records = [
      _record(kind: 1, start: 0, length: 7, contentStart: 1, contentLength: 5),
      _record(kind: 2, start: 1, length: 5, contentStart: 3, contentLength: 1),
    ];
    final facts = FlarkV3InlineFactsDecoder.decode(
      sourceDocument: document,
      expectedSource: version,
      factSource: version,
      expectedProfilePartition: 3,
      profilePartition: 3,
      expectedLeaf: leaf,
      factLeaf: leaf,
      disposition: FlarkV3InlineFactsDisposition.authoritative,
      factCount: records.length,
      encodedFacts: Uint8List.fromList([
        for (final record in records) ...record,
      ]),
    );
    final decision = FlarkV3InlineIslandPresentation.resolve(
      sourceDocument: document,
      expectedSource: version,
      structuralQuery: FlarkV3DocumentStructuralQuery(
        sourceRevision: version.revision,
        structureRevision: version.revision,
        structure: FlarkV3DocumentStructure(
          kind: FlarkV3DocumentStructureKind.paragraph,
          source: leaf,
          visibleSource: leaf,
          referenceDefinitionCount: 0,
        ),
        projection: FlarkV3DocumentProjection(
          kind: FlarkV3DocumentStructureKind.paragraph,
          source: leaf,
          projectedSource: leaf,
          runCount: 1,
        ),
        inlineFacts: facts,
      ),
      activeIsland: leaf,
    );
    final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
      decision as FlarkV3AuthoritativeInlineIslandPresentation,
    ).inputLease;

    expect(lease.displayText, 'x');
    final styled = lease.buildTextSpan(
      baseStyle: const TextStyle(),
      composing: TextRange.empty,
    );
    final nestedRun = styled.children!.single as TextSpan;
    expect(nestedRun.text, 'x');
    expect(nestedRun.style!.fontStyle, FontStyle.italic);
    expect(nestedRun.style!.fontWeight, FontWeight.w700);
    const forwardSource = TextSelection(baseOffset: 3, extentOffset: 4);
    const reverseSource = TextSelection(baseOffset: 4, extentOffset: 3);
    const forwardDisplay = TextSelection(baseOffset: 0, extentOffset: 1);
    const reverseDisplay = TextSelection(baseOffset: 1, extentOffset: 0);
    expect(lease.sourceSelectionToDisplay(forwardSource), forwardDisplay);
    expect(lease.sourceSelectionToDisplay(reverseSource), reverseDisplay);
    expect(
      lease.displaySelectionToSource(
        forwardDisplay,
        preferredSourceSelection: forwardSource,
      ),
      forwardSource,
    );
    expect(
      lease.displaySelectionToSource(
        reverseDisplay,
        preferredSourceSelection: reverseSource,
      ),
      reverseSource,
    );

    for (final fixture in const [
      (displayCaret: 0, sourceCaret: 0, expected: 'a***x***'),
      (displayCaret: 0, sourceCaret: 3, expected: '***ax***'),
      (displayCaret: 1, sourceCaret: 4, expected: '***xa***'),
      (displayCaret: 1, sourceCaret: 7, expected: '***x***a'),
    ]) {
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: fixture.displayCaret,
        displayEndUtf16: fixture.displayCaret,
        replacement: 'a',
        nextDisplayValue: TextEditingValue(
          text: fixture.displayCaret == 0 ? 'ax' : 'xa',
          selection: TextSelection.collapsed(offset: fixture.displayCaret + 1),
        ),
        preferredSourceSelection: TextSelection.collapsed(
          offset: fixture.sourceCaret,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect(edit.sourceStartUtf16, fixture.sourceCaret);
      expect(edit.sourceEndUtf16, fixture.sourceCaret);
      expect(
        source.replaceRange(
          edit.sourceStartUtf16,
          edit.sourceEndUtf16,
          edit.replacement,
        ),
        fixture.expected,
      );
      expect(
        edit.nextLease.displayText,
        fixture.displayCaret == 0 ? 'ax' : 'xa',
      );
    }

    final backspace = lease.applyDisplayEdit(
      displayStartUtf16: 0,
      displayEndUtf16: 1,
      replacement: '',
      nextDisplayValue: const TextEditingValue(
        selection: TextSelection.collapsed(offset: 0),
      ),
      preferredSourceSelection: const TextSelection.collapsed(offset: 4),
      preferredSourceComposing: TextRange.empty,
    );
    expect(backspace.sourceStartUtf16, 0);
    expect(backspace.sourceEndUtf16, 7);
    expect(
      source.replaceRange(
        backspace.sourceStartUtf16,
        backspace.sourceEndUtf16,
        backspace.replacement,
      ),
      isEmpty,
    );
    expect(backspace.sourceSelection, const TextSelection.collapsed(offset: 0));
    expect(backspace.nextLease.displayText, isEmpty);

    final replacement = backspace.nextLease.applyDisplayEdit(
      displayStartUtf16: 0,
      displayEndUtf16: 0,
      replacement: 'y',
      nextDisplayValue: const TextEditingValue(
        text: 'y',
        selection: TextSelection.collapsed(offset: 1),
      ),
      preferredSourceSelection: backspace.sourceSelection,
      preferredSourceComposing: TextRange.empty,
    );
    expect(replacement.sourceStartUtf16, 0);
    expect(replacement.sourceEndUtf16, 0);
    expect(
      ''.replaceRange(
        replacement.sourceStartUtf16,
        replacement.sourceEndUtf16,
        replacement.replacement,
      ),
      'y',
    );
    expect(
      replacement.sourceSelection,
      const TextSelection.collapsed(offset: 1),
    );
    final replacementSpan = replacement.nextLease.buildTextSpan(
      baseStyle: const TextStyle(),
      composing: TextRange.empty,
    );
    final replacementRun = replacementSpan.children!.single as TextSpan;
    expect(replacementRun.text, 'y');
    expect(replacementRun.style!.fontStyle, isNull);
    expect(replacementRun.style!.fontWeight, isNull);
  });

  test(
    'escaped punctuation edits atomically and retains parent style authority',
    () {
      const source = r'**\***';
      final authoritative = _resolveAuthoritativeInline(
        source,
        records: [
          _record(
            kind: 2,
            start: 0,
            length: 6,
            contentStart: 2,
            contentLength: 2,
          ),
          _record(
            kind: 7,
            start: 2,
            length: 2,
            contentStart: 3,
            contentLength: 1,
          ),
        ],
      );
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;
      expect(lease.displayText, '*');
      final initial = lease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(
        (initial.children!.single as TextSpan).style!.fontWeight,
        isNotNull,
      );

      final replacement = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 1,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: 'x',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 3,
          extentOffset: 4,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect(
        (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
        (2, 4),
      );
      expect(
        source.replaceRange(
          replacement.sourceStartUtf16,
          replacement.sourceEndUtf16,
          replacement.sourceReplacement,
        ),
        '**x**',
      );
      final replacementSpan = replacement.nextLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(
        (replacementSpan.children!.single as TextSpan).style!.fontWeight,
        isNotNull,
        reason:
            'consuming the escape atom must retain its certified strong parent',
      );

      final before = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: 'a',
        nextDisplayValue: const TextEditingValue(
          text: 'a*',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 3),
        preferredSourceComposing: TextRange.empty,
      );
      expect((before.sourceStartUtf16, before.sourceEndUtf16), (2, 2));
      expect(
        source.replaceRange(
          before.sourceStartUtf16,
          before.sourceEndUtf16,
          before.sourceReplacement,
        ),
        r'**a\***',
      );
      expect(before.nextLease.displayText, 'a*');

      final after = lease.applyDisplayEdit(
        displayStartUtf16: 1,
        displayEndUtf16: 1,
        replacement: 'a',
        nextDisplayValue: const TextEditingValue(
          text: '*a',
          selection: TextSelection.collapsed(offset: 2),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 4),
        preferredSourceComposing: TextRange.empty,
      );
      expect((after.sourceStartUtf16, after.sourceEndUtf16), (4, 4));
      expect(
        source.replaceRange(
          after.sourceStartUtf16,
          after.sourceEndUtf16,
          after.sourceReplacement,
        ),
        r'**\*a**',
      );
      expect(after.nextLease.displayText, '*a');

      final deletion = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 1,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          selection: TextSelection.collapsed(offset: 0),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 3,
          extentOffset: 4,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect((deletion.sourceStartUtf16, deletion.sourceEndUtf16), (0, 6));
      expect(deletion.sourceReplacement, isEmpty);
      expect(deletion.nextLease.displayText, isEmpty);
    },
  );

  test(
    'character-reference active paint and atomic edits converge after recertification',
    () {
      const entity = '&NotEqualTilde;';
      const cooked = '\u2242\u0338';
      const source = '**&NotEqualTilde;**';
      final authoritative = _resolveAuthoritativeInline(
        source,
        records: [
          _record(
            kind: 2,
            start: 0,
            length: source.length,
            contentStart: 2,
            contentLength: entity.length,
          ),
          _characterReferenceRecord(
            start: 2,
            length: entity.length,
            first: 0x2242,
            second: 0x0338,
          ),
        ],
      );
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;

      expect(lease.displayText, cooked);
      final initialSpan = lease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      final initialRun = initialSpan.children!.single as TextSpan;
      expect(initialRun.text, cooked);
      expect(initialRun.style!.fontWeight, FontWeight.w700);

      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 1,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: 'x\u0338',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 2),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 2,
          extentOffset: 3,
        ),
        preferredSourceComposing: const TextRange(start: 2, end: 3),
      );

      expect(
        (edit.sourceStartUtf16, edit.sourceEndUtf16),
        (2, 2 + entity.length),
      );
      expect(edit.sourceReplacement, 'x\u0338');
      const editedSource = '**x\u0338**';
      expect(
        source.replaceRange(
          edit.sourceStartUtf16,
          edit.sourceEndUtf16,
          edit.sourceReplacement,
        ),
        editedSource,
      );
      expect(edit.nextLease.displayText, 'x\u0338');
      expect(edit.displayValue.text, edit.nextLease.displayText);
      expect(edit.sourceComposing, const TextRange(start: 2, end: 4));
      final provisionalSpan = edit.nextLease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: edit.displayValue.composing,
      );
      expect(
        provisionalSpan.children!.cast<TextSpan>().every(
          (run) => run.style!.fontWeight == FontWeight.w700,
        ),
        isTrue,
      );

      final recertified = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          editedSource,
          records: [
            _record(
              kind: 2,
              start: 0,
              length: 7,
              contentStart: 2,
              contentLength: 3,
            ),
          ],
        ),
      ).inputLease;
      expect(recertified.displayText, edit.nextLease.displayText);
      final recertifiedSpan = recertified.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      expect(
        (recertifiedSpan.children!.single as TextSpan).style!.fontWeight,
        FontWeight.w700,
      );
    },
  );

  test(
    'character-reference active edit rejects an endpoint inside a surrogate pair',
    () {
      const entity = '&#x1F600;';
      final authoritative = _resolveAuthoritativeInline(
        entity,
        records: [
          _characterReferenceRecord(
            start: 0,
            length: entity.length,
            first: 0x1F600,
          ),
        ],
      );
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;

      expect(lease.displayText, '\u{1F600}');
      expect(
        () => lease.applyDisplayEdit(
          displayStartUtf16: 1,
          displayEndUtf16: 1,
          replacement: 'x',
          nextDisplayValue: const TextEditingValue(
            text: '\uD83Dx\uDE00',
            selection: TextSelection.collapsed(offset: 2),
          ),
          preferredSourceSelection: const TextSelection.collapsed(offset: 0),
          preferredSourceComposing: TextRange.empty,
        ),
        throwsStateError,
      );
    },
  );

  test('hard line breaks hide only their certified marker', () {
    for (final fixture in const [
      (
        source: 'a  \nb',
        factStart: 1,
        factLength: 3,
        contentStart: 3,
        contentLength: 1,
        display: 'a\nb',
      ),
      (
        source: 'a\\\rb',
        factStart: 1,
        factLength: 2,
        contentStart: 2,
        contentLength: 1,
        display: 'a\nb',
      ),
      (
        source: 'a  \r\nb',
        factStart: 1,
        factLength: 4,
        contentStart: 3,
        contentLength: 2,
        display: 'a\nb',
      ),
    ]) {
      final authoritative = _resolveAuthoritativeInline(
        fixture.source,
        records: [
          _record(
            kind: 8,
            start: fixture.factStart,
            length: fixture.factLength,
            contentStart: fixture.contentStart,
            contentLength: fixture.contentLength,
          ),
        ],
      );
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        authoritative,
      ).inputLease;

      expect(lease.displayText, fixture.display);
      expect(
        lease
            .buildTextSpan(
              baseStyle: const TextStyle(),
              composing: TextRange.empty,
            )
            .toPlainText(),
        fixture.display,
      );
      expect(
        lease.displayText,
        isNot(contains(fixture.source.substring(1, fixture.contentStart))),
      );

      final enclosingProjection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: fixture.source,
        pieces: [
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 0,
            sourceEndUtf16: fixture.source.length,
          ),
        ],
        certifiedSourceVersion: authoritative.facts.sourceVersion,
      );
      final composed =
          FlarkV3ProjectedInputLease.fromSourceProjectionWithAuthoritativeInline(
            enclosingProjection,
            authoritative,
          );
      expect(composed.displayText, fixture.display);
    }

    const softSource = 'a\nb';
    final softLease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
      _resolveAuthoritativeInline(softSource, records: const []),
    ).inputLease;
    expect(softLease.displayText, softSource);
    for (var offset = 0; offset <= softSource.length; offset += 1) {
      expect(softLease.sourceToDisplayOffset(offset), offset);
    }
  });

  test('emphasis remains styled across a certified hard line break', () {
    const source = '*a  \nb*';
    final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
      _resolveAuthoritativeInline(
        source,
        records: [
          _record(
            kind: 1,
            start: 0,
            length: 7,
            contentStart: 1,
            contentLength: 5,
          ),
          _record(
            kind: 8,
            start: 2,
            length: 3,
            contentStart: 4,
            contentLength: 1,
          ),
        ],
      ),
    ).inputLease;

    expect(lease.displayText, 'a\nb');
    final runs = lease
        .buildTextSpan(baseStyle: const TextStyle(), composing: TextRange.empty)
        .children!
        .cast<TextSpan>();
    expect(runs.map((run) => run.text).join(), 'a\nb');
    expect(
      runs.where((run) => run.text!.isNotEmpty),
      everyElement(
        isA<TextSpan>().having(
          (run) => run.style?.fontStyle,
          'fontStyle',
          FontStyle.italic,
        ),
      ),
    );
  });

  test(
    'hard line break joins and boundary insertions consume no hidden tail',
    () {
      const source = 'a  \nb';
      final lease = FlarkV3FlutterInlinePresentation.fromAuthoritative(
        _resolveAuthoritativeInline(
          source,
          records: [
            _record(
              kind: 8,
              start: 1,
              length: 3,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
        ),
      ).inputLease;
      expect(lease.displayText, 'a\nb');

      for (final sourceCaret in const [1, 4]) {
        final join = lease.applyDisplayEdit(
          displayStartUtf16: 1,
          displayEndUtf16: 2,
          replacement: '',
          nextDisplayValue: const TextEditingValue(
            text: 'ab',
            selection: TextSelection.collapsed(offset: 1),
          ),
          preferredSourceSelection: TextSelection.collapsed(
            offset: sourceCaret,
          ),
          preferredSourceComposing: TextRange.empty,
        );
        expect(
          (join.sourceStartUtf16, join.sourceEndUtf16),
          (1, 4),
          reason:
              'Backspace/Delete over the visible EOL must consume its certified '
              'hard-break marker atomically.',
        );
        expect(
          source.replaceRange(
            join.sourceStartUtf16,
            join.sourceEndUtf16,
            join.sourceReplacement,
          ),
          'ab',
        );
        expect(join.nextLease.displayText, 'ab');
      }

      final beforeBreak = lease.applyDisplayEdit(
        displayStartUtf16: 1,
        displayEndUtf16: 1,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: 'ax\nb',
          selection: TextSelection.collapsed(offset: 2),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 3),
        preferredSourceComposing: TextRange.empty,
      );
      expect(
        (beforeBreak.sourceStartUtf16, beforeBreak.sourceEndUtf16),
        (1, 1),
      );
      expect(
        source.replaceRange(
          beforeBreak.sourceStartUtf16,
          beforeBreak.sourceEndUtf16,
          beforeBreak.sourceReplacement,
        ),
        'ax  \nb',
      );
      expect(beforeBreak.nextLease.displayText, 'ax\nb');

      final afterBreak = lease.applyDisplayEdit(
        displayStartUtf16: 2,
        displayEndUtf16: 2,
        replacement: 'y',
        nextDisplayValue: const TextEditingValue(
          text: 'a\nyb',
          selection: TextSelection.collapsed(offset: 3),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 4),
        preferredSourceComposing: TextRange.empty,
      );
      expect((afterBreak.sourceStartUtf16, afterBreak.sourceEndUtf16), (4, 4));
      expect(
        source.replaceRange(
          afterBreak.sourceStartUtf16,
          afterBreak.sourceEndUtf16,
          afterBreak.sourceReplacement,
        ),
        'a  \nyb',
      );

      final replacement = lease.applyDisplayEdit(
        displayStartUtf16: 1,
        displayEndUtf16: 2,
        replacement: 'x',
        nextDisplayValue: const TextEditingValue(
          text: 'axb',
          selection: TextSelection.collapsed(offset: 2),
        ),
        preferredSourceSelection: const TextSelection(
          baseOffset: 3,
          extentOffset: 4,
        ),
        preferredSourceComposing: TextRange.empty,
      );
      expect(
        (replacement.sourceStartUtf16, replacement.sourceEndUtf16),
        (1, 4),
      );
      expect(
        source.replaceRange(
          replacement.sourceStartUtf16,
          replacement.sourceEndUtf16,
          replacement.sourceReplacement,
        ),
        'axb',
      );
    },
  );
}

FlarkV3AuthoritativeInlineIslandPresentation _resolveAuthoritativeInline(
  String source, {
  required List<Uint8List> records,
  List<_InlineValueEntry> valueEntries = const <_InlineValueEntry>[],
  int? leafEndUtf16,
}) {
  final document = FlarkV3SourceDocument.fromString(source);
  final version = FlarkV3SourceVersion.fromDocument(
    documentSession: FlarkV3DocumentSessionId(51, 52, 53, 54),
    document: document,
  );
  final resolvedLeafEndUtf16 = leafEndUtf16 ?? document.utf16Length;
  final leaf = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: utf8.encode(source.substring(0, resolvedLeafEndUtf16)).length,
    startUtf16: 0,
    endUtf16: resolvedLeafEndUtf16,
  );
  final facts = FlarkV3InlineFactsDecoder.decode(
    sourceDocument: document,
    expectedSource: version,
    factSource: version,
    expectedProfilePartition: 3,
    profilePartition: 3,
    expectedLeaf: leaf,
    factLeaf: leaf,
    disposition: FlarkV3InlineFactsDisposition.authoritative,
    factCount: records.length,
    encodedFacts: Uint8List.fromList([for (final record in records) ...record]),
    inlineValues: valueEntries.isEmpty
        ? null
        : FlarkV3InlineValuesPayload(
            sourceVersion: version,
            profilePartition: 3,
            source: leaf,
            encodedBytes: _encodeInlineValues(valueEntries),
          ),
  );
  final decision = FlarkV3InlineIslandPresentation.resolve(
    sourceDocument: document,
    expectedSource: version,
    structuralQuery: FlarkV3DocumentStructuralQuery(
      sourceRevision: version.revision,
      structureRevision: version.revision,
      structure: FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: leaf,
        visibleSource: leaf,
        referenceDefinitionCount: 0,
      ),
      projection: FlarkV3DocumentProjection(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: leaf,
        projectedSource: leaf,
        runCount: 1,
      ),
      inlineFacts: facts,
    ),
    activeIsland: leaf,
  );
  return decision as FlarkV3AuthoritativeInlineIslandPresentation;
}

Uint8List _record({
  required int kind,
  int flags = 0,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, kind)
    ..setUint8(1, flags)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

Uint8List _characterReferenceRecord({
  required int start,
  required int length,
  required int first,
  int? second,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 9)
    ..setUint8(1, second == null ? 1 : 2)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, first, Endian.little)
    ..setUint32(16, second ?? 0, Endian.little);
  return bytes;
}

Uint8List _encodeInlineValues(List<_InlineValueEntry> entries) {
  final cooked = [
    for (final entry in entries)
      (
        destination: Uint8List.fromList(utf8.encode(entry.cookedDestination)),
        title: Uint8List.fromList(utf8.encode(entry.cookedTitle ?? '')),
      ),
  ];
  final bytes = Uint8List(
    16 +
        entries.length * 32 +
        cooked.fold(
          0,
          (sum, value) => sum + value.destination.length + value.title.length,
        ),
  );
  bytes.setRange(0, 8, ascii.encode('FLKIV001'));
  final data = ByteData.sublistView(bytes)
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, entries.length, Endian.little);
  var offset = 16;
  for (var index = 0; index < entries.length; index += 1) {
    final entry = entries[index];
    final value = cooked[index];
    data
      ..setUint32(offset, entry.parentFactOrdinal, Endian.little)
      ..setUint32(offset + 4, entry.cookedTitle == null ? 0 : 1, Endian.little)
      ..setUint32(offset + 8, entry.destinationStart, Endian.little)
      ..setUint32(offset + 12, entry.destinationLength, Endian.little)
      ..setUint32(offset + 16, entry.titleStart, Endian.little)
      ..setUint32(offset + 20, entry.titleLength, Endian.little)
      ..setUint32(offset + 24, value.destination.length, Endian.little)
      ..setUint32(offset + 28, value.title.length, Endian.little);
    offset += 32;
    bytes.setRange(
      offset,
      offset + value.destination.length,
      value.destination,
    );
    offset += value.destination.length;
    bytes.setRange(offset, offset + value.title.length, value.title);
    offset += value.title.length;
  }
  return bytes;
}

final class _InlineValueEntry {
  const _InlineValueEntry({
    required this.parentFactOrdinal,
    required this.destinationStart,
    required this.destinationLength,
    this.titleStart = 0,
    this.titleLength = 0,
    required this.cookedDestination,
    this.cookedTitle,
  });

  final int parentFactOrdinal;
  final int destinationStart;
  final int destinationLength;
  final int titleStart;
  final int titleLength;
  final String cookedDestination;
  final String? cookedTitle;
}
