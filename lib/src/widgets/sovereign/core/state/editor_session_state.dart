import 'document_state.dart';
import 'history_state.dart';
import 'projection_state.dart';
import 'telemetry_state.dart';

class EditorSessionState {
  const EditorSessionState({
    required this.document,
    required this.projection,
    required this.history,
    required this.telemetry,
  });

  final DocumentState document;
  final ProjectionState projection;
  final HistoryState history;
  final TelemetryState telemetry;

  EditorSessionState copyWith({
    DocumentState? document,
    ProjectionState? projection,
    HistoryState? history,
    TelemetryState? telemetry,
  }) {
    return EditorSessionState(
      document: document ?? this.document,
      projection: projection ?? this.projection,
      history: history ?? this.history,
      telemetry: telemetry ?? this.telemetry,
    );
  }
}
