// G1 baseline probe — what `Cmd/Ctrl+A` then Copy actually places on the
// clipboard in v2's live-rendered editor.
//
// This is a *measurement*, not a specification. RFC 024 §7 requires that
// select-all + copy yield "complete exact source"; the assertions below record
// that v2 does neither, in two distinct ways:
//
//   * plain-paragraph document (one whole-document host editable) — the whole
//     document is copied, but through the *projection*, so hidden inline
//     markers are stripped: `**bold**` reaches the clipboard as `bold`;
//   * structured document (list + quote -> five per-block editables) — the
//     model selection is correctly the whole document (0..53 of 53), yet the
//     clipboard receives only the focused block's projected text.
//
// Cause: nothing in the package overrides `CopySelectionTextIntent`, so copy
// falls through to `EditableText`, which can only see its own editable.
//
// Referenced by docs/testing/ime_device_matrix_runbook.md §5 N3, which asks the
// device runner to confirm this on-device. Delete or invert this file when the
// new input surface (RFC 024 gate G4) lands.
import 'package:flark_flutter/src/v2/flutter/flutter.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  String? clipboardText;

  setUp(() {
    clipboardText = null;
    TestDefaultBinaryMessengerBinding
        .instance
        .defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.setData') {
            clipboardText = (call.arguments as Map)['text'] as String?;
          }
          if (call.method == 'Clipboard.getData') {
            return <String, dynamic>{'text': clipboardText ?? ''};
          }
          return null;
        });
  });

  Future<FlarkFlutterController> pump(WidgetTester tester, String md) async {
    final controller = FlarkFlutterController.fromMarkdown(md);
    addTearDown(controller.dispose);
    await controller.parseNow();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MediaQuery(
          data: const MediaQueryData(size: Size(800, 600)),
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );
    await tester.pump();
    return controller;
  }

  /// Runs select-all and then `EditableText`'s copy, on the editable at
  /// [finder].
  ///
  /// Select-all is dispatched through `Actions` first, so the package's
  /// document-wide `SelectAllTextIntent` override (`_selectAllDocument` in
  /// `projected_editable/live_block_text.dart`) runs exactly as Cmd/Ctrl+A
  /// drives it on the per-block surface. That override is *not* installed
  /// above the whole-document host editable, where no ancestor `Actions`
  /// handles the intent; there `EditableText`'s own action is what Cmd/Ctrl+A
  /// reaches, so fall back to it.
  Future<void> selectAllThenCopy(WidgetTester tester, Finder finder) async {
    await tester.showKeyboard(finder);
    await tester.pump();
    final handled = Actions.maybeInvoke(
      tester.element(finder),
      const SelectAllTextIntent(SelectionChangedCause.keyboard),
    );
    if (handled == null) {
      // ignore: invalid_use_of_protected_member
      tester.state<EditableTextState>(finder).selectAll(
        SelectionChangedCause.keyboard,
      );
    }
    await tester.pump();
    // ignore: invalid_use_of_protected_member
    tester.state<EditableTextState>(finder).copySelection(
      SelectionChangedCause.keyboard,
    );
    await tester.pumpAndSettle(const Duration(milliseconds: 50));
  }

  testWidgets(
    'plain-paragraph document: copy covers the whole document but strips '
    'hidden inline markers',
    (tester) async {
      const md = 'alpha **bold** one\n\nbeta `code` two';
      final controller = await pump(tester, md);
      final editable = find.byType(EditableText);
      expect(tester.widgetList(editable), hasLength(1));

      await selectAllThenCopy(tester, editable.first);

      expect(controller.markdown, md);
      expect(controller.selection.start, 0);
      expect(controller.selection.end, md.length); // 35
      // Measured: the projection, not the source.
      expect(clipboardText, 'alpha bold one\n\nbeta code two');
      expect(clipboardText, isNot(md));
    },
  );

  testWidgets(
    'structured document: model selection spans the document but copy yields '
    'only the focused block',
    (tester) async {
      const md = '- one **b** item\n- two item\n\n> quoted line\n\ntail para';
      final controller = await pump(tester, md);
      final editables = find.byType(EditableText);
      expect(tester.widgetList(editables), hasLength(5));

      await selectAllThenCopy(tester, editables.first);

      expect(controller.selection.start, 0);
      expect(controller.selection.end, md.length); // 53 — the whole document
      // Measured: one block, projected.
      expect(clipboardText, 'one b item');
    },
  );
}
