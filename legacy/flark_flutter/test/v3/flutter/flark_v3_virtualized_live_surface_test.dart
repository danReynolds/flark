import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_flutter_live_controller.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_block_chrome.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_inline_image.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_virtualized_live_surface.dart';
import 'package:flark_flutter/src/v3/flutter/flark_v3_visible_block_coordinator.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'passive blocks retain exact paint while actions fail closed on gap',
    (tester) async {
      final fixture = _SurfaceFixture.mixed();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Tail is active.');
      final focusNode = FocusNode();
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      expect(find.byType(EditableText), findsOneWidget);
      final paragraph = _passiveSpan(tester, 0);
      final heading = _passiveSpan(tester, 1);
      final fence = _passiveSpan(tester, 2);

      expect(paragraph.toPlainText(), 'Alpha bold and emphasis with code.');
      expect(heading.toPlainText(), 'Heading strong');
      expect(fence.toPlainText(), 'final value = 1;\n');
      final allPassive =
          '${paragraph.toPlainText()}${heading.toPlainText()}'
          '${fence.toPlainText()}';
      expect(allPassive, isNot(contains('**')));
      expect(allPassive, isNot(contains('_emphasis_')));
      expect(allPassive, isNot(contains('`code`')));
      expect(allPassive, isNot(contains('```')));

      expect(_styleForText(paragraph, 'bold').fontWeight, FontWeight.w700);
      expect(_styleForText(paragraph, 'emphasis').fontStyle, FontStyle.italic);
      expect(_styleForText(paragraph, 'code').fontFamily, 'monospace');
      expect(
        (heading.children!.first as TextSpan).style!.fontSize,
        greaterThan(16),
      );
      expect(
        (fence.children!.first as TextSpan).style!.fontFamily,
        'monospace',
      );

      final editableState = tester.state<EditableTextState>(
        find.byType(EditableText),
      );
      final passiveRenderObjects = [
        for (final ordinal in const [0, 1, 2])
          tester.renderObject(find.byKey(_passiveTextKey(ordinal))),
      ];
      fixture.source.enterGap('viewport query bound');
      await tester.pump();

      for (var ordinal = 0; ordinal < 3; ordinal += 1) {
        final passive = find.byKey(_passiveTextKey(ordinal));
        expect(passive, findsOneWidget);
        expect(
          tester.renderObject(passive),
          same(passiveRenderObjects[ordinal]),
        );
      }
      final activeBeforeTap = fixture.source.snapshot.activeOrdinal;
      await tester.tap(find.byKey(_passiveTextKey(0)), warnIfMissed: false);
      expect(
        fixture.source.snapshot.activeOrdinal,
        activeBeforeTap,
        reason: 'retained paint must not retain stale row actions',
      );
      expect(
        tester.state<EditableTextState>(find.byType(EditableText)),
        same(editableState),
        reason: 'source-gap fallback must not remount the active input client',
      );
    },
  );

  testWidgets(
    'passive links own link taps and semantics while other taps activate rows',
    (tester) async {
      final fixture = _SurfaceFixture.links();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Active editor');
      final focusNode = FocusNode();
      final scrollController = ScrollController();
      final activations = <FlarkV3InlineLinkAnnotation>[];
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      addTearDown(scrollController.dispose);
      final semantics = tester.ensureSemantics();

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          scrollController: scrollController,
          onLinkActivated: activations.add,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      const uri = 'https://example.com';
      const email = 'dev@example.com';
      final passive = _passiveSpan(tester, 0);
      expect(passive.toPlainText(), 'before $uri and $email after');
      for (final target in const [uri, email]) {
        expect(
          _styleForText(
            passive,
            target,
          ).decoration!.contains(TextDecoration.underline),
          isTrue,
        );
      }
      final uriSemantics = find.semantics.byLabel(uri);
      expect(uriSemantics, findsOne);
      expect(
        uriSemantics.evaluate().single,
        isSemantics(
          identifier: 'flark-v3-passive-link-0-7',
          label: uri,
          isLink: true,
          hasTapAction: true,
        ),
      );
      final visibleActivationOffset = scrollController.offset;

      await tester.tapAt(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: 7,
          endUtf16: 7 + uri.length,
        ),
      );
      await tester.pump();

      expect(activations, hasLength(1));
      expect(activations.single.kind, FlarkV3InlineLinkKind.uri);
      expect(
        activations.single.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.exactContent,
      );
      expect(activations.single.destination, uri);
      expect(fixture.source.snapshot.activeOrdinal, 1);

      final emailStart = 7 + uri.length + 5;
      await tester.tapAt(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: emailStart,
          endUtf16: emailStart + email.length,
        ),
      );
      await tester.pump();

      expect(activations, hasLength(2));
      expect(activations.last.kind, FlarkV3InlineLinkKind.email);
      expect(
        activations.last.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
      );
      expect(activations.last.destination, 'mailto:$email');
      expect(fixture.source.snapshot.activeOrdinal, 1);

      expect(focusNode.hasFocus, isFalse);
      await tester.tapAt(
        _passiveTextPosition(tester, ordinal: 0, startUtf16: 0, endUtf16: 6),
      );
      expect(
        focusNode.hasFocus,
        isFalse,
        reason:
            'web focus must wait until the stable input has followed the '
            'new active row',
      );
      await tester.pump();
      expect(
        focusNode.hasFocus,
        isFalse,
        reason:
            'the active row must composite at its new position before the '
            'web text client reveals its caret',
      );
      await tester.pump();
      expect(focusNode.hasFocus, isTrue);
      expect(
        scrollController.offset,
        visibleActivationOffset,
        reason: 'activating an already-visible row must not jump the viewport',
      );

      expect(fixture.source.snapshot.activeOrdinal, 0);
      expect(activations, hasLength(2));
      await tester.tap(find.byType(EditableText));
      await tester.pump();
      expect(
        activations,
        hasLength(2),
        reason: 'active editing never exposes a passive activation target',
      );
      semantics.dispose();
    },
  );

  testWidgets(
    'passive link activation fails closed when authority changes mid-gesture',
    (tester) async {
      final fixture = _SurfaceFixture.links();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Active editor');
      final focusNode = FocusNode();
      final activations = <FlarkV3InlineLinkAnnotation>[];
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          onLinkActivated: activations.add,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      const uri = 'https://example.com';
      final gesture = await tester.startGesture(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: 7,
          endUtf16: 7 + uri.length,
        ),
      );
      fixture.source.enterGap('source advanced during pointer sequence');
      await tester.pump();
      await gesture.up();
      await tester.pump();

      expect(activations, isEmpty);
      expect(
        fixture.source.snapshot,
        isA<FlarkV3SourceGapViewportSurfaceSnapshot>(),
      );
    },
  );

  testWidgets(
    'passive link without an app callback retains ordinary row activation',
    (tester) async {
      final fixture = _SurfaceFixture.links();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Active editor');
      final focusNode = FocusNode();
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      const uri = 'https://example.com';
      await tester.tapAt(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: 7,
          endUtf16: 7 + uri.length,
        ),
      );
      await tester.pump();
      await tester.pump();

      expect(fixture.source.snapshot.activeOrdinal, 0);
      expect(focusNode.hasFocus, isTrue);
    },
  );

  testWidgets(
    'passive direct links and images keep distinct parser-authored behavior',
    (tester) async {
      final fixture = _SurfaceFixture.directMedia();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Active editor');
      final focusNode = FocusNode();
      final activations = <FlarkV3InlineLinkAnnotation>[];
      final imageSpecs = <String, FlarkV3InlineImageSpec>{};
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      final semantics = tester.ensureSemantics();

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          onLinkActivated: activations.add,
          inlineImageBuilder: (context, spec) {
            imageSpecs[spec.destination] = spec;
            return SizedBox(
              key: ValueKey<Object>((
                'test-v3-image',
                spec.destination,
                spec.alt,
              )),
              width: 80,
              height: 24,
            );
          },
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      final directLink = _passiveSpan(tester, 0);
      expect(directLink.toPlainText(), 'label');
      expect(directLink.toPlainText(), isNot(contains('[label]')));
      expect(
        _styleForText(
          directLink,
          'label',
        ).decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      await tester.tapAt(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: 0,
          endUtf16: 'label'.length,
        ),
      );
      await tester.pump();
      expect(activations, hasLength(1));
      expect(activations.single.kind, FlarkV3InlineLinkKind.direct);
      expect(activations.single.destination, 'dest');
      expect(activations.single.title, 'title');

      expect(imageSpecs.keys.toSet(), {'image-only', 'hero.png', 'empty.png'});
      final unlinked = imageSpecs['image-only']!;
      expect(unlinked.alt, 'inside');
      expect(unlinked.outerLink, isNull);
      final linked = imageSpecs['hero.png']!;
      expect(linked.alt, 'hero');
      expect(linked.outerLink?.destination, 'outer');
      final empty = imageSpecs['empty.png']!;
      expect(empty.alt, isEmpty);
      expect(empty.outerLink, isNull);

      final unlinkedSemantics = find.semantics.byLabel('inside');
      expect(
        unlinkedSemantics,
        findsOne,
        reason: 'the inner link label becomes image alt, not a link action',
      );
      expect(
        unlinkedSemantics.evaluate().single,
        isSemantics(
          identifier: 'flark-v3-passive-image-1-0',
          label: 'inside',
          value: 'image-only',
          isImage: true,
        ),
      );
      expect(
        find.semantics.byLabel('hero').evaluate().single,
        isSemantics(
          identifier: 'flark-v3-passive-image-2-1',
          label: 'hero',
          value: 'hero.png',
          isImage: true,
          isLink: true,
          hasTapAction: true,
        ),
      );
      expect(
        find.semantics.byLabel('Image').evaluate().single,
        isSemantics(
          identifier: 'flark-v3-passive-image-3-0',
          label: 'Image',
          value: 'empty.png',
          isImage: true,
        ),
      );

      await tester.tapAt(
        tester.getCenter(
          find.byKey(
            const ValueKey<Object>(('test-v3-image', 'hero.png', 'hero')),
          ),
        ),
      );
      await tester.pump();
      expect(activations, hasLength(2));
      expect(
        activations.last.destination,
        'outer',
        reason: 'the surrounding link, never the image URL, owns activation',
      );
      expect(fixture.source.snapshot.activeOrdinal, 4);

      await tester.tapAt(
        tester.getCenter(
          find.byKey(
            const ValueKey<Object>(('test-v3-image', 'image-only', 'inside')),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(
        activations,
        hasLength(2),
        reason: 'a link nested in image alt never leaks an action',
      );
      expect(fixture.source.snapshot.activeOrdinal, 1);
      semantics.dispose();
    },
  );

  testWidgets(
    'passive reference links and images retain resolved semantics marker-free',
    (tester) async {
      final fixture = _SurfaceFixture.referenceMedia();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Active editor');
      final focusNode = FocusNode();
      final activations = <FlarkV3InlineLinkAnnotation>[];
      final imageSpecs = <String, FlarkV3InlineImageSpec>{};
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          onLinkActivated: activations.add,
          inlineImageBuilder: (context, spec) {
            imageSpecs[spec.destination] = spec;
            return SizedBox(
              key: ValueKey<Object>((
                'test-v3-reference-image',
                spec.destination,
                spec.alt,
              )),
              width: 80,
              height: 24,
            );
          },
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      final referenceLink = _passiveSpan(tester, 0);
      expect(referenceLink.toPlainText(), 'label');
      expect(referenceLink.toPlainText(), isNot(contains('[id]')));
      expect(
        _styleForText(
          referenceLink,
          'label',
        ).decoration!.contains(TextDecoration.underline),
        isTrue,
      );
      await tester.tapAt(
        _passiveTextPosition(
          tester,
          ordinal: 0,
          startUtf16: 0,
          endUtf16: 'label'.length,
        ),
      );
      await tester.pump();
      expect(activations, hasLength(1));
      final referenceActivation = activations.single;
      expect(referenceActivation.kind, FlarkV3InlineLinkKind.reference);
      expect(referenceActivation.destination, '/resolved');
      expect(referenceActivation.title, 'ref title');
      expect(
        referenceActivation.destinationSource.startUtf16,
        greaterThan('[label][id]'.length),
      );
      expect(
        referenceActivation.titleSource!.startUtf16,
        greaterThan(referenceActivation.destinationSource.endUtf16),
      );
      expect(
        fixture.source.snapshot.activeOrdinal,
        3,
        reason: 'the reference-link recognizer owns the tap',
      );

      expect(imageSpecs.keys.toSet(), {'/image-only', '/hero.png'});
      final unlinked = imageSpecs['/image-only']!;
      expect(unlinked.alt, 'inside');
      expect(unlinked.annotation.title, 'image title');
      expect(unlinked.outerLink, isNull);
      expect(
        unlinked.annotation.destinationSource.startUtf16,
        greaterThan('![[inside][inner]][image]'.length),
      );

      final linked = imageSpecs['/hero.png']!;
      expect(linked.alt, 'hero');
      expect(linked.annotation.title, 'hero title');
      expect(linked.outerLink?.kind, FlarkV3InlineLinkKind.reference);
      expect(linked.outerLink?.destination, '/outer');
      expect(linked.outerLink?.title, 'outer title');

      await tester.tapAt(
        tester.getCenter(
          find.byKey(
            const ValueKey<Object>((
              'test-v3-reference-image',
              '/hero.png',
              'hero',
            )),
          ),
        ),
      );
      await tester.pump();
      expect(activations, hasLength(2));
      expect(activations.last.kind, FlarkV3InlineLinkKind.reference);
      expect(activations.last.destination, '/outer');
      expect(
        fixture.source.snapshot.activeOrdinal,
        3,
        reason: 'the enclosing reference link, not the image URL, owns tap',
      );

      await tester.tapAt(
        tester.getCenter(
          find.byKey(
            const ValueKey<Object>((
              'test-v3-reference-image',
              '/image-only',
              'inside',
            )),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(
        activations,
        hasLength(2),
        reason: 'a reference link nested in image alt never leaks an action',
      );
      expect(fixture.source.snapshot.activeOrdinal, 1);
    },
  );

  testWidgets('default passive image fallback performs no implicit fetch', (
    tester,
  ) async {
    const destination = 'https://example.test/image.png';
    const source = '![]($destination)';
    final projection = _directProjection(
      source,
      records: [
        _inlineRecord(
          kind: 11,
          start: 0,
          length: source.length,
          contentStart: 2,
          contentLength: 0,
        ),
      ],
      entries: const [
        _InlineValueEntry(
          parentFactOrdinal: 0,
          destinationStart: 4,
          destinationLength: destination.length,
          cookedDestination: destination,
        ),
      ],
    );
    final annotation = projection.imageAnnotations.single;
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkV3InlineImage(
          spec: FlarkV3InlineImageSpec(
            annotation: annotation,
            alt: '',
            outerLink: null,
            constraints: FlarkV3InlineImage.inlineConstraints,
          ),
        ),
      ),
    );

    expect(find.byType(Image), findsNothing);
    expect(
      find.byKey(const ValueKey<String>('flark-v3-inline-image-fallback')),
      findsOne,
    );
    expect(find.text(destination), findsOne);
  });

  testWidgets(
    'passive activation keeps text aligned, focuses, and reuses one client',
    (tester) async {
      final fixture = _SurfaceFixture.mixed();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      final editingController = TextEditingController(text: 'Tail is active.');
      final focusNode = FocusNode();
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          horizontalPadding: 28,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await tester.pump();

      final editableFinder = find.byType(EditableText);
      final editableState = tester.state<EditableTextState>(editableFinder);
      final passiveX = tester.getTopLeft(find.byKey(_passiveTextKey(0))).dx;

      await tester.tap(find.byKey(_passiveTextKey(0)));
      await tester.pump();
      await tester.pump();

      expect(fixture.source.snapshot.activeOrdinal, 0);
      expect(
        tester.state<EditableTextState>(editableFinder),
        same(editableState),
      );
      expect(tester.getTopLeft(editableFinder).dx, closeTo(passiveX, 0.01));
      expect(focusNode.hasFocus, isTrue);
      final setClientCalls = _setClientCalls(tester);
      expect(setClientCalls, hasLength(1));
      final clientId =
          (setClientCalls.single.arguments as List<dynamic>).first as int;

      tester.testTextInput.enterText('typed after tap');
      await tester.pump();

      expect(editingController.text, 'typed after tap');
      expect(_setClientCalls(tester), hasLength(1));
      expect(
        (_setClientCalls(tester).single.arguments as List<dynamic>).first,
        clientId,
      );
    },
  );

  testWidgets(
    'activation retains passive pixels until exact active presentation swaps',
    (tester) async {
      final fixture = _SurfaceFixture.mixed();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();
      final readiness = _FakeActivePresentationReadiness();
      final editingController = TextEditingController(text: 'Tail is active.');
      final focusNode = FocusNode();
      final scrollController = ScrollController();
      addTearDown(readiness.dispose);
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      addTearDown(scrollController.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          scrollController: scrollController,
          horizontalPadding: 28,
          activePresentationProgress: readiness,
          activePresentationReadiness: readiness.call,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16, height: 1.35),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
            maxLines: null,
          ),
        ),
      );
      await tester.pump();

      final target =
          (fixture.source.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
              .blocks
              .singleWhere((block) => block.ordinal == 0);
      final passiveText = find.byKey(_passiveTextKey(target.ordinal));
      final editable = find.byType(EditableText);
      final editableState = tester.state<EditableTextState>(editable);
      final passiveRender = tester.renderObject<RenderParagraph>(passiveText);
      final passiveRect = tester.getRect(passiveText);
      final initialScrollOffset = scrollController.offset;

      await tester.tap(passiveText);
      editingController.value = const TextEditingValue(
        text: '**raw activation source**',
        selection: TextSelection.collapsed(offset: 25),
      );

      await tester.pump();
      expect(
        tester
            .widget<Opacity>(
              find.byKey(const Key('flark-v3-active-visual-gate')),
            )
            .opacity,
        0,
      );
      expect(passiveText, findsOneWidget);
      expect(
        tester.renderObject<RenderParagraph>(passiveText),
        same(passiveRender),
      );
      expect(tester.getRect(passiveText), passiveRect);
      expect(
        _passiveSpan(tester, target.ordinal).toPlainText(),
        target.displayText,
      );
      expect(
        _passiveSpan(tester, target.ordinal).toPlainText(),
        isNot(contains('**')),
      );
      expect(scrollController.offset, initialScrollOffset);
      expect(focusNode.hasFocus, isFalse);
      expect(tester.state<EditableTextState>(editable), same(editableState));

      await tester.pump();
      expect(passiveText, findsOneWidget);
      expect(tester.getRect(passiveText), passiveRect);
      expect(scrollController.offset, initialScrollOffset);
      expect(focusNode.hasFocus, isFalse);

      editingController.value = TextEditingValue(
        text: target.displayText,
        selection: TextSelection.collapsed(offset: target.displayText.length),
      );
      readiness.complete();
      await tester.pump();

      expect(
        tester
            .widget<Opacity>(
              find.byKey(const Key('flark-v3-active-visual-gate')),
            )
            .opacity,
        1,
      );
      expect(passiveText, findsNothing);
      expect(tester.state<EditableTextState>(editable), same(editableState));
      final activeTopLeft = tester.getTopLeft(editable);
      expect(activeTopLeft.dx, closeTo(passiveRect.left, 0.01));
      expect(activeTopLeft.dy, closeTo(passiveRect.top, 0.01));
      expect(
        tester.getSize(editable).height,
        closeTo(passiveRect.height, 0.01),
      );
      expect(scrollController.offset, initialScrollOffset);
      expect(focusNode.hasFocus, isFalse);

      await tester.pump();
      expect(focusNode.hasFocus, isTrue);
      final clientCalls = _setClientCalls(tester);
      expect(clientCalls, hasLength(1));
      final clientId =
          (clientCalls.single.arguments as List<dynamic>).first as int;
      final renderEditable = tester.renderObject(editable);
      final activeRect = tester.getRect(editable);
      final activeStyle = tester.widget<EditableText>(editable).style;

      editingController.value = TextEditingValue(
        text: '${target.displayText}!',
        selection: TextSelection.collapsed(
          offset: target.displayText.length + 1,
        ),
      );
      fixture.source.enterGap('typing recertification');
      await tester.pump();

      expect(
        tester
            .widget<Opacity>(
              find.byKey(const Key('flark-v3-active-visual-gate')),
            )
            .opacity,
        1,
        reason: 'ordinary typing gaps must not restart activation staging',
      );
      expect(tester.renderObject(editable), same(renderEditable));
      expect(tester.widget<EditableText>(editable).style, activeStyle);
      expect(tester.getRect(editable), activeRect);
      expect(_setClientCalls(tester), hasLength(1));
      expect(
        (_setClientCalls(tester).single.arguments as List<dynamic>).first,
        clientId,
      );
    },
  );

  testWidgets(
    'fenced code activation shares passive chrome geometry before atomic swap',
    (tester) async {
      final fixture = _SurfaceFixture.mixed();
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();
      final readiness = _FakeActivePresentationReadiness();
      final editingController = TextEditingController(text: 'Tail is active.');
      final focusNode = FocusNode();
      final scrollController = ScrollController();
      addTearDown(readiness.dispose);
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);
      addTearDown(scrollController.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          focusNode: focusNode,
          scrollController: scrollController,
          activePresentationProgress: readiness,
          activePresentationReadiness: readiness.call,
          activeEditorBuilder: (_) => FlarkV3CodeBlockChrome(
            key: const Key('test-active-code-block-chrome'),
            active: true,
            child: EditableText(
              controller: editingController,
              focusNode: focusNode,
              style: const TextStyle(
                fontSize: 16,
                height: 1.35,
                fontFamily: 'monospace',
              ),
              cursorColor: const Color(0xFF006ADC),
              backgroundCursorColor: const Color(0x00000000),
              maxLines: null,
            ),
          ),
        ),
      );
      await tester.pump();

      final target =
          (fixture.source.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
              .blocks
              .singleWhere((block) => block.ordinal == 2);
      final passiveText = find.byKey(_passiveTextKey(target.ordinal));
      final passiveChrome = find.byKey(
        ValueKey<Object>((
          'flark-v3-passive-code-block-chrome',
          target.ordinal,
        )),
      );
      final passiveChromeRect = tester.getRect(passiveChrome);
      final passiveRender = tester.renderObject<RenderParagraph>(passiveText);
      final editable = find.byType(EditableText);
      final editableState = tester.state<EditableTextState>(editable);
      final initialScrollOffset = scrollController.offset;

      await tester.tap(passiveText);
      editingController.value = const TextEditingValue(
        text: '```dart\nraw\n```\n',
        selection: TextSelection.collapsed(offset: 12),
      );
      await tester.pump();
      await tester.pump();

      expect(passiveChrome, findsOneWidget);
      expect(tester.getRect(passiveChrome), passiveChromeRect);
      expect(
        tester.renderObject<RenderParagraph>(passiveText),
        same(passiveRender),
      );
      expect(
        tester
            .widget<Opacity>(
              find.byKey(const Key('flark-v3-active-visual-gate')),
            )
            .opacity,
        0,
      );
      expect(
        _passiveSpan(tester, target.ordinal).toPlainText(),
        target.displayText,
      );
      expect(
        _passiveSpan(tester, target.ordinal).toPlainText(),
        isNot(contains('```')),
      );
      expect(scrollController.offset, initialScrollOffset);
      expect(focusNode.hasFocus, isFalse);

      editingController.value = TextEditingValue(
        text: target.displayText,
        selection: TextSelection.collapsed(offset: target.displayText.length),
      );
      readiness.complete();
      await tester.pump();

      final activeChrome = find.byKey(
        const Key('test-active-code-block-chrome'),
      );
      expect(passiveChrome, findsNothing);
      expect(activeChrome, findsOneWidget);
      expect(tester.getRect(activeChrome), passiveChromeRect);
      final decoration = tester.widget<DecoratedBox>(
        find
            .descendant(of: activeChrome, matching: find.byType(DecoratedBox))
            .first,
      );
      expect(
        (decoration.decoration as BoxDecoration).color,
        flarkV3CodeBlockBackground,
      );
      expect(tester.state<EditableTextState>(editable), same(editableState));
      expect(scrollController.offset, initialScrollOffset);

      await tester.pump();
      expect(focusNode.hasFocus, isTrue);
      expect(_setClientCalls(tester), hasLength(1));
    },
  );

  testWidgets('blank-boundary passive records add no second visual gap', (
    tester,
  ) async {
    final fixture = _SurfaceFixture.mixed();
    addTearDown(fixture.close);
    fixture.materializeCurrentWindow();
    fixture.flushCoordinator();
    final editingController = TextEditingController(text: 'Tail is active.');
    final focusNode = FocusNode();
    addTearDown(editingController.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      _surfaceApp(
        fixture,
        focusNode: focusNode,
        activeEditorBuilder: (_) => EditableText(
          controller: editingController,
          focusNode: focusNode,
          style: const TextStyle(fontSize: 16),
          cursorColor: const Color(0xFF006ADC),
          backgroundCursorColor: const Color(0x00000000),
        ),
      ),
    );
    await tester.pump();

    expect(
      tester
          .getSize(
            find.byKey(const ValueKey<Object>(('flark-v3-blank-boundary', 4))),
          )
          .height,
      0,
    );
    expect(
      tester
          .getSize(find.byKey(ValueKey<Object>((fixture.identity, 4))))
          .height,
      0,
    );
  });

  testWidgets(
    '4,096 blocks keep bounded passive mounts and one stable input client',
    (tester) async {
      final fixture = _SurfaceFixture.large(
        blockCount: 4096,
        initialActiveOrdinal: 2048,
      );
      addTearDown(fixture.close);
      fixture.materializeCurrentWindow();
      fixture.flushCoordinator();

      var createdControllers = 0;
      final editingController = TextEditingController(text: 'active 2048');
      createdControllers += 1;
      final focusNode = FocusNode();
      addTearDown(editingController.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        _surfaceApp(
          fixture,
          activeEditorBuilder: (_) => EditableText(
            controller: editingController,
            focusNode: focusNode,
            style: const TextStyle(fontSize: 16),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
          ),
        ),
      );
      await _pumpSurface(tester, fixture);

      final editableFinder = find.byType(EditableText);
      expect(editableFinder, findsOneWidget);
      expect(createdControllers, 1);
      expect(
        fixture.surfaceController.mountedPresentationCount,
        lessThanOrEqualTo(flarkV3MaximumMountedViewportPresentations),
      );
      final editableState = tester.state<EditableTextState>(editableFinder);
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = _setClientCalls(tester);
      final clientId =
          (setClientCallsBefore.last.arguments as List<dynamic>).first as int;

      final unrelatedOrdinal = fixture.source.snapshot.activeOrdinal - 2;
      final unrelatedBuilds = fixture.surfaceController.passiveBuildCount(
        unrelatedOrdinal,
      );
      expect(unrelatedBuilds, greaterThan(0));

      fixture.source.activateOrdinal(2049);
      await tester.pump();
      expect(
        tester.state<EditableTextState>(editableFinder),
        same(editableState),
      );
      expect(
        fixture.surfaceController.passiveBuildCount(unrelatedOrdinal),
        unrelatedBuilds,
        reason: 'an unchanged passive presentation keeps its widget instance',
      );

      fixture.surfaceController.revealAndActivateOrdinal(4095);
      fixture.flushCoordinator();
      await _pumpSurface(tester, fixture);

      expect(fixture.source.snapshot.activeOrdinal, 4095);
      expect(find.byType(EditableText), findsOneWidget);
      expect(
        tester.state<EditableTextState>(editableFinder),
        same(editableState),
      );
      expect(createdControllers, 1);
      expect(
        fixture.surfaceController.mountedPresentationCount,
        lessThanOrEqualTo(flarkV3MaximumMountedViewportPresentations),
      );
      final setClientCallsAfter = _setClientCalls(tester);
      expect(setClientCallsAfter, hasLength(setClientCallsBefore.length));
      expect(
        (setClientCallsAfter.last.arguments as List<dynamic>).first,
        clientId,
      );
    },
  );

  test('one Flutter viewport page rejects eager 97-block materialization', () {
    final identity = _identity(sourceLength: 1000);
    expect(
      () => FlarkV3ExactViewportSurfaceSnapshot(
        totalBlockCount: 100,
        activeOrdinal: 0,
        estimatedBlockExtent: 44,
        identity: identity,
        blocks: List.generate(
          flarkV3MaximumMountedViewportPresentations + 1,
          (ordinal) =>
              _paragraphPresentation(identity, ordinal, sourceUnit: 10),
        ),
      ),
      throwsRangeError,
    );
  });
}

Future<void> _pumpSurface(WidgetTester tester, _SurfaceFixture fixture) async {
  for (var turn = 0; turn < 8; turn += 1) {
    fixture.flushCoordinator();
    await tester.pump();
    if (!fixture.scheduler.hasPending) break;
  }
  await tester.pump();
}

Widget _surfaceApp(
  _SurfaceFixture fixture, {
  required FlarkV3ActiveEditorBuilder activeEditorBuilder,
  FocusNode? focusNode,
  ScrollController? scrollController,
  double horizontalPadding = 16,
  ValueChanged<FlarkV3InlineLinkAnnotation>? onLinkActivated,
  FlarkV3InlineImageBuilder? inlineImageBuilder,
  Listenable? activePresentationProgress,
  FlarkV3ActivePresentationReadiness? activePresentationReadiness,
}) => Directionality(
  textDirection: TextDirection.ltr,
  child: Center(
    child: SizedBox(
      width: 640,
      height: 600,
      child: FlarkV3VirtualizedLiveSurface.withActiveEditorBuilder(
        activeEditorBuilder: activeEditorBuilder,
        visibleBlockCoordinator: fixture.coordinator,
        presentationSource: fixture.source,
        controller: fixture.surfaceController,
        focusNode: focusNode,
        scrollController: scrollController,
        horizontalPadding: horizontalPadding,
        windowBlockCount: fixture.windowBlockCount,
        onLinkActivated: onLinkActivated,
        inlineImageBuilder: inlineImageBuilder,
        activePresentationProgress: activePresentationProgress,
        activePresentationReadiness: activePresentationReadiness,
      ),
    ),
  ),
);

TextSpan _passiveSpan(WidgetTester tester, int ordinal) =>
    tester.widget<RichText>(find.byKey(_passiveTextKey(ordinal))).text
        as TextSpan;

Key _passiveTextKey(int ordinal) =>
    ValueKey<Object>(('flark-v3-passive-text', ordinal));

TextStyle _styleForText(TextSpan root, String text) {
  for (final child in root.children!) {
    final span = child as TextSpan;
    if (span.text == text) return span.style!;
  }
  throw StateError('No passive span contains "$text".');
}

Offset _passiveTextPosition(
  WidgetTester tester, {
  required int ordinal,
  required int startUtf16,
  required int endUtf16,
}) {
  final paragraph = tester.renderObject<RenderParagraph>(
    find.byKey(_passiveTextKey(ordinal)),
  );
  final boxes = paragraph.getBoxesForSelection(
    TextSelection(baseOffset: startUtf16, extentOffset: endUtf16),
  );
  if (boxes.isEmpty) {
    throw StateError('The requested passive text range has no hit-test box.');
  }
  return paragraph.localToGlobal(boxes.first.toRect().center);
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

final class _SurfaceFixture {
  _SurfaceFixture._({
    required this.identity,
    required this.driver,
    required this.scheduler,
    required this.coordinator,
    required this.source,
    required this.windowBlockCount,
  });

  factory _SurfaceFixture.large({
    required int blockCount,
    required int initialActiveOrdinal,
  }) {
    const sourceUnit = 10;
    final identity = _identity(sourceLength: blockCount * sourceUnit);
    final scheduler = _ManualFrameScheduler();
    final driver = _FakeVisibleBlockDriver(
      identity: identity,
      sourceLengthUtf16: blockCount * sourceUnit,
      blockForOrdinal: (ordinal) =>
          _paragraphStructuralBlock(ordinal, sourceUnit: sourceUnit),
    );
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 64 * 1024,
        maximumBlockCount: 64,
        maximumStoragePagesVisited: 65,
        maximumOpenDepth: 16,
        maximumTreeNodesVisited: 1024,
      ),
    );
    const windowBlockCount = 64;
    late final _FakePresentationSource source;
    source = _FakePresentationSource(
      identity: identity,
      totalBlockCount: blockCount,
      activeOrdinal: initialActiveOrdinal,
      windowBlockCount: windowBlockCount,
      presentationForOrdinal: (ordinal) =>
          _paragraphPresentation(identity, ordinal, sourceUnit: sourceUnit),
      requestStructuralWindow: (start, end) {
        coordinator.requestVisibleSourceRange(
          TextRange(start: start * sourceUnit, end: end * sourceUnit),
          maximumBlocks: end - start,
        );
      },
    );
    return _SurfaceFixture._(
      identity: identity,
      driver: driver,
      scheduler: scheduler,
      coordinator: coordinator,
      source: source,
      windowBlockCount: windowBlockCount,
    );
  }

  factory _SurfaceFixture.mixed() {
    const sourceUnit = 100;
    const blockCount = 5;
    final identity = _identity(sourceLength: sourceUnit * blockCount);
    final scheduler = _ManualFrameScheduler();
    final structures = [
      _paragraphStructuralBlock(0, sourceUnit: sourceUnit),
      _headingStructuralBlock(1, sourceUnit: sourceUnit),
      _fenceStructuralBlock(2, sourceUnit: sourceUnit),
      _paragraphStructuralBlock(3, sourceUnit: sourceUnit),
      _blankStructuralBlock(4, sourceUnit: sourceUnit),
    ];
    final driver = _FakeVisibleBlockDriver(
      identity: identity,
      sourceLengthUtf16: sourceUnit * blockCount,
      blockForOrdinal: (ordinal) => structures[ordinal],
    );
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 16 * 1024,
        maximumBlockCount: blockCount,
        maximumStoragePagesVisited: blockCount + 1,
        maximumOpenDepth: 16,
        maximumTreeNodesVisited: 64,
      ),
    );
    final presentations = [
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 0,
        physicalSource: _span(0, 100),
        visibleSource: _span(0, 100),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: 'Alpha bold and emphasis with code.',
        runs: [
          _run(0, 6),
          _run(6, 10, [FlarkV3InlineFactKind.strong]),
          _run(10, 15),
          _run(15, 23, [FlarkV3InlineFactKind.emphasis]),
          _run(23, 29),
          _run(29, 33, [FlarkV3InlineFactKind.code]),
          _run(33, 34),
        ],
      ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 1,
        physicalSource: _span(100, 200),
        visibleSource: _span(103, 199),
        kind: FlarkV3DocumentStructureKind.heading,
        headingLevel: 2,
        displayText: 'Heading strong',
        runs: [
          _run(0, 8),
          _run(8, 14, [FlarkV3InlineFactKind.strong]),
        ],
      ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 2,
        physicalSource: _span(200, 300),
        visibleSource: _span(210, 290),
        kind: FlarkV3DocumentStructureKind.fencedCode,
        displayText: 'final value = 1;\n',
        runs: [_run(0, 17)],
      ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 3,
        physicalSource: _span(300, 400),
        visibleSource: _span(300, 400),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: 'Tail is active.',
        runs: [_run(0, 15)],
      ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 4,
        physicalSource: _span(400, 500),
        visibleSource: _span(400, 400),
        kind: FlarkV3DocumentStructureKind.unknown,
        displayText: '',
        runs: const [],
      ),
    ];
    final source = _FakePresentationSource(
      identity: identity,
      totalBlockCount: blockCount,
      activeOrdinal: 3,
      windowBlockCount: blockCount,
      presentationForOrdinal: (ordinal) => presentations[ordinal],
      requestStructuralWindow: (start, end) {
        coordinator.requestVisibleSourceRange(
          TextRange(start: start * sourceUnit, end: end * sourceUnit),
          maximumBlocks: end - start,
        );
      },
    );
    return _SurfaceFixture._(
      identity: identity,
      driver: driver,
      scheduler: scheduler,
      coordinator: coordinator,
      source: source,
      windowBlockCount: blockCount,
    );
  }

  factory _SurfaceFixture.links() {
    const sourceUnit = 100;
    const blockCount = 2;
    const uri = 'https://example.com';
    const email = 'dev@example.com';
    const display = 'before $uri and $email after';
    final annotations = _angleLinkAnnotations();
    final identity = _identity(sourceLength: sourceUnit * blockCount);
    final scheduler = _ManualFrameScheduler();
    final driver = _FakeVisibleBlockDriver(
      identity: identity,
      sourceLengthUtf16: sourceUnit * blockCount,
      blockForOrdinal: (ordinal) =>
          _paragraphStructuralBlock(ordinal, sourceUnit: sourceUnit),
    );
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 16 * 1024,
        maximumBlockCount: blockCount,
        maximumStoragePagesVisited: blockCount + 1,
        maximumOpenDepth: 16,
        maximumTreeNodesVisited: 64,
      ),
    );
    final uriStart = 'before '.length;
    final emailStart = uriStart + uri.length + ' and '.length;
    final presentations = [
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 0,
        physicalSource: _span(0, sourceUnit),
        visibleSource: _span(0, sourceUnit),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: display,
        runs: [
          _run(0, uriStart),
          _linkRun(uriStart, uriStart + 8, annotations.uri),
          _linkRun(uriStart + 8, uriStart + uri.length, annotations.uri),
          _run(uriStart + uri.length, emailStart),
          _linkRun(emailStart, emailStart + email.length, annotations.email),
          _run(emailStart + email.length, display.length),
        ],
      ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 1,
        physicalSource: _span(sourceUnit, sourceUnit * 2),
        visibleSource: _span(sourceUnit, sourceUnit * 2),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: 'Active editor',
        runs: [_run(0, 'Active editor'.length)],
      ),
    ];
    final source = _FakePresentationSource(
      identity: identity,
      totalBlockCount: blockCount,
      activeOrdinal: 1,
      windowBlockCount: blockCount,
      presentationForOrdinal: (ordinal) => presentations[ordinal],
      requestStructuralWindow: (start, end) {
        coordinator.requestVisibleSourceRange(
          TextRange(start: start * sourceUnit, end: end * sourceUnit),
          maximumBlocks: end - start,
        );
      },
    );
    return _SurfaceFixture._(
      identity: identity,
      driver: driver,
      scheduler: scheduler,
      coordinator: coordinator,
      source: source,
      windowBlockCount: blockCount,
    );
  }

  factory _SurfaceFixture.directMedia() {
    const sourceUnit = 100;
    const blockCount = 5;
    final identity = _identity(sourceLength: sourceUnit * blockCount);
    final scheduler = _ManualFrameScheduler();
    final driver = _FakeVisibleBlockDriver(
      identity: identity,
      sourceLengthUtf16: sourceUnit * blockCount,
      blockForOrdinal: (ordinal) =>
          _paragraphStructuralBlock(ordinal, sourceUnit: sourceUnit),
    );
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 16 * 1024,
        maximumBlockCount: blockCount,
        maximumStoragePagesVisited: blockCount + 1,
        maximumOpenDepth: 16,
        maximumTreeNodesVisited: 64,
      ),
    );
    final projections = [
      _directProjection(
        '[label](dest "title")',
        records: [
          _inlineRecord(
            kind: 10,
            start: 0,
            length: 21,
            contentStart: 1,
            contentLength: 5,
          ),
        ],
        entries: const [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: 8,
            destinationLength: 4,
            titleStart: 13,
            titleLength: 7,
            cookedDestination: 'dest',
            cookedTitle: 'title',
          ),
        ],
      ),
      _directProjection(
        '![[inside](ignored)](image-only)',
        records: [
          _inlineRecord(
            kind: 11,
            start: 0,
            length: 32,
            contentStart: 2,
            contentLength: 17,
          ),
          _inlineRecord(
            kind: 10,
            start: 2,
            length: 17,
            contentStart: 3,
            contentLength: 6,
          ),
        ],
        entries: const [
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
      _directProjection(
        '[![hero](hero.png)](outer)',
        records: [
          _inlineRecord(
            kind: 10,
            start: 0,
            length: 26,
            contentStart: 1,
            contentLength: 17,
          ),
          _inlineRecord(
            kind: 11,
            start: 1,
            length: 17,
            contentStart: 3,
            contentLength: 4,
          ),
        ],
        entries: const [
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
      _directProjection(
        '![](empty.png)',
        records: [
          _inlineRecord(
            kind: 11,
            start: 0,
            length: 14,
            contentStart: 2,
            contentLength: 0,
          ),
        ],
        entries: const [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: 4,
            destinationLength: 9,
            cookedDestination: 'empty.png',
          ),
        ],
      ),
    ];
    final presentations = [
      for (var ordinal = 0; ordinal < projections.length; ordinal += 1)
        _presentationFromProjection(
          identity,
          ordinal,
          sourceUnit: sourceUnit,
          projection: projections[ordinal],
        ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 4,
        physicalSource: _span(400, 500),
        visibleSource: _span(400, 500),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: 'Active editor',
        runs: [_run(0, 'Active editor'.length)],
      ),
    ];
    final source = _FakePresentationSource(
      identity: identity,
      totalBlockCount: blockCount,
      activeOrdinal: 4,
      windowBlockCount: blockCount,
      presentationForOrdinal: (ordinal) => presentations[ordinal],
      requestStructuralWindow: (start, end) {
        coordinator.requestVisibleSourceRange(
          TextRange(start: start * sourceUnit, end: end * sourceUnit),
          maximumBlocks: end - start,
        );
      },
    );
    return _SurfaceFixture._(
      identity: identity,
      driver: driver,
      scheduler: scheduler,
      coordinator: coordinator,
      source: source,
      windowBlockCount: blockCount,
    );
  }

  factory _SurfaceFixture.referenceMedia() {
    const sourceUnit = 100;
    const blockCount = 4;
    final identity = _identity(sourceLength: sourceUnit * blockCount);
    final scheduler = _ManualFrameScheduler();
    final driver = _FakeVisibleBlockDriver(
      identity: identity,
      sourceLengthUtf16: sourceUnit * blockCount,
      blockForOrdinal: (ordinal) =>
          _paragraphStructuralBlock(ordinal, sourceUnit: sourceUnit),
    );
    final coordinator = FlarkV3FlutterVisibleBlockCoordinator.fromDriver(
      driver: driver,
      frameScheduler: scheduler,
      pageBudget: const FlarkV3DocumentBlockRangeBudget(
        maximumEncodedBytes: 16 * 1024,
        maximumBlockCount: blockCount,
        maximumStoragePagesVisited: blockCount + 1,
        maximumOpenDepth: 16,
        maximumTreeNodesVisited: 64,
      ),
    );

    const referenceLinkUse = '[label][id]';
    const referenceLinkSource =
        '$referenceLinkUse\n\n[id]: /resolved "ref title"';
    final referenceLinkDestination = referenceLinkSource.indexOf('/resolved');
    final referenceLinkTitle = referenceLinkSource.indexOf('"ref title"');

    const unlinkedImageUse = '![[inside][inner]][image]';
    const unlinkedImageSource =
        '$unlinkedImageUse\n\n[inner]: /ignored\n'
        '[image]: /image-only "image title"';
    final innerDestination = unlinkedImageSource.indexOf('/ignored');
    final unlinkedImageDestination = unlinkedImageSource.indexOf('/image-only');
    final unlinkedImageTitle = unlinkedImageSource.indexOf('"image title"');

    const linkedImageUse = '[![hero][img]][outer]';
    const linkedImageSource =
        '$linkedImageUse\n\n[img]: /hero.png "hero title"\n'
        '[outer]: /outer "outer title"';
    final linkedImageDestination = linkedImageSource.indexOf('/hero.png');
    final linkedImageTitle = linkedImageSource.indexOf('"hero title"');
    final outerDestination = linkedImageSource.indexOf('/outer');
    final outerTitle = linkedImageSource.indexOf('"outer title"');

    final projections = [
      _referenceProjection(
        referenceLinkSource,
        leafEnd: referenceLinkUse.length,
        records: [
          _inlineRecord(
            kind: 12,
            start: 0,
            length: referenceLinkUse.length,
            contentStart: 1,
            contentLength: 5,
          ),
        ],
        entries: [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: referenceLinkDestination,
            destinationLength: '/resolved'.length,
            titleStart: referenceLinkTitle,
            titleLength: '"ref title"'.length,
            cookedDestination: '/resolved',
            cookedTitle: 'ref title',
          ),
        ],
      ),
      _referenceProjection(
        unlinkedImageSource,
        leafEnd: unlinkedImageUse.length,
        records: [
          _inlineRecord(
            kind: 13,
            start: 0,
            length: unlinkedImageUse.length,
            contentStart: 2,
            contentLength: 15,
          ),
          _inlineRecord(
            kind: 12,
            start: 2,
            length: 15,
            contentStart: 3,
            contentLength: 6,
          ),
        ],
        entries: [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: unlinkedImageDestination,
            destinationLength: '/image-only'.length,
            titleStart: unlinkedImageTitle,
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
      _referenceProjection(
        linkedImageSource,
        leafEnd: linkedImageUse.length,
        records: [
          _inlineRecord(
            kind: 12,
            start: 0,
            length: linkedImageUse.length,
            contentStart: 1,
            contentLength: 12,
          ),
          _inlineRecord(
            kind: 13,
            start: 1,
            length: 12,
            contentStart: 3,
            contentLength: 4,
          ),
        ],
        entries: [
          _InlineValueEntry(
            parentFactOrdinal: 0,
            destinationStart: outerDestination,
            destinationLength: '/outer'.length,
            titleStart: outerTitle,
            titleLength: '"outer title"'.length,
            cookedDestination: '/outer',
            cookedTitle: 'outer title',
          ),
          _InlineValueEntry(
            parentFactOrdinal: 1,
            destinationStart: linkedImageDestination,
            destinationLength: '/hero.png'.length,
            titleStart: linkedImageTitle,
            titleLength: '"hero title"'.length,
            cookedDestination: '/hero.png',
            cookedTitle: 'hero title',
          ),
        ],
      ),
    ];
    final presentations = [
      for (var ordinal = 0; ordinal < projections.length; ordinal += 1)
        _presentationFromProjection(
          identity,
          ordinal,
          sourceUnit: sourceUnit,
          projection: projections[ordinal],
        ),
      FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: identity,
        ordinal: 3,
        physicalSource: _span(300, 400),
        visibleSource: _span(300, 400),
        kind: FlarkV3DocumentStructureKind.paragraph,
        displayText: 'Active editor',
        runs: [_run(0, 'Active editor'.length)],
      ),
    ];
    final source = _FakePresentationSource(
      identity: identity,
      totalBlockCount: blockCount,
      activeOrdinal: 3,
      windowBlockCount: blockCount,
      presentationForOrdinal: (ordinal) => presentations[ordinal],
      requestStructuralWindow: (start, end) {
        coordinator.requestVisibleSourceRange(
          TextRange(start: start * sourceUnit, end: end * sourceUnit),
          maximumBlocks: end - start,
        );
      },
    );
    return _SurfaceFixture._(
      identity: identity,
      driver: driver,
      scheduler: scheduler,
      coordinator: coordinator,
      source: source,
      windowBlockCount: blockCount,
    );
  }

  final FlarkV3ViewportPresentationIdentity identity;
  final _FakeVisibleBlockDriver driver;
  final _ManualFrameScheduler scheduler;
  final FlarkV3FlutterVisibleBlockCoordinator coordinator;
  final _FakePresentationSource source;
  final int windowBlockCount;
  final surfaceController = FlarkV3VirtualizedLiveSurfaceController();

  void materializeCurrentWindow() {
    final snapshot = source.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
    source.requestStructuralWindow(
      snapshot.firstOrdinal,
      snapshot.lastOrdinal + 1,
    );
  }

  void flushCoordinator() => scheduler.flushAll();

  Future<void> close() async {
    coordinator.dispose();
    await driver.close();
  }
}

final class _FakePresentationSource extends ChangeNotifier
    implements FlarkV3ViewportPresentationSource {
  _FakePresentationSource({
    required this.identity,
    required this.totalBlockCount,
    required int activeOrdinal,
    required this.windowBlockCount,
    required this.presentationForOrdinal,
    required this.requestStructuralWindow,
  }) : _snapshot = _page(
         identity: identity,
         totalBlockCount: totalBlockCount,
         activeOrdinal: activeOrdinal,
         windowBlockCount: windowBlockCount,
         presentationForOrdinal: presentationForOrdinal,
       );

  final FlarkV3ViewportPresentationIdentity identity;
  final int totalBlockCount;
  final int windowBlockCount;
  final FlarkV3ParserAuthoredBlockPresentation Function(int ordinal)
  presentationForOrdinal;
  final void Function(int startOrdinal, int endOrdinal) requestStructuralWindow;

  FlarkV3ViewportSurfaceSnapshot _snapshot;

  @override
  FlarkV3ViewportSurfaceSnapshot get snapshot => _snapshot;

  @override
  void requestWindow(FlarkV3ViewportWindowDemand demand) {
    final current = _snapshot;
    final activeOrdinal = current.activeOrdinal;
    final next = _page(
      identity: identity,
      totalBlockCount: totalBlockCount,
      activeOrdinal: activeOrdinal,
      windowBlockCount: demand.maximumBlocks,
      presentationForOrdinal: presentationForOrdinal,
      centerOrdinal: demand.centerOrdinal,
    );
    if (current is FlarkV3ExactViewportSurfaceSnapshot &&
        current.firstOrdinal == next.firstOrdinal &&
        current.lastOrdinal == next.lastOrdinal) {
      return;
    }
    _snapshot = next;
    requestStructuralWindow(next.firstOrdinal, next.lastOrdinal + 1);
    notifyListeners();
  }

  @override
  void activateOrdinal(int ordinal) {
    if (ordinal < 0 || ordinal >= totalBlockCount) {
      throw RangeError.index(ordinal, this, 'ordinal');
    }
    final current = _snapshot;
    if (current is FlarkV3ExactViewportSurfaceSnapshot &&
        current.containsOrdinal(ordinal)) {
      _snapshot = FlarkV3ExactViewportSurfaceSnapshot(
        totalBlockCount: current.totalBlockCount,
        activeOrdinal: ordinal,
        estimatedBlockExtent: current.estimatedBlockExtent,
        identity: current.identity,
        blocks: current.blocks,
      );
    } else {
      _snapshot = _page(
        identity: identity,
        totalBlockCount: totalBlockCount,
        activeOrdinal: ordinal,
        windowBlockCount: windowBlockCount,
        presentationForOrdinal: presentationForOrdinal,
      );
      final page = _snapshot as FlarkV3ExactViewportSurfaceSnapshot;
      requestStructuralWindow(page.firstOrdinal, page.lastOrdinal + 1);
    }
    notifyListeners();
  }

  void enterGap(Object reason) {
    final current = _snapshot;
    _snapshot = FlarkV3SourceGapViewportSurfaceSnapshot(
      totalBlockCount: current.totalBlockCount,
      activeOrdinal: current.activeOrdinal,
      estimatedBlockExtent: current.estimatedBlockExtent,
      reason: reason,
    );
    notifyListeners();
  }

  static FlarkV3ExactViewportSurfaceSnapshot _page({
    required FlarkV3ViewportPresentationIdentity identity,
    required int totalBlockCount,
    required int activeOrdinal,
    required int windowBlockCount,
    required FlarkV3ParserAuthoredBlockPresentation Function(int ordinal)
    presentationForOrdinal,
    int? centerOrdinal,
  }) {
    final center = centerOrdinal ?? activeOrdinal;
    final start = (center - windowBlockCount ~/ 2).clamp(
      0,
      totalBlockCount - windowBlockCount,
    );
    final end = (start + windowBlockCount).clamp(0, totalBlockCount);
    return FlarkV3ExactViewportSurfaceSnapshot(
      totalBlockCount: totalBlockCount,
      activeOrdinal: activeOrdinal,
      estimatedBlockExtent: 44,
      identity: identity,
      blocks: [
        for (var ordinal = start; ordinal < end; ordinal += 1)
          presentationForOrdinal(ordinal),
      ],
    );
  }
}

final class _FakeVisibleBlockDriver
    implements FlarkV3FlutterVisibleBlockDriver {
  _FakeVisibleBlockDriver({
    required this.identity,
    required this.sourceLengthUtf16,
    required this.blockForOrdinal,
  });

  final FlarkV3ViewportPresentationIdentity identity;
  @override
  final int sourceLengthUtf16;
  final FlarkV3DocumentStructuralBlock Function(int ordinal) blockForOrdinal;
  final _changes = StreamController<void>.broadcast(sync: true);

  @override
  int get sourceRevision => identity.sourceVersion.revision;

  @override
  int get structureGeneration => identity.structureGeneration;

  @override
  bool get isQueryable => true;

  @override
  Stream<void> get changes => _changes.stream;

  @override
  FlarkV3VisibleBlockSet advance(
    FlarkV3VisibleBlockDemand demand, {
    required FlarkV3DocumentBlockRangeBudget budget,
  }) {
    final sourceUnit = sourceLengthUtf16 ~/ 4096 == 10 ? 10 : 100;
    final startOrdinal = demand.startUtf16 ~/ sourceUnit;
    final endOrdinal = demand.endUtf16 ~/ sourceUnit;
    final blocks = [
      for (var ordinal = startOrdinal; ordinal < endOrdinal; ordinal += 1)
        blockForOrdinal(ordinal),
    ];
    return FlarkV3ExactVisibleBlockSet(
      demand: demand,
      coveredSource: _span(demand.startUtf16, demand.endUtf16),
      blocks: blocks,
      demandCovered: true,
      truncated: false,
    );
  }

  @override
  void reset() {}

  Future<void> close() => _changes.close();
}

final class _ManualFrameScheduler implements FlarkV3FrameScheduler {
  final _callbacks = <VoidCallback>[];

  bool get hasPending => _callbacks.isNotEmpty;

  @override
  void schedule(VoidCallback callback) => _callbacks.add(callback);

  void flushAll() {
    while (_callbacks.isNotEmpty) {
      final callback = _callbacks.removeAt(0);
      callback();
    }
  }
}

final class _FakeActivePresentationReadiness extends ChangeNotifier {
  bool ready = false;

  bool call(FlarkV3ParserAuthoredBlockPresentation target) => ready;

  void complete() {
    ready = true;
    notifyListeners();
  }
}

FlarkV3ViewportPresentationIdentity _identity({required int sourceLength}) {
  final document = FlarkV3SourceDocument.fromString('x' * sourceLength);
  final sourceVersion = FlarkV3SourceVersion.fromDocument(
    documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
    document: document,
  );
  return FlarkV3ViewportPresentationIdentity(
    sourceVersion: sourceVersion,
    sourceRoot: FlarkV3SourceRootId(5, 6),
    parseGeneration: 1,
    structureGeneration: 1,
    viewportGeneration: 1,
  );
}

FlarkV3ParserAuthoredBlockPresentation _paragraphPresentation(
  FlarkV3ViewportPresentationIdentity identity,
  int ordinal, {
  required int sourceUnit,
}) {
  final display = 'Presented block $ordinal.';
  return FlarkV3ParserAuthoredBlockPresentation.authoritative(
    identity: identity,
    ordinal: ordinal,
    physicalSource: _span(ordinal * sourceUnit, (ordinal + 1) * sourceUnit),
    visibleSource: _span(ordinal * sourceUnit, (ordinal + 1) * sourceUnit),
    kind: FlarkV3DocumentStructureKind.paragraph,
    displayText: display,
    runs: [_run(0, display.length)],
  );
}

FlarkV3ParserAuthoredBlockPresentation _presentationFromProjection(
  FlarkV3ViewportPresentationIdentity identity,
  int ordinal, {
  required int sourceUnit,
  required FlarkV3InlineProjection projection,
}) {
  final runs = [
    for (final run in projection.runs)
      FlarkV3PassiveInlineRun(
        startUtf16: run.displayStartUtf16,
        endUtf16: run.displayEndUtf16,
        styles: run.semanticStyles,
        linkAnnotation: run.linkAnnotation,
      ),
  ];
  final facts =
      <FlarkV3InlineFact>{
        for (final run in projection.runs) ...run.semanticFacts,
      }.toList()..sort(
        (left, right) =>
            left.source.startUtf16.compareTo(right.source.startUtf16),
      );
  final images = <FlarkV3PassiveInlineImage>[];
  for (final image in projection.imageAnnotations) {
    FlarkV3InlineLinkAnnotation? outerLink;
    for (final fact in facts) {
      if (fact.linkAnnotation != null &&
          fact.content.startUtf16 <= image.source.startUtf16 &&
          fact.content.endUtf16 >= image.source.endUtf16) {
        outerLink = fact.linkAnnotation;
      }
    }
    images.add(
      FlarkV3PassiveInlineImage(
        startUtf16: projection.sourceToDisplayOffset(image.content.startUtf16),
        endUtf16: projection.sourceToDisplayOffset(image.content.endUtf16),
        annotation: image,
        outerLink: outerLink,
      ),
    );
  }
  return FlarkV3ParserAuthoredBlockPresentation.authoritative(
    identity: identity,
    ordinal: ordinal,
    physicalSource: _span(ordinal * sourceUnit, (ordinal + 1) * sourceUnit),
    visibleSource: _span(ordinal * sourceUnit, (ordinal + 1) * sourceUnit),
    kind: FlarkV3DocumentStructureKind.paragraph,
    displayText: projection.displayText,
    runs: runs,
    images: images,
  );
}

FlarkV3PassiveInlineRun _run(
  int start,
  int end, [
  Iterable<FlarkV3InlineFactKind> styles = const [],
]) => FlarkV3PassiveInlineRun(startUtf16: start, endUtf16: end, styles: styles);

FlarkV3PassiveInlineRun _linkRun(
  int start,
  int end,
  FlarkV3InlineLinkAnnotation linkAnnotation,
) => FlarkV3PassiveInlineRun(
  startUtf16: start,
  endUtf16: end,
  styles: const <FlarkV3InlineFactKind>[],
  linkAnnotation: linkAnnotation,
);

FlarkV3DocumentStructuralBlock _paragraphStructuralBlock(
  int ordinal, {
  required int sourceUnit,
}) {
  final source = _span(ordinal * sourceUnit, (ordinal + 1) * sourceUnit);
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: source,
      visibleSource: source,
      referenceDefinitionCount: 0,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: source,
      projectedSource: source,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralBlock _headingStructuralBlock(
  int ordinal, {
  required int sourceUnit,
}) {
  final start = ordinal * sourceUnit;
  final source = _span(start, start + sourceUnit);
  final content = _span(start + 3, start + sourceUnit - 1);
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.heading,
      source: source,
      visibleSource: content,
      referenceDefinitionCount: 0,
      heading: FlarkV3AtxHeadingFacts(
        level: 2,
        contentSource: content,
        openingMarker: _span(start, start + 3),
        closingMarker: null,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.heading,
      source: source,
      projectedSource: content,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralBlock _blankStructuralBlock(
  int ordinal, {
  required int sourceUnit,
}) {
  final start = ordinal * sourceUnit;
  final source = _span(start, start + sourceUnit);
  final visible = _span(start, start);
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      visibleSource: visible,
      referenceDefinitionCount: 0,
      unknownReason: FlarkV3DocumentUnknownReason.blankBoundary,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      projectedSource: source,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralBlock _fenceStructuralBlock(
  int ordinal, {
  required int sourceUnit,
}) {
  final start = ordinal * sourceUnit;
  final source = _span(start, start + sourceUnit);
  final body = _span(start + 10, start + sourceUnit - 10);
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.fencedCode,
      source: source,
      visibleSource: body,
      referenceDefinitionCount: 0,
      fencedCode: FlarkV3FencedCodeFacts(
        marker: FlarkV3CodeFenceMarker.backtick,
        openingIndent: 0,
        openingMarker: _span(start, start + 3),
        rawInfoSource: _span(start + 3, start + 7),
        bodySource: body,
        closingMarker: _span(start + sourceUnit - 10, start + sourceUnit - 7),
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.fencedCode,
      source: source,
      projectedSource: body,
      runCount: 1,
    ),
  );
}

FlarkV3SourceSpan _span(int start, int end) => FlarkV3SourceSpan(
  startUtf8: start,
  endUtf8: end,
  startUtf16: start,
  endUtf16: end,
);

({FlarkV3InlineLinkAnnotation uri, FlarkV3InlineLinkAnnotation email})
_angleLinkAnnotations() {
  const uri = 'https://example.com';
  const email = 'dev@example.com';
  const source = 'before <$uri> and <$email> after';
  final document = FlarkV3SourceDocument.fromString(source);
  final version = FlarkV3SourceVersion.fromDocument(
    documentSession: FlarkV3DocumentSessionId(91, 92, 93, 94),
    document: document,
  );
  final leaf = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: document.utf8Length,
    startUtf16: 0,
    endUtf16: document.utf16Length,
  );
  final uriSourceStart = 'before '.length;
  final uriSourceLength = uri.length + 2;
  final emailSourceStart = uriSourceStart + uriSourceLength + ' and '.length;
  final records = [
    _inlineRecord(
      kind: 5,
      start: uriSourceStart,
      length: uriSourceLength,
      contentStart: uriSourceStart + 1,
      contentLength: uri.length,
    ),
    _inlineRecord(
      kind: 6,
      start: emailSourceStart,
      length: email.length + 2,
      contentStart: emailSourceStart + 1,
      contentLength: email.length,
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
    encodedFacts: Uint8List.fromList([for (final record in records) ...record]),
  );
  return (
    uri: facts.facts[0].linkAnnotation!,
    email: facts.facts[1].linkAnnotation!,
  );
}

FlarkV3InlineProjection _directProjection(
  String source, {
  required List<Uint8List> records,
  required List<_InlineValueEntry> entries,
}) => _valueProjection(
  source,
  leafStart: 0,
  leafEnd: source.length,
  records: records,
  entries: entries,
);

FlarkV3InlineProjection _referenceProjection(
  String source, {
  required int leafEnd,
  required List<Uint8List> records,
  required List<_InlineValueEntry> entries,
}) => _valueProjection(
  source,
  leafStart: 0,
  leafEnd: leafEnd,
  records: records,
  entries: entries,
);

FlarkV3InlineProjection _valueProjection(
  String source, {
  required int leafStart,
  required int leafEnd,
  required List<Uint8List> records,
  required List<_InlineValueEntry> entries,
}) {
  final document = FlarkV3SourceDocument.fromString(source);
  final version = FlarkV3SourceVersion.fromDocument(
    documentSession: FlarkV3DocumentSessionId(101, 102, 103, 104),
    document: document,
  );
  final leaf = FlarkV3SourceSpan(
    startUtf8: leafStart,
    endUtf8: leafEnd,
    startUtf16: leafStart,
    endUtf16: leafEnd,
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
    inlineValues: FlarkV3InlineValuesPayload(
      sourceVersion: version,
      profilePartition: 3,
      source: leaf,
      encodedBytes: _encodeInlineValues(entries),
    ),
  );
  return FlarkV3InlineProjection.fromValidatedFacts(
    sourceDocument: document,
    expectedSource: version,
    facts: facts,
    markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  );
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

Uint8List _inlineRecord({
  required int kind,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, kind)
    ..setUint8(1, 0)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}
