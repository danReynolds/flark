import 'dart:async';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/editor.dart' show FlarkEditorWidget;
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('public toolbar state, save routing, loads and remounts', (
    t,
  ) async {
    final c = FlarkController(markdown: 'hello');
    await t.runAsync(() => c.ready);
    final saved = <String>[];
    Widget view() => MaterialApp(
      home: Scaffold(
        body: FlarkEditor(controller: c, onChanged: saved.add),
      ),
    );
    await t.pumpWidget(view());
    c.selectAll();
    await t.pump();
    await t.tap(find.byTooltip('Bold'));
    await t.pump();
    expect(c.markdown, '**hello**');
    expect(c.state.styles.bold.isOn, isTrue);
    expect(
      t
          .widget<IconButton>(
            find.byWidgetPredicate(
              (w) => w is IconButton && w.tooltip == 'Bold',
            ),
          )
          .isSelected,
      isTrue,
    );
    expect(saved, ['**hello**']);
    c.loadMarkdown('fetched');
    await t.pump();
    expect(saved.length, 1);
    await t.pumpWidget(const SizedBox());
    c.replaceMarkdown('unmounted edit');
    await t.pumpWidget(view());
    expect(saved.length, 1);
    expect(c.markdown, 'unmounted edit');
    c.replaceMarkdown('mounted edit');
    expect(saved.last, 'mounted edit');
    await t.pumpWidget(const SizedBox());
    c.dispose();
    c.dispose();
    expect(t.takeException(), isNull);
  });

  testWidgets('widget-owned session persists rebuild and new key resets it', (
    t,
  ) async {
    Widget view(String seed, String key) => MaterialApp(
      home: Scaffold(
        body: FlarkEditor(key: ValueKey(key), initialMarkdown: seed),
      ),
    );
    await t.pumpWidget(view('first', 'a'));
    await t.pump();
    expect(find.byType(FlarkEditorWidget), findsOneWidget);
    var host = t.widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget));
    host.actions!.replaceMarkdown('edited');
    await t.pumpWidget(view('ignored seed', 'a'));
    host = t.widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget));
    expect(host.actions!.markdown, 'edited');
    await t.pumpWidget(view('second', 'b'));
    await t.pump();
    host = t.widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget));
    expect(host.actions!.markdown, 'second');
    await t.pumpWidget(const SizedBox());
    expect(t.takeException(), isNull);
  });

  testWidgets(
    'public resource dialog targets survive focus and reject document loads',
    (t) async {
      final c = FlarkController(markdown: 'label');
      await t.runAsync(() => c.ready);
      FlarkResourceSession? resource;
      var completion = Completer<void>();
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditor(
              controller: c,
              presentResourceEditor: (_, session) {
                resource = session;
                return completion.future;
              },
            ),
          ),
        ),
      );
      c.selectAll();
      final result = c.showLinkEditor();
      expect(resource, isNotNull);
      expect(
        resource!.save(destination: 'https://example.com', label: 'label'),
        isTrue,
      );
      completion.complete();
      expect((await result).changed, isTrue);
      expect(c.markdown, '[label](<https://example.com>)');
      completion = Completer<void>();
      final stale = c.showLinkEditor();
      c.loadMarkdown('another note');
      expect(
        resource!.save(destination: 'https://wrong.test', label: 'wrong'),
        isFalse,
      );
      completion.complete();
      expect((await stale).reason, FlarkEditRejection.staleRevision);
      await t.pumpWidget(const SizedBox());
      expect((await c.showLinkEditor()).reason, FlarkEditRejection.unavailable);
      c.dispose();
    },
  );

  testWidgets(
    'dedicated Markdown updates source and has no editing view or input',
    (t) async {
      Widget view(String text) => MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: FlarkMarkdown(markdown: text, selectable: true),
          ),
        ),
      );
      await t.pumpWidget(view('# Article\n\n**body**'));
      await t.pump();
      expect(find.byType(FlarkEditorWidget), findsNothing);
      expect(t.testTextInput.hasAnyClients, isFalse);
      final surface = t.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      expect(surface.controller.editor.source, '# Article\n\n**body**');
      final semantics = SemanticsConfiguration();
      surface.describeSemanticsConfiguration(semantics);
      expect(semantics.isReadOnly, isTrue);
      expect(semantics.onSetText, isNull);
      expect(semantics.onCopy, isNotNull);
      semantics.onSetSelection!(
        const TextSelection(baseOffset: 0, extentOffset: 7),
      );
      expect(surface.controller.editor.selection.isCollapsed, isFalse);
      expect(surface.controller.editor.source, '# Article\n\n**body**');
      expect(t.testTextInput.hasAnyClients, isFalse);
      await t.pumpWidget(view('# Changed'));
      expect(surface.controller.editor.source, '# Changed');
      await t.pumpWidget(const SizedBox());
      expect(t.takeException(), isNull);
    },
  );

  testWidgets('read-only fallback is identified and offers complete source', (
    t,
  ) async {
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: FlarkMarkdown(markdown: 'x' * 20000),
          ),
        ),
      ),
    );
    await t.pump();
    expect(
      find.text('Rendered preview unavailable for this document.'),
      findsOneWidget,
    );
    expect(find.text('Copy complete Markdown'), findsOneWidget);
    expect(find.byType(FlarkSurface), findsNothing);
    await t.pumpWidget(const SizedBox());
  });
}
