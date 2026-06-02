import '../document/sovereign_document.dart';
import '../selection/sovereign_selection.dart';
import '../transaction/sovereign_transaction.dart';

final class FlarkEditorState {
  const FlarkEditorState({required this.document, required this.selection});

  factory FlarkEditorState.fromMarkdown(
    String markdown, {
    FlarkSelection? selection,
  }) {
    final document = FlarkDocument.fromMarkdown(markdown);
    final initialSelection =
        selection ?? FlarkSelection.collapsed(document.length);
    initialSelection.validate(document.length);
    return FlarkEditorState(document: document, selection: initialSelection);
  }

  final FlarkDocument document;
  final FlarkSelection selection;

  int get revision => document.revision;

  String get markdown => document.markdown;

  FlarkEditorState applyTransaction(FlarkTransaction transaction) {
    transaction.selectionBefore?.validate(document.length);
    final nextDocument = transaction.applyToDocument(document);
    final nextSelection =
        transaction.selectionAfter ??
        transaction.mapSelection(selection).validate(nextDocument.length);
    nextSelection.validate(nextDocument.length);

    return FlarkEditorState(document: nextDocument, selection: nextSelection);
  }

  FlarkEditorState copyWith({
    FlarkDocument? document,
    FlarkSelection? selection,
  }) {
    final nextDocument = document ?? this.document;
    final nextSelection = selection ?? this.selection;
    nextSelection.validate(nextDocument.length);
    return FlarkEditorState(document: nextDocument, selection: nextSelection);
  }
}
