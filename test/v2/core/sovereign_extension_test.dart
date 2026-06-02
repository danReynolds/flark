import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';

void main() {
  group('SovereignExtensionSet', () {
    test('rejects duplicate extension ids', () {
      expect(
        () => SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
          const SovereignCoreEditingExtension(),
        ]),
        throwsA(isA<StateError>()),
      );
    });

    test('builds a command registry from extensions', () {
      final state = SovereignEditorState.fromMarkdown(
        'hello',
        selection: const SovereignSelection(baseOffset: 1, extentOffset: 4),
      );
      final registry = SovereignExtensionSet([
        const SovereignCoreEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('i'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'hio');
      expect(next.selection, const SovereignSelection.collapsed(2));
      expect(
        result.transaction!.metadata.intent,
        SovereignTransactionIntent.input,
      );
      expect(
        result.transaction!.metadata.parseInvalidationRange,
        const SovereignSourceRange(1, 4),
      );
    });
  });
}
