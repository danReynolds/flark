import '../core/core.dart'
    show SovereignEditorRuntimeResult, SovereignSourceRange;
import '../markdown/markdown.dart';
import 'sovereign_flutter_controller.dart';

/// High-level Markdown editing helpers for [SovereignFlutterController].
///
/// These helpers are intentionally thin wrappers around the command layer.
/// They keep toolbar and menu code readable while preserving the normal
/// command result for callers that need rejection details.
extension SovereignMarkdownControllerCommands on SovereignFlutterController {
  SovereignEditorRuntimeResult setHeadingLevel(
    int level, {
    String userEvent = 'command.setHeadingLevel',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.setHeadingLevel,
      payload: SovereignSetHeadingLevelPayload(level, userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult clearHeading({
    String userEvent = 'command.clearHeading',
  }) {
    return setHeadingLevel(0, userEvent: userEvent);
  }

  SovereignEditorRuntimeResult toggleInlineStyle(
    SovereignMarkdownInlineStyle style, {
    String userEvent = 'command.toggleInlineStyle',
  }) {
    return dispatch(
      command: SovereignMarkdownInlineCommands.toggleInlineStyle,
      payload: SovereignToggleInlineStylePayload(style, userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult toggleStrong({
    String userEvent = 'command.toggleStrong',
  }) {
    return toggleInlineStyle(
      SovereignMarkdownInlineStyle.strong,
      userEvent: userEvent,
    );
  }

  SovereignEditorRuntimeResult toggleEmphasis({
    String userEvent = 'command.toggleEmphasis',
  }) {
    return toggleInlineStyle(
      SovereignMarkdownInlineStyle.emphasis,
      userEvent: userEvent,
    );
  }

  SovereignEditorRuntimeResult toggleInlineCode({
    String userEvent = 'command.toggleInlineCode',
  }) {
    return toggleInlineStyle(
      SovereignMarkdownInlineStyle.inlineCode,
      userEvent: userEvent,
    );
  }

  SovereignEditorRuntimeResult toggleStrikethrough({
    String userEvent = 'command.toggleStrikethrough',
  }) {
    return toggleInlineStyle(
      SovereignMarkdownInlineStyle.strikethrough,
      userEvent: userEvent,
    );
  }

  SovereignEditorRuntimeResult toggleQuote({
    String userEvent = 'command.toggleQuote',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.toggleQuote,
      payload: SovereignToggleQuotePayload(userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult toggleBulletList({
    String userEvent = 'command.toggleBulletList',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.toggleBulletList,
      payload: SovereignToggleBulletListPayload(userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult toggleOrderedList({
    int startNumber = 1,
    String userEvent = 'command.toggleOrderedList',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.toggleOrderedList,
      payload: SovereignToggleOrderedListPayload(
        startNumber: startNumber,
        userEvent: userEvent,
      ),
    );
  }

  SovereignEditorRuntimeResult toggleTaskList({
    String userEvent = 'command.toggleTaskList',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.toggleTaskList,
      payload: SovereignToggleTaskListPayload(userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult insertThematicBreak({
    String userEvent = 'command.insertThematicBreak',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.insertThematicBreak,
      payload: SovereignInsertThematicBreakPayload(userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult insertCodeFence({
    String? language,
    String userEvent = 'command.insertFence',
  }) {
    return dispatch(
      command: SovereignMarkdownBlockCommands.insertFence,
      payload: SovereignInsertFencePayload(
        language: language,
        userEvent: userEvent,
      ),
    );
  }

  SovereignEditorRuntimeResult insertTable({
    int columns = 2,
    int bodyRows = 1,
    String userEvent = 'command.insertTable',
  }) {
    return dispatch(
      command: SovereignMarkdownTableCommands.insertTable,
      payload: SovereignInsertTablePayload(
        columns: columns,
        bodyRows: bodyRows,
        userEvent: userEvent,
      ),
    );
  }

  SovereignMarkdownLinkEditContext resolveLinkEditContext() {
    return SovereignMarkdownLinkCommands.resolveLinkEditContext(state);
  }

  SovereignEditorRuntimeResult insertLink({
    String userEvent = 'command.insertLink',
  }) {
    return dispatch(
      command: SovereignMarkdownLinkCommands.insertLink,
      payload: SovereignInsertLinkPayload(userEvent: userEvent),
    );
  }

  SovereignEditorRuntimeResult applyLinkEdit({
    required SovereignMarkdownLinkEditContext context,
    required String label,
    required String url,
    String userEvent = 'command.applyLinkEdit',
  }) {
    return dispatch(
      command: SovereignMarkdownLinkCommands.applyLinkEdit,
      payload: SovereignApplyLinkEditPayload(
        context: context,
        label: label,
        url: url,
        userEvent: userEvent,
      ),
    );
  }

  SovereignEditorRuntimeResult removeLink({
    required SovereignSourceRange linkRange,
    String userEvent = 'command.removeLink',
  }) {
    return dispatch(
      command: SovereignMarkdownLinkCommands.removeLink,
      payload: SovereignRemoveLinkPayload(
        linkRange: linkRange,
        userEvent: userEvent,
      ),
    );
  }
}
