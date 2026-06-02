import '../document/sovereign_document.dart';
import '../selection/sovereign_selection.dart';
import '../transaction/sovereign_transaction.dart';

final class SovereignEditorState {
  const SovereignEditorState({
    required this.document,
    required this.selection,
  });

  factory SovereignEditorState.fromMarkdown(
    String markdown, {
    SovereignSelection? selection,
  }) {
    final document = SovereignDocument.fromMarkdown(markdown);
    final initialSelection =
        selection ?? SovereignSelection.collapsed(document.length);
    initialSelection.validate(document.length);
    return SovereignEditorState(
      document: document,
      selection: initialSelection,
    );
  }

  final SovereignDocument document;
  final SovereignSelection selection;

  int get revision => document.revision;

  String get markdown => document.markdown;

  SovereignEditorState applyTransaction(SovereignTransaction transaction) {
    transaction.selectionBefore?.validate(document.length);
    final nextDocument = transaction.applyToDocument(document);
    final nextSelection = transaction.selectionAfter ??
        transaction.mapSelection(selection).validate(nextDocument.length);
    nextSelection.validate(nextDocument.length);

    return SovereignEditorState(
      document: nextDocument,
      selection: nextSelection,
    );
  }

  SovereignEditorState copyWith({
    SovereignDocument? document,
    SovereignSelection? selection,
  }) {
    final nextDocument = document ?? this.document;
    final nextSelection = selection ?? this.selection;
    nextSelection.validate(nextDocument.length);
    return SovereignEditorState(
      document: nextDocument,
      selection: nextSelection,
    );
  }
}
