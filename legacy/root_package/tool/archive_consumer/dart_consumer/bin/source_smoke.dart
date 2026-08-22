import 'package:flark/flark.dart';

void main() {
  final runtime = FlarkEditorRuntime.fromMarkdown('live');
  final result = runtime.applyTransaction(
    FlarkTransaction.single(
      FlarkSourceOperation.insert(4, ' **Markdown**'),
      selectionAfter: const FlarkSelection.collapsed(17),
    ),
  );
  if (result.runtime.state.markdown != 'live **Markdown**' ||
      result.appliedTransactions.length != 1) {
    throw StateError('The archive-backed Dart source runtime did not edit.');
  }
  print('Dart-only Flark source runtime passed.');
}
