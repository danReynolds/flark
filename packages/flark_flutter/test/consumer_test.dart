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
    host.session!.replaceSourceRange(
      0,
      host.session!.state.markdown.length,
      'edited',
      replaceAll: true,
    );
    await t.pumpWidget(view('ignored seed', 'a'));
    host = t.widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget));
    expect(host.session!.state.markdown, 'edited');
    await t.pumpWidget(view('second', 'b'));
    await t.pump();
    host = t.widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget));
    expect(host.session!.state.markdown, 'second');
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
    'a refused consumer command shows the notice a refused key does',
    (t) async {
      // The session hands the controller's commands to the view's input
      // controller, which says why the kernel refused one, whatever sent it.
      final c = FlarkController(markdown: '```\ncode\n```');
      await t.runAsync(() => c.ready);
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkEditor(controller: c)),
        ),
      );
      c.setSelection(5, 5);
      expect(c.setHeading(1).reason, FlarkEditRejection.unsupportedEdit);
      await t.pump();
      expect(find.text('This edit needs source mode.'), findsOneWidget);
      expect(c.markdown, '```\ncode\n```');
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets('the Source button takes the notice away, as it does alone', (
    t,
  ) async {
    // Given a session, the button switched modes through the session and
    // left the rendered mode's notice in place of the source mode banner.
    final c = FlarkController(markdown: 'abc');
    await t.runAsync(() => c.ready);
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditor(controller: c)),
      ),
    );
    expect(c.insertText('\uD800').reason, FlarkEditRejection.invalidSource);
    await t.pump();
    expect(
      find.text('The inserted text is not valid Unicode text.'),
      findsOneWidget,
    );
    await t.tap(find.text('Source'));
    await t.pump();
    expect(c.state.mode, FlarkMode.source);
    expect(
      find.text('The inserted text is not valid Unicode text.'),
      findsNothing,
    );
    expect(
      find.text('Source mode · exact Markdown remains editable'),
      findsOneWidget,
    );
    await t.pumpWidget(const SizedBox());
    c.dispose();
  });

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

  testWidgets(
    'the session opens no resource editor where the kernel cannot set one',
    (t) async {
      // Its presenter follows the kernel's canSetResource, as the toolbar's
      // buttons do, rather than opening a form whose Save can only fail.
      final c = FlarkController(markdown: 'one\n\ntwo');
      await t.runAsync(() => c.ready);
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkEditor(controller: c)),
        ),
      );
      c.setSelection(0, 8);
      await t.pump();
      expect(c.state.link.canSet, isFalse);
      for (final open in [c.showLinkEditor, c.showImageEditor]) {
        final result = open();
        await t.pump();
        expect(find.byType(AlertDialog), findsNothing);
        expect((await result).reason, FlarkEditRejection.unavailable);
      }
      expect(c.markdown, 'one\n\ntwo');
      await t.pumpWidget(const SizedBox());
      c.dispose();
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

  testWidgets('Markdown the parser cannot take shows as source until it can', (
    t,
  ) async {
    // A chat-style reveal: substring(0, n) can end between the two code units
    // of an emoji for one frame.
    const full = 'Thanks \u{1F600} **done**';
    Widget view(String text) => MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(child: FlarkMarkdown(markdown: text)),
      ),
    );
    await t.pumpWidget(view(full.substring(0, 7)));
    await t.pump();
    expect(find.byType(FlarkSurface), findsOneWidget);
    await t.pumpWidget(view(full.substring(0, 8)));
    expect(
      find.text('Rendered preview unavailable for this document.'),
      findsOneWidget,
    );
    await t.pumpWidget(view(full));
    expect(find.byType(FlarkSurface), findsOneWidget);
    expect(find.textContaining('Unable to render'), findsNothing);
    await t.pumpWidget(const SizedBox());
    expect(t.takeException(), isNull);
  });

  testWidgets('the source fallback never splits a surrogate pair', (t) async {
    // One 21,000-unit line is past the live shape limit, and its emoji
    // straddles the preview's 1,024-unit cut.
    final markdown = '${'x' * 1023}\u{1F600}${'x' * 20000}';
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(child: FlarkMarkdown(markdown: markdown)),
        ),
      ),
    );
    await t.pump();
    expect(find.text('Copy complete Markdown'), findsOneWidget);
    expect(find.textContaining('\u{1F600}'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    expect(t.takeException(), isNull);
  });

  testWidgets('an editor seeded with text it cannot hold shows its failure', (
    t,
  ) async {
    for (final seed in [
      'x\n' * (512 * 1024 + 1),
      'draft \u{1F600}'.substring(0, 7),
    ]) {
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditor(key: ValueKey(seed), initialMarkdown: seed),
          ),
        ),
      );
      expect(find.text('Unable to load editor'), findsOneWidget);
      expect(t.takeException(), isNull);
    }
    await t.pumpWidget(const SizedBox());
  });
}
