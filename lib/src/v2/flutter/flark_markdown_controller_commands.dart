import '../core/core.dart'
    show FlarkCommandResult, FlarkEditorRuntimeResult, FlarkSourceRange;
import '../markdown/markdown.dart';
import 'flark_flutter_controller.dart';

/// Concise Markdown command facade for [FlarkFlutterController].
///
/// Use `controller.commands` for toolbar/menu wiring. Read getters expose the
/// current selection state, while mutation methods dispatch the same command
/// layer as keyboard shortcuts and direct controller helpers.
final class FlarkMarkdownCommands {
  const FlarkMarkdownCommands(this._controller);

  final FlarkFlutterController _controller;

  FlarkMarkdownCommandCapabilities get _state {
    return FlarkMarkdownCommandQueries.capabilitiesAtSelection(
      _controller.state,
      pendingInlineStyles: _controller.pendingInlineStyles,
      mutedInlineStyles: _controller.mutedInlineStyles,
    );
  }

  bool get canMutate => _state.canMutate;

  bool get canUndo => _controller.runtime.canUndo;

  bool get canRedo => _controller.runtime.canRedo;

  bool get strongActive {
    return isInlineActive(FlarkMarkdownInlineStyle.strong);
  }

  bool get emphasisActive {
    return isInlineActive(FlarkMarkdownInlineStyle.emphasis);
  }

  bool get inlineCodeActive {
    return isInlineActive(FlarkMarkdownInlineStyle.inlineCode);
  }

  bool get strikethroughActive {
    return isInlineActive(FlarkMarkdownInlineStyle.strikethrough);
  }

  int? get headingLevel => _state.activeHeadingLevel;

  bool get quoteActive => _state.quoteActive;

  bool get bulletListActive => _state.bulletListActive;

  bool get orderedListActive => _state.orderedListActive;

  bool get taskListActive => _state.taskListActive;

  bool get tableActive => _state.tableActive;

  bool isInlineActive(FlarkMarkdownInlineStyle style) {
    return _state.isInlineStyleActive(style);
  }

  FlarkEditorRuntimeResult undo() => _controller.undo();

  FlarkEditorRuntimeResult redo() => _controller.redo();

  FlarkEditorRuntimeResult setHeadingLevel(
    int level, {
    String userEvent = 'command.setHeadingLevel',
  }) {
    return _controller.setHeadingLevel(level, userEvent: userEvent);
  }

  FlarkEditorRuntimeResult clearHeading({
    String userEvent = 'command.clearHeading',
  }) {
    return _controller.clearHeading(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleInlineStyle(
    FlarkMarkdownInlineStyle style, {
    String userEvent = 'command.toggleInlineStyle',
  }) {
    return _controller.toggleInlineStyle(style, userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleStrong({
    String userEvent = 'command.toggleStrong',
  }) {
    return _controller.toggleStrong(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleEmphasis({
    String userEvent = 'command.toggleEmphasis',
  }) {
    return _controller.toggleEmphasis(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleInlineCode({
    String userEvent = 'command.toggleInlineCode',
  }) {
    return _controller.toggleInlineCode(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleStrikethrough({
    String userEvent = 'command.toggleStrikethrough',
  }) {
    return _controller.toggleStrikethrough(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleQuote({
    String userEvent = 'command.toggleQuote',
  }) {
    return _controller.toggleQuote(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleBulletList({
    String userEvent = 'command.toggleBulletList',
  }) {
    return _controller.toggleBulletList(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleOrderedList({
    int startNumber = 1,
    String userEvent = 'command.toggleOrderedList',
  }) {
    return _controller.toggleOrderedList(
      startNumber: startNumber,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult toggleTaskList({
    String userEvent = 'command.toggleTaskList',
  }) {
    return _controller.toggleTaskList(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult insertThematicBreak({
    String userEvent = 'command.insertThematicBreak',
  }) {
    return _controller.insertThematicBreak(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult insertCodeFence({
    String? language,
    String userEvent = 'command.insertFence',
  }) {
    return _controller.insertCodeFence(
      language: language,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult insertTable({
    int columns = 2,
    int bodyRows = 1,
    String userEvent = 'command.insertTable',
  }) {
    return _controller.insertTable(
      columns: columns,
      bodyRows: bodyRows,
      userEvent: userEvent,
    );
  }

  FlarkMarkdownLinkEditContext resolveLinkEditContext() {
    return _controller.resolveLinkEditContext();
  }

  FlarkEditorRuntimeResult insertLink({
    String userEvent = 'command.insertLink',
  }) {
    return _controller.insertLink(userEvent: userEvent);
  }

  FlarkEditorRuntimeResult applyLinkEdit({
    required FlarkMarkdownLinkEditContext context,
    required String label,
    required String url,
    String userEvent = 'command.applyLinkEdit',
  }) {
    return _controller.applyLinkEdit(
      context: context,
      label: label,
      url: url,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult removeLink({
    required FlarkSourceRange linkRange,
    String userEvent = 'command.removeLink',
  }) {
    return _controller.removeLink(linkRange: linkRange, userEvent: userEvent);
  }
}

extension FlarkMarkdownControllerCommandFacade on FlarkFlutterController {
  FlarkMarkdownCommands get commands => FlarkMarkdownCommands(this);
}

/// High-level Markdown editing helpers for [FlarkFlutterController].
///
/// These helpers are intentionally thin wrappers around the command layer.
/// They keep toolbar and menu code readable while preserving the normal
/// command result for callers that need rejection details.
extension FlarkMarkdownControllerCommands on FlarkFlutterController {
  FlarkEditorRuntimeResult setHeadingLevel(
    int level, {
    String userEvent = 'command.setHeadingLevel',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.setHeadingLevel,
      payload: FlarkSetHeadingLevelPayload(level, userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult clearHeading({
    String userEvent = 'command.clearHeading',
  }) {
    return setHeadingLevel(0, userEvent: userEvent);
  }

  FlarkEditorRuntimeResult toggleInlineStyle(
    FlarkMarkdownInlineStyle style, {
    String userEvent = 'command.toggleInlineStyle',
  }) {
    if (state.selection.isCollapsed) {
      // Caret inside an existing run of this style → arm it off (muted): the
      // text already written stays styled and the next typed character leaves
      // or splits the run. Otherwise → arm it on (pending) so the next typed
      // run is wrapped. Neither edits the document here.
      final sourceActive = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      ).isInlineStyleActive(style);
      if (sourceActive) {
        toggleMutedInlineStyle(style);
      } else if (wouldArmInlineStyleApply(style)) {
        togglePendingInlineStyle(style);
      }
      // Otherwise the style cannot be applied at this caret — its marker would
      // merge with an adjacent marker (e.g. arming italic at a bold run's
      // trailing edge, where `**a*b***` is not representable). Arming it would
      // light the toolbar for a style the next keystroke silently drops, so we
      // leave it unarmed and report the command handled (a no-op).
      return FlarkEditorRuntimeResult(
        runtime: runtime,
        commandResult: FlarkCommandResult.handled(),
      );
    }
    return dispatch(
      command: FlarkMarkdownInlineCommands.toggleInlineStyle,
      payload: FlarkToggleInlineStylePayload(style, userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult toggleStrong({
    String userEvent = 'command.toggleStrong',
  }) {
    return toggleInlineStyle(
      FlarkMarkdownInlineStyle.strong,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult toggleEmphasis({
    String userEvent = 'command.toggleEmphasis',
  }) {
    return toggleInlineStyle(
      FlarkMarkdownInlineStyle.emphasis,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult toggleInlineCode({
    String userEvent = 'command.toggleInlineCode',
  }) {
    return toggleInlineStyle(
      FlarkMarkdownInlineStyle.inlineCode,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult toggleStrikethrough({
    String userEvent = 'command.toggleStrikethrough',
  }) {
    return toggleInlineStyle(
      FlarkMarkdownInlineStyle.strikethrough,
      userEvent: userEvent,
    );
  }

  FlarkEditorRuntimeResult toggleQuote({
    String userEvent = 'command.toggleQuote',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.toggleQuote,
      payload: FlarkToggleQuotePayload(userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult toggleBulletList({
    String userEvent = 'command.toggleBulletList',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.toggleBulletList,
      payload: FlarkToggleBulletListPayload(userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult toggleOrderedList({
    int startNumber = 1,
    String userEvent = 'command.toggleOrderedList',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.toggleOrderedList,
      payload: FlarkToggleOrderedListPayload(
        startNumber: startNumber,
        userEvent: userEvent,
      ),
    );
  }

  FlarkEditorRuntimeResult toggleTaskList({
    String userEvent = 'command.toggleTaskList',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.toggleTaskList,
      payload: FlarkToggleTaskListPayload(userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult insertThematicBreak({
    String userEvent = 'command.insertThematicBreak',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.insertThematicBreak,
      payload: FlarkInsertThematicBreakPayload(userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult insertCodeFence({
    String? language,
    String userEvent = 'command.insertFence',
  }) {
    return dispatch(
      command: FlarkMarkdownBlockCommands.insertFence,
      payload: FlarkInsertFencePayload(
        language: language,
        userEvent: userEvent,
      ),
    );
  }

  FlarkEditorRuntimeResult insertTable({
    int columns = 2,
    int bodyRows = 1,
    String userEvent = 'command.insertTable',
  }) {
    return dispatch(
      command: FlarkMarkdownTableCommands.insertTable,
      payload: FlarkInsertTablePayload(
        columns: columns,
        bodyRows: bodyRows,
        userEvent: userEvent,
      ),
    );
  }

  FlarkMarkdownLinkEditContext resolveLinkEditContext() {
    return FlarkMarkdownLinkCommands.resolveLinkEditContext(state);
  }

  FlarkEditorRuntimeResult insertLink({
    String userEvent = 'command.insertLink',
  }) {
    return dispatch(
      command: FlarkMarkdownLinkCommands.insertLink,
      payload: FlarkInsertLinkPayload(userEvent: userEvent),
    );
  }

  FlarkEditorRuntimeResult applyLinkEdit({
    required FlarkMarkdownLinkEditContext context,
    required String label,
    required String url,
    String userEvent = 'command.applyLinkEdit',
  }) {
    return dispatch(
      command: FlarkMarkdownLinkCommands.applyLinkEdit,
      payload: FlarkApplyLinkEditPayload(
        context: context,
        label: label,
        url: url,
        userEvent: userEvent,
      ),
    );
  }

  FlarkEditorRuntimeResult removeLink({
    required FlarkSourceRange linkRange,
    String userEvent = 'command.removeLink',
  }) {
    return dispatch(
      command: FlarkMarkdownLinkCommands.removeLink,
      payload: FlarkRemoveLinkPayload(
        linkRange: linkRange,
        userEvent: userEvent,
      ),
    );
  }
}
