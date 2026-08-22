import 'dart:convert';
import 'dart:typed_data';

// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('FlarkV3TightBulletListItemEditPolicy', () {
    test('certified inline facts compose with marker-free list structure', () {
      const source = '- **bold** *em* `code`\r\n';
      final document = FlarkV3SourceDocument.fromString(source);
      final version = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
        document: document,
      );
      final content = FlarkV3SourceSpan(
        startUtf8: 2,
        endUtf8: 22,
        startUtf16: 2,
        endUtf16: 22,
      );
      final facts = FlarkV3InlineFactsDecoder.decode(
        sourceDocument: document,
        expectedSource: version,
        factSource: version,
        expectedProfilePartition: 3,
        profilePartition: 3,
        expectedLeaf: content,
        factLeaf: content,
        disposition: FlarkV3InlineFactsDisposition.authoritative,
        factCount: 3,
        encodedFacts: Uint8List.fromList([
          ..._inlineRecord(
            kind: 2,
            start: 0,
            length: 8,
            contentStart: 2,
            contentLength: 4,
          ),
          ..._inlineRecord(
            kind: 1,
            start: 9,
            length: 4,
            contentStart: 10,
            contentLength: 2,
          ),
          ..._inlineRecord(
            kind: 3,
            start: 14,
            length: 6,
            contentStart: 15,
            contentLength: 4,
          ),
        ]),
      );
      final authoritative = _authoritativeInlinePresentation(
        document: document,
        version: version,
        content: content,
        facts: facts,
      );
      final structuralProjection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: source,
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 0,
            sourceEndUtf16: 2,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 2,
            sourceEndUtf16: 22,
          ),
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 22,
            sourceEndUtf16: 24,
            displayText: '\n',
          ),
        ],
        certifiedSourceVersion: version,
      );
      final lease =
          FlarkV3ProjectedInputLease.fromSourceProjectionWithAuthoritativeInline(
            structuralProjection,
            authoritative,
            editPolicy: FlarkV3TightBulletListItemEditPolicy(
              configuration: _configuration(canonicalLineEnding: '\r\n'),
            ),
          );

      expect(lease.displayText, 'bold em code\n');
      final span = lease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      final runs = span.children!.cast<TextSpan>().toList();
      expect(runs.map((run) => run.text).join(), 'bold em code\n');
      expect(
        runs.singleWhere((run) => run.text == 'bold').style!.fontWeight,
        FontWeight.w700,
      );
      expect(
        runs.singleWhere((run) => run.text == 'em').style!.fontStyle,
        FontStyle.italic,
      );
      expect(
        runs.singleWhere((run) => run.text == 'code').style!.fontFamily,
        'monospace',
      );

      final insert = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: 'a',
        nextDisplayValue: const TextEditingValue(
          text: 'abold em code\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 4),
        preferredSourceComposing: TextRange.empty,
      );
      expect((insert.sourceStartUtf16, insert.sourceEndUtf16), (4, 4));
      expect(
        source.replaceRange(
          insert.sourceStartUtf16,
          insert.sourceEndUtf16,
          insert.sourceReplacement,
        ),
        '- **abold** *em* `code`\r\n',
      );

      final removePrefix = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'bold em code\n',
          selection: TextSelection.collapsed(offset: 0),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 2),
        preferredSourceComposing: TextRange.empty,
      );
      expect(
        (removePrefix.sourceStartUtf16, removePrefix.sourceEndUtf16),
        (0, 2),
      );
      expect(removePrefix.nextLease.displayText, 'bold em code\n');
      expect(
        removePrefix.nextLease.sourceToDisplayOffset(2),
        0,
        reason: 'the certified inline topology shifted with prefix removal',
      );

      final enter = lease.applyDisplayEdit(
        displayStartUtf16: lease.displayLengthUtf16 - 1,
        displayEndUtf16: lease.displayLengthUtf16 - 1,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'bold em code\n\n',
          selection: TextSelection.collapsed(offset: 13),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 22),
        preferredSourceComposing: TextRange.empty,
      );
      expect(enter.sourceReplacement, '\r\n- ');
      expect(enter.nextLease.displayText, 'bold em code\n\n');
    });

    test('nonempty Enter inserts only the configured canonical sibling', () {
      final edit = _itemLease('- item').applyDisplayEdit(
        displayStartUtf16: 4,
        displayEndUtf16: 4,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n',
          selection: TextSelection.collapsed(offset: 5),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 6),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (6, 6));
      expect(edit.sourceReplacement, '\n- ');
      expect(edit.displayReplacement, '\n');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 9));
      expect(edit.nextLease.displayText, 'item\n');
      expect(edit.nextLease.isCertified, isFalse);
    });

    test(
      'continuation prefix is data and need not match the active marker',
      () {
        final configuration = FlarkV3TightBulletListItemConfiguration(
          activeHiddenSourcePrefix: '* ',
          activeRemovableSourcePrefix: '* ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix: '- ',
          canonicalLineEnding: '\n',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
        );
        final edit = _itemLease('* item', configuration: configuration)
            .applyDisplayEdit(
              displayStartUtf16: 4,
              displayEndUtf16: 4,
              replacement: '\n',
              nextDisplayValue: const TextEditingValue(
                text: 'item\n',
                selection: TextSelection.collapsed(offset: 5),
              ),
              preferredSourceSelection: const TextSelection.collapsed(
                offset: 6,
              ),
              preferredSourceComposing: TextRange.empty,
            );

        expect(edit.sourceReplacement, '\n- ');
      },
    );

    test('generic policy inserts an exact parser-authored ordered prefix', () {
      final configuration = FlarkV3TightListItemConfiguration(
        activeHiddenSourcePrefix: '123456789) ',
        activeRemovableSourcePrefix: '123456789) ',
        activeRemovableSourcePrefixOffsetUtf16: 0,
        continuationSourcePrefix: '123456790) ',
        canonicalLineEnding: '\n',
        emptyEnterExits: true,
        backspaceAtStartRemovesPrefix: true,
        markerPresentation: FlarkV3ListItemMarkerPresentation.parserText(
          parserText: '123456789)',
        ),
      );
      final edit = _itemLease('123456789) item', configuration: configuration)
          .applyDisplayEdit(
            displayStartUtf16: 4,
            displayEndUtf16: 4,
            replacement: '\n',
            nextDisplayValue: const TextEditingValue(
              text: 'item\n',
              selection: TextSelection.collapsed(offset: 5),
            ),
            preferredSourceSelection: const TextSelection.collapsed(offset: 15),
            preferredSourceComposing: TextRange.empty,
          );

      expect(edit.sourceReplacement, '\n123456790) ');
      expect(configuration.markerPresentation.parserText, '123456789)');
      expect(
        configuration.markerPresentation.minimumGutterWidth,
        greaterThanOrEqualTo(88),
      );
    });

    test('empty Enter replaces the exact marker and exits the list', () {
      final edit = _itemLease('- ').applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 2),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (0, 2));
      expect(edit.sourceReplacement, '\n');
      expect(edit.displayReplacement, '\n');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 1));
    });

    test('continued empty item exits without a phantom list marker', () {
      final continued = _itemLease('- item').applyDisplayEdit(
        displayStartUtf16: 4,
        displayEndUtf16: 4,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n',
          selection: TextSelection.collapsed(offset: 5),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 6),
        preferredSourceComposing: TextRange.empty,
      );
      final exited = continued.nextLease.applyDisplayEdit(
        displayStartUtf16: 5,
        displayEndUtf16: 5,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n\n',
          selection: TextSelection.collapsed(offset: 6),
        ),
        preferredSourceSelection: continued.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );

      expect((exited.sourceStartUtf16, exited.sourceEndUtf16), (7, 9));
      expect(exited.sourceReplacement, '\n');
      expect(exited.sourceSelection, const TextSelection.collapsed(offset: 8));

      // The current line is now mechanically unlisted. Before parser
      // recertification, another Enter stays plain instead of resurrecting a
      // marker from stale policy state.
      final plainEnter = exited.nextLease.applyDisplayEdit(
        displayStartUtf16: 6,
        displayEndUtf16: 6,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n\n\n',
          selection: TextSelection.collapsed(offset: 7),
        ),
        preferredSourceSelection: exited.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );
      expect(plainEnter.sourceReplacement, '\n');
    });

    test('Backspace command removes only the exact hidden item prefix', () {
      final edit = _itemLease('- item').applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'item',
          selection: TextSelection.collapsed(offset: 0),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 2),
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (0, 2));
      expect(edit.sourceReplacement, isEmpty);
      expect(edit.nextLease.displayText, 'item');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 0));

      final plainEnter = edit.nextLease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\nitem',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: edit.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );
      expect(plainEnter.sourceReplacement, '\n');
    });

    test('Backspace and empty exit preserve protected BOF source', () {
      final configuration = FlarkV3TightBulletListItemConfiguration(
        activeHiddenSourcePrefix: '\uFEFF- ',
        activeRemovableSourcePrefix: '- ',
        activeRemovableSourcePrefixOffsetUtf16: 1,
        continuationSourcePrefix: '- ',
        canonicalLineEnding: '\n',
        emptyEnterExits: true,
        backspaceAtStartRemovesPrefix: true,
      );
      final backspace =
          _itemLease(
            '\uFEFF- item',
            configuration: configuration,
            hiddenPrefixSplitUtf16: 1,
          ).applyDisplayEdit(
            displayStartUtf16: 0,
            displayEndUtf16: 0,
            replacement: '',
            nextDisplayValue: const TextEditingValue(
              text: 'item',
              selection: TextSelection.collapsed(offset: 0),
            ),
            preferredSourceSelection: const TextSelection.collapsed(offset: 3),
            preferredSourceComposing: TextRange.empty,
          );

      expect((backspace.sourceStartUtf16, backspace.sourceEndUtf16), (1, 3));
      expect(
        backspace.sourceSelection,
        const TextSelection.collapsed(offset: 1),
      );
      final plainEnter = backspace.nextLease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\nitem',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: backspace.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );
      expect(plainEnter.sourceReplacement, '\n');

      final exit = _itemLease('\uFEFF- ', configuration: configuration)
          .applyDisplayEdit(
            displayStartUtf16: 0,
            displayEndUtf16: 0,
            replacement: '\n',
            nextDisplayValue: const TextEditingValue(
              text: '\n',
              selection: TextSelection.collapsed(offset: 1),
            ),
            preferredSourceSelection: const TextSelection.collapsed(offset: 3),
            preferredSourceComposing: TextRange.empty,
          );
      expect((exit.sourceStartUtf16, exit.sourceEndUtf16), (1, 3));
      expect(exit.sourceReplacement, '\n');
      expect(exit.sourceSelection, const TextSelection.collapsed(offset: 2));
    });

    test('removable cut preserves protected source on both sides', () {
      final configuration = FlarkV3TightBulletListItemConfiguration(
        activeHiddenSourcePrefix: '\uFEFF-   ',
        activeRemovableSourcePrefix: '- ',
        activeRemovableSourcePrefixOffsetUtf16: 1,
        continuationSourcePrefix: '- ',
        canonicalLineEnding: '\r\n',
        emptyEnterExits: true,
        backspaceAtStartRemovesPrefix: true,
      );
      final lease = _itemLease(
        '\uFEFF-   ',
        configuration: configuration,
        hiddenPrefixLengthUtf16: 5,
      );

      final backspace = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: '',
          selection: TextSelection.collapsed(offset: 0),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 5),
        preferredSourceComposing: TextRange.empty,
      );
      expect((backspace.sourceStartUtf16, backspace.sourceEndUtf16), (1, 3));
      expect(backspace.sourceReplacement, isEmpty);

      final plainEnter = backspace.nextLease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: backspace.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );
      expect(plainEnter.sourceReplacement, '\n');

      final exit = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 5),
        preferredSourceComposing: TextRange.empty,
      );
      expect((exit.sourceStartUtf16, exit.sourceEndUtf16), (1, 3));
      expect(exit.sourceReplacement, '\r\n');
    });

    test('Backspace at a continued item start does not join visible lines', () {
      final continued = _itemLease('- item').applyDisplayEdit(
        displayStartUtf16: 4,
        displayEndUtf16: 4,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n',
          selection: TextSelection.collapsed(offset: 5),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 6),
        preferredSourceComposing: TextRange.empty,
      );
      final edit = continued.nextLease.applyDisplayEdit(
        displayStartUtf16: 5,
        displayEndUtf16: 5,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'item\n',
          selection: TextSelection.collapsed(offset: 5),
        ),
        preferredSourceSelection: continued.sourceSelection,
        preferredSourceComposing: TextRange.empty,
      );

      expect((edit.sourceStartUtf16, edit.sourceEndUtf16), (7, 9));
      expect(edit.sourceReplacement, isEmpty);
      expect(edit.nextLease.displayText, 'item\n');
    });

    test('CRLF source operations retain one LF in display space', () {
      final configuration = _configuration(canonicalLineEnding: '\r\n');
      final lease = _itemLease('- ', configuration: configuration);
      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 2),
        preferredSourceComposing: TextRange.empty,
      );

      expect(edit.sourceReplacement, '\r\n');
      expect(edit.displayReplacement, '\n');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 2));
    });

    test('Unicode item text retains absolute UTF-16 source coordinates', () {
      // The selected item starts after `π🌍\n`: four UTF-16 code units but
      // seven UTF-8 bytes. The runtime derives this UTF-16 origin from its
      // authenticated byte boundaries before constructing the policy.
      final lease = _itemLease(
        '- 🌍x',
        sourceStartUtf16: 4,
        sourcePrefixUtf8Bytes: 7,
      );
      expect(lease.displayText, '🌍x');

      final enter = lease.applyDisplayEdit(
        displayStartUtf16: 3,
        displayEndUtf16: 3,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: '🌍x\n',
          selection: TextSelection.collapsed(offset: 4),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 9),
        preferredSourceComposing: TextRange.empty,
      );
      expect((enter.sourceStartUtf16, enter.sourceEndUtf16), (9, 9));
      expect(enter.sourceReplacement, '\n- ');
      expect(enter.sourceSelection, const TextSelection.collapsed(offset: 12));

      final backspace = lease.applyDisplayEdit(
        displayStartUtf16: 0,
        displayEndUtf16: 0,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: '🌍x',
          selection: TextSelection.collapsed(offset: 0),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 6),
        preferredSourceComposing: TextRange.empty,
      );
      expect((backspace.sourceStartUtf16, backspace.sourceEndUtf16), (4, 6));
      expect(
        backspace.sourceSelection,
        const TextSelection.collapsed(offset: 4),
      );
    });

    test('withheld local actions stay ordinary and never remove a marker', () {
      final configuration = FlarkV3TightBulletListItemConfiguration(
        activeHiddenSourcePrefix: '- ',
        activeRemovableSourcePrefix: '- ',
        activeRemovableSourcePrefixOffsetUtf16: 0,
        continuationSourcePrefix: '- ',
        canonicalLineEnding: '\n',
        emptyEnterExits: false,
        backspaceAtStartRemovesPrefix: false,
      );
      final backspace = _itemLease('- item', configuration: configuration)
          .applyDisplayEdit(
            displayStartUtf16: 0,
            displayEndUtf16: 0,
            replacement: '',
            nextDisplayValue: const TextEditingValue(
              text: 'item',
              selection: TextSelection.collapsed(offset: 0),
            ),
            preferredSourceSelection: const TextSelection.collapsed(offset: 2),
            preferredSourceComposing: TextRange.empty,
          );
      expect((backspace.sourceStartUtf16, backspace.sourceEndUtf16), (2, 2));

      final enter = _itemLease('- ', configuration: configuration)
          .applyDisplayEdit(
            displayStartUtf16: 0,
            displayEndUtf16: 0,
            replacement: '\n',
            nextDisplayValue: const TextEditingValue(
              text: '\n',
              selection: TextSelection.collapsed(offset: 1),
            ),
            preferredSourceSelection: const TextSelection.collapsed(offset: 2),
            preferredSourceComposing: TextRange.empty,
          );
      expect(enter.sourceReplacement, '\n- ');
    });

    test('mismatched parser configuration fails closed', () {
      final lease = _itemLease('* item', configuration: _configuration());

      expect(
        () => lease.applyDisplayEdit(
          displayStartUtf16: 4,
          displayEndUtf16: 4,
          replacement: '!',
          nextDisplayValue: const TextEditingValue(
            text: 'item!',
            selection: TextSelection.collapsed(offset: 5),
          ),
          preferredSourceSelection: const TextSelection.collapsed(offset: 6),
          preferredSourceComposing: TextRange.empty,
        ),
        throwsStateError,
      );
    });

    test('configuration is explicit, comparable, and bounded', () {
      expect(_configuration(), _configuration());
      expect(
        () => FlarkV3TightBulletListItemConfiguration(
          activeHiddenSourcePrefix: '',
          activeRemovableSourcePrefix: '- ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix: '- ',
          canonicalLineEnding: '\n',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3TightBulletListItemConfiguration(
          activeHiddenSourcePrefix: '- ',
          activeRemovableSourcePrefix: '- ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix:
              'x' *
              (FlarkV3TightBulletListItemConfiguration
                      .maximumSourcePrefixUtf16 +
                  1),
          canonicalLineEnding: '\n',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3TightBulletListItemConfiguration(
          activeHiddenSourcePrefix: '- ',
          activeRemovableSourcePrefix: '- ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix: '- ',
          canonicalLineEnding: '\n\r',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3TightBulletListItemConfiguration(
          activeHiddenSourcePrefix: '* ',
          activeRemovableSourcePrefix: '- ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix: '- ',
          canonicalLineEnding: '\n',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3ListItemMarkerPresentation.parserText(parserText: ''),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3ListItemMarkerPresentation.parserText(
          parserText:
              'x' *
              (FlarkV3ListItemMarkerPresentation.maximumParserTextUtf16 + 1),
        ),
        throwsArgumentError,
      );
    });
  });

  testWidgets(
    'generic list gutter preserves bullet keys and one EditableText client',
    (tester) async {
      final controller = TextEditingController(text: 'item');
      final focusNode = FocusNode();
      final editableKey = GlobalKey();
      addTearDown(controller.dispose);
      addTearDown(focusNode.dispose);
      late StateSetter setState;
      var markerColor = const Color(0xFF64748B);
      FlarkV3TightListItemConfiguration? configuration = _configuration();

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: StatefulBuilder(
            builder: (context, update) {
              setState = update;
              return SizedBox(
                width: 260,
                child: FlarkV3ListItemGutter(
                  configuration: configuration,
                  markerColor: markerColor,
                  child: EditableText(
                    key: editableKey,
                    controller: controller,
                    focusNode: focusNode,
                    style: const TextStyle(fontSize: 14),
                    cursorColor: const Color(0xFF006ADC),
                    backgroundCursorColor: const Color(0x00000000),
                  ),
                ),
              );
            },
          ),
        ),
      );

      expect(find.byType(EditableText), findsOneWidget);
      expect(
        find.byKey(const Key('flark-v3-list-item-gutter')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('flark-v3-list-item-marker')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('flark-v3-bullet-list-item-marker')),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byKey(const Key('flark-v3-bullet-list-item-marker')),
          matching: find.byType(CustomPaint),
        ),
        findsOneWidget,
      );
      final editableState = tester.state<EditableTextState>(
        find.byType(EditableText),
      );
      final markerRect = tester.getRect(
        find.byKey(const Key('flark-v3-bullet-list-item-marker')),
      );
      final editableRect = tester.getRect(find.byType(EditableText));
      expect(markerRect.right, lessThan(editableRect.left));

      setState(() => markerColor = const Color(0xFF0F172A));
      await tester.pump();

      expect(find.byType(EditableText), findsOneWidget);
      expect(
        identical(
          tester.state<EditableTextState>(find.byType(EditableText)),
          editableState,
        ),
        isTrue,
      );
      expect(controller.text, 'item');

      setState(
        () => configuration = FlarkV3TightListItemConfiguration(
          activeHiddenSourcePrefix: '123456789) ',
          activeRemovableSourcePrefix: '123456789) ',
          activeRemovableSourcePrefixOffsetUtf16: 0,
          continuationSourcePrefix: '123456790) ',
          canonicalLineEnding: '\n',
          emptyEnterExits: true,
          backspaceAtStartRemovesPrefix: true,
          markerPresentation: FlarkV3ListItemMarkerPresentation.parserText(
            parserText: '123456789)',
          ),
        ),
      );
      await tester.pump();

      expect(find.byType(EditableText), findsOneWidget);
      expect(
        identical(
          tester.state<EditableTextState>(find.byType(EditableText)),
          editableState,
        ),
        isTrue,
      );
      expect(
        find.byKey(const Key('flark-v3-list-item-gutter')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('flark-v3-ordered-list-item-gutter')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('flark-v3-list-item-marker')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('flark-v3-ordered-list-item-marker')),
        findsOneWidget,
      );
      expect(find.text('123456789)', findRichText: true), findsOneWidget);
      expect(
        tester
            .getSize(find.byKey(const Key('flark-v3-ordered-list-item-gutter')))
            .width,
        greaterThanOrEqualTo(88),
      );
      expect(
        find.byKey(const Key('flark-v3-bullet-list-item-marker')),
        findsNothing,
      );
      expect(controller.text, 'item');
    },
  );
}

FlarkV3TightBulletListItemConfiguration _configuration({
  String canonicalLineEnding = '\n',
}) => FlarkV3TightBulletListItemConfiguration(
  activeHiddenSourcePrefix: '- ',
  activeRemovableSourcePrefix: '- ',
  activeRemovableSourcePrefixOffsetUtf16: 0,
  continuationSourcePrefix: '- ',
  canonicalLineEnding: canonicalLineEnding,
  emptyEnterExits: true,
  backspaceAtStartRemovesPrefix: true,
);

FlarkV3ProjectedInputLease _itemLease(
  String source, {
  FlarkV3TightBulletListItemConfiguration? configuration,
  int sourceStartUtf16 = 0,
  int sourcePrefixUtf8Bytes = 0,
  int? hiddenPrefixSplitUtf16,
  int? hiddenPrefixLengthUtf16,
}) {
  final prefixLength = hiddenPrefixLengthUtf16 ?? source.indexOf(' ') + 1;
  final split = hiddenPrefixSplitUtf16;
  if (split != null && (split <= 0 || split >= prefixLength)) {
    throw RangeError.range(
      split,
      1,
      prefixLength - 1,
      'hiddenPrefixSplitUtf16',
    );
  }
  return FlarkV3ProjectedInputLease.fromSourceProjection(
    FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: sourceStartUtf16,
      sourceText: source,
      pieces: [
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: sourceStartUtf16,
          sourceEndUtf16: sourceStartUtf16 + (split ?? prefixLength),
        ),
        if (split != null)
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: sourceStartUtf16 + split,
            sourceEndUtf16: sourceStartUtf16 + prefixLength,
          ),
        if (prefixLength < source.length)
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: sourceStartUtf16 + prefixLength,
            sourceEndUtf16: sourceStartUtf16 + source.length,
          ),
      ],
      certifiedSourceVersion: FlarkV3SourceVersion(
        documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
        revision: 9,
        metric: FlarkV3SourceMetric(
          bytes: sourcePrefixUtf8Bytes + utf8.encode(source).length,
          utf16: sourceStartUtf16 + source.length,
        ),
        contentHash: const FlarkV3ContentHash128(5, 6, 7, 8),
      ),
    ),
    editPolicy: FlarkV3TightBulletListItemEditPolicy(
      configuration: configuration ?? _configuration(),
    ),
  );
}

FlarkV3AuthoritativeInlineIslandPresentation _authoritativeInlinePresentation({
  required FlarkV3SourceDocument document,
  required FlarkV3SourceVersion version,
  required FlarkV3SourceSpan content,
  required FlarkV3InlineFacts facts,
}) {
  final presentation = FlarkV3InlineIslandPresentation.resolve(
    sourceDocument: document,
    expectedSource: version,
    structuralQuery: FlarkV3DocumentStructuralQuery(
      sourceRevision: version.revision,
      structureRevision: version.revision,
      structure: FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: content,
        visibleSource: content,
        referenceDefinitionCount: 0,
      ),
      projection: FlarkV3DocumentProjection(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: content,
        projectedSource: content,
        runCount: 1,
      ),
      inlineFacts: facts,
    ),
    activeIsland: content,
  );
  return presentation as FlarkV3AuthoritativeInlineIslandPresentation;
}

List<int> _inlineRecord({
  required int kind,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data.setUint8(0, kind);
  data.setUint32(4, start, Endian.little);
  data.setUint32(8, length, Endian.little);
  data.setUint32(12, contentStart, Endian.little);
  data.setUint32(16, contentLength, Endian.little);
  return bytes;
}
