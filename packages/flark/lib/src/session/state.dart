import '../kernel/document.dart';
import '../kernel/editor.dart';
import '../kernel/projection.dart';
import '../kernel/resource.dart';
import '../kernel/style_state.dart';

enum FlarkStatus { loading, ready, failed, disposed }

enum FlarkMode { rendered, source }

enum FlarkStyle {
  bold(Style.strong),
  italic(Style.emphasis),
  strikethrough(Style.strikethrough),
  inlineCode(Style.code);

  const FlarkStyle(this.kernelStyle);
  final int kernelStyle;
}

enum FlarkSelectionScope { document, codeBlock }

enum FlarkEditOutcome { changed, unchanged, rejected }

enum FlarkEditRejection {
  notReady,
  disposed,
  staleRevision,
  invalidSource,
  sourceLimit,
  unsupportedEdit,
  extractionDeviation,
  unavailable,
  readOnly,
}

final class FlarkEditResult {
  const FlarkEditResult.changed()
    : outcome = FlarkEditOutcome.changed,
      reason = null;
  const FlarkEditResult.unchanged()
    : outcome = FlarkEditOutcome.unchanged,
      reason = null;
  const FlarkEditResult.rejected(this.reason)
    : outcome = FlarkEditOutcome.rejected;
  final FlarkEditOutcome outcome;
  final FlarkEditRejection? reason;
  bool get changed => outcome == FlarkEditOutcome.changed;
  bool get accepted => outcome != FlarkEditOutcome.rejected;
}

const _unavailableStyle = FlarkStyleState(
  value: FlarkStyleValue.off,
  canEnable: false,
  canDisable: false,
);

final class FlarkStylesState {
  FlarkStylesState(FlarkEditor? editor)
    : _values = List.unmodifiable(
        FlarkStyle.values.map(
          (style) => editor?.styleState(style.kernelStyle) ?? _unavailableStyle,
        ),
      );
  final List<FlarkStyleState> _values;
  FlarkStyleState operator [](FlarkStyle style) => _values[style.index];
  FlarkStyleState get bold => this[FlarkStyle.bold];
  FlarkStyleState get italic => this[FlarkStyle.italic];
  FlarkStyleState get strikethrough => this[FlarkStyle.strikethrough];
  FlarkStyleState get inlineCode => this[FlarkStyle.inlineCode];
}

final class FlarkHeadingState {
  const FlarkHeadingState(this.level, this.isMixed, this.canSet);

  /// Zero means paragraph; null means unavailable or mixed (see isMixed).
  final int? level;
  final bool isMixed, canSet;
}

final class FlarkLinkState {
  const FlarkLinkState(this.resource, this.canSet);
  final InlineResource? resource;
  final bool canSet;
  bool get canRemove => resource != null && canSet;
}

final class FlarkCodeState {
  const FlarkCodeState(this.language, this.canSetLanguage);
  final String? language;
  final bool canSetLanguage;
}

/// Immutable publication. Read again after a controller notification.
final class FlarkState {
  FlarkState({
    required this.markdown,
    required this.revision,
    required this.status,
    required this.error,
    FlarkEditor? editor,
  }) : selection = editor?.selection ?? const FlarkSelection.collapsed(0),
       mode = editor?.sourceMode == true
           ? FlarkMode.source
           : FlarkMode.rendered,
       canUndo = editor?.history.canUndo ?? false,
       canRedo = editor?.history.canRedo ?? false,
       styles = FlarkStylesState(editor),
       heading = _heading(editor),
       link = _link(editor),
       code = _code(editor);
  final String markdown;
  final int revision;
  final FlarkStatus status;
  final Object? error;
  final FlarkSelection selection;
  final FlarkMode mode;
  final bool canUndo, canRedo;
  final FlarkStylesState styles;
  final FlarkHeadingState heading;
  final FlarkLinkState link;
  final FlarkCodeState code;

  static FlarkHeadingState _heading(FlarkEditor? e) {
    if (e == null || e.sourceMode) {
      return const FlarkHeadingState(null, false, false);
    }
    final first = e.document.rowAt(e.selection.start);
    final last = e.document.rowAt(e.selection.end);
    final rows = e.projection.rows.sublist(first.index, last.index + 1);
    final levels = rows.map((row) => row.headingLevel).toSet();
    return FlarkHeadingState(
      levels.length == 1 ? levels.single : null,
      levels.length > 1,
      first.index == last.index &&
          (first.kind == RowKind.paragraph ||
              first.kind == RowKind.heading ||
              (first.kind == RowKind.blank && e.selection.isCollapsed)),
    );
  }

  static FlarkLinkState _link(FlarkEditor? e) {
    if (e == null || e.sourceMode) return const FlarkLinkState(null, false);
    return FlarkLinkState(
      e.document.resourceAt(e.selection, image: false),
      e.canSetResource(),
    );
  }

  static FlarkCodeState _code(FlarkEditor? e) {
    if (e == null || e.sourceMode) return const FlarkCodeState(null, false);
    final row = e.document.caretRow;
    return FlarkCodeState(
      row.fenced
          ? e.source.substring(row.codeInfoStart, row.codeInfoEnd)
          : null,
      row.fenced && e.document.rowAt(e.selection.base).index == row.index,
    );
  }
}
