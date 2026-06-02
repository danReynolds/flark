import '../core/core.dart' show FlarkEditorRuntimeResult, FlarkSourceRange;
import '../markdown/markdown.dart';
import 'sovereign_flutter_controller.dart';

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
