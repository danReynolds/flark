import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';

void main() {
  group('FlarkExtensionSet', () {
    test('rejects duplicate extension ids', () {
      expect(
        () => FlarkExtensionSet([
          const FlarkCoreEditingExtension(),
          const FlarkCoreEditingExtension(),
        ]),
        throwsA(isA<StateError>()),
      );
    });

    test('builds a command registry from extensions', () {
      final state = FlarkEditorState.fromMarkdown(
        'hello',
        selection: const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );
      final registry = FlarkExtensionSet([
        const FlarkCoreEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('i'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'hio');
      expect(next.selection, const FlarkSelection.collapsed(2));
      expect(result.transaction!.metadata.intent, FlarkTransactionIntent.input);
      expect(
        result.transaction!.metadata.parseInvalidationRange,
        const FlarkSourceRange(1, 4),
      );
    });
  });
}
