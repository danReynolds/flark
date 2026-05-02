import 'package:flutter/services.dart';

import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_code_highlighter.dart';
import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import 'input_intent_backspace_handler.dart';
import 'input_intent_enter_handler.dart';
import 'input_intent_navigation_handler.dart';
import 'input_intent_tab_handler.dart';

abstract class SovereignInputIntentHost
    implements
        SovereignBackspaceIntentHost,
        SovereignEnterIntentHost,
        SovereignTabIntentHost,
        SovereignNavigationIntentHost {
  @override
  TextEditingValue get value;
  @override
  TextSelection get selection;
  @override
  set selection(TextSelection value);
  @override
  LineIndex get lineIndex;
  @override
  GeometryModel get geometry;
  @override
  List<TextRange> get projectedHiddenRanges;

  @override
  void commitProgrammaticTextEdit(TextEditingValue newValue);
  @override
  bool isCaretInFenceBody(String text, int caret);
  @override
  bool tryHandleIndentedCodeBlockEnter(String text, int caret);
  bool hasTaskMarker(String text, int markerEnd, int lineEnd);
  bool isUnclosedFenceAtEof(String text, MeasuredBlock block);
}

/// Orchestrates user input intents (Enter/Tab/arrows/backspace) while the
/// controller remains the authoritative text model.
class SovereignInputIntentHandler {
  SovereignInputIntentHandler(this._host);

  final SovereignInputIntentHost _host;
  late final SovereignTabIntentHandler _tabHandler = SovereignTabIntentHandler(
    _host,
  );
  late final SovereignEnterIntentHandler _enterHandler =
      SovereignEnterIntentHandler(_host);
  late final SovereignBackspaceIntentHandler _backspaceHandler =
      SovereignBackspaceIntentHandler(_host);
  late final SovereignArrowExitIntentHandler _arrowExitHandler =
      SovereignArrowExitIntentHandler(_host);

  void handleEnter() => _enterHandler.handleEnter();

  bool toggleTaskCheckboxAtSelection() {
    final value = _host.value;
    if (value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid) return false;

    final text = value.text;
    if (text.isEmpty) return false;
    final caret = (sel.isCollapsed ? sel.baseOffset : sel.start).clamp(
      0,
      text.length,
    );
    return toggleTaskCheckboxAtOffset(caret, insertIfList: true);
  }

  bool toggleTaskCheckboxAtOffset(int offset, {bool insertIfList = false}) {
    final value = _host.value;
    if (value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid) return false;

    final text = value.text;
    if (text.isEmpty) return false;
    final caret = offset.clamp(0, text.length);
    if (_host.fenceContextForCaret(text, caret, includeUnclosedEof: true) !=
        null) {
      return false;
    }

    final line = _host.lineIndex.lineAtOffset(caret);
    final lineStart = _host.lineIndex.offsetAtLine(line);
    final lineEnd = (line + 1 < _host.lineIndex.lineCount)
        ? _host.lineIndex.offsetAtLine(line + 1)
        : text.length;
    final lineContentEnd =
        (lineEnd > lineStart && text.codeUnitAt(lineEnd - 1) == 10)
            ? lineEnd - 1
            : lineEnd;
    final marker = _tabHandler.listMarkerForTabLine(
      text,
      lineStart,
      lineContentEnd,
    );
    if (marker == null) return false;

    final markerEnd = marker.markerEnd;
    final hasTask = _host.hasTaskMarker(text, markerEnd, lineContentEnd);
    if (hasTask) {
      final stateOffset = markerEnd + 1;
      if (stateOffset < 0 || stateOffset >= text.length) return false;
      final oldState = text.codeUnitAt(stateOffset);
      final newState = (oldState == 120 || oldState == 88) ? ' ' : 'x';
      final newText = text.replaceRange(stateOffset, stateOffset + 1, newState);
      _host.commitProgrammaticTextEdit(
        value.copyWith(
          text: newText,
          selection: sel,
          composing: TextRange.empty,
        ),
      );
      return true;
    }

    if (!insertIfList) return false;

    final insertAt = markerEnd;
    const insert = '[ ] ';
    final newText = text.replaceRange(insertAt, insertAt, insert);
    TextSelection newSelection;
    if (sel.isCollapsed) {
      final offset = sel.baseOffset >= insertAt
          ? sel.baseOffset + insert.length
          : sel.baseOffset;
      newSelection = TextSelection.collapsed(
        offset: offset.clamp(0, newText.length),
      );
    } else {
      int shift(int offset) =>
          offset <= insertAt ? offset : offset + insert.length;
      newSelection = TextSelection(
        baseOffset: shift(sel.baseOffset).clamp(0, newText.length),
        extentOffset: shift(sel.extentOffset).clamp(0, newText.length),
        affinity: sel.affinity,
        isDirectional: sel.isDirectional,
      );
    }

    _host.commitProgrammaticTextEdit(
      value.copyWith(
        text: newText,
        selection: newSelection,
        composing: TextRange.empty,
      ),
    );
    return true;
  }

  bool handleTabKey({required bool reverse}) =>
      _tabHandler.handleTabKey(reverse: reverse);

  bool handleArrowDownKey() => _arrowExitHandler.handleArrowDownKey();

  bool handleArrowUpKey() => _arrowExitHandler.handleArrowUpKey();

  bool tryExitFencedCodeOnArrowDown() =>
      _arrowExitHandler.tryExitFencedCodeOnArrowDown();

  bool tryExitBlockquoteOnArrowDown() =>
      _arrowExitHandler.tryExitBlockquoteOnArrowDown();

  bool tryExitFencedCodeOnArrowUp() =>
      _arrowExitHandler.tryExitFencedCodeOnArrowUp();

  bool tryExitBlockquoteOnArrowUp() =>
      _arrowExitHandler.tryExitBlockquoteOnArrowUp();

  bool tryExitFencedCodeOnEnter() => _enterHandler.tryExitFencedCodeOnEnter();

  TextEditingValue maybeReenterInlineWrapperOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeReenterInlineWrapperOnBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeDeleteFencedPairOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeDeleteFencedPairOnBackspace(oldValue, newValue);

  TextEditingValue maybeOutdentFencedCodeOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeOutdentFencedCodeOnBackspace(oldValue, newValue);

  TextEditingValue maybeOutdentIndentedCodeOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeOutdentIndentedCodeOnBackspace(oldValue, newValue);

  TextEditingValue maybeCollapseEmptyFenceOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeCollapseEmptyFenceOnBackspace(oldValue, newValue);

  TextEditingValue maybeProtectEmptyFenceEntryBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeProtectEmptyFenceEntryBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeProtectHiddenFenceBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _backspaceHandler.maybeProtectHiddenFenceBackspace(oldValue, newValue);

  bool setFencedCodeLanguageForSelection(String? fenceTag) {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final caret = sel.baseOffset;
    final text = _host.value.text;
    if (caret < 0 || caret > text.length) return false;

    MeasuredBlock? containing;
    for (final block in _host.geometry.codeBlocks) {
      final inside = caret >= block.startOffset && caret < block.endOffset;
      final atUnclosedEofEnd =
          caret == block.endOffset && _host.isUnclosedFenceAtEof(text, block);
      if (inside || atUnclosedEofEnd) {
        containing = block;
        break;
      }
    }
    if (containing == null) return false;

    final start = containing.startOffset;
    if (start < 0 ||
        start + 3 > text.length ||
        !text.startsWith('```', start)) {
      return false;
    }

    final openLineEnd = FencedCodeScanner.endOfLine(text, start);
    final openLineContentEnd =
        (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
            ? openLineEnd - 1
            : openLineEnd;

    final infoStart = (start + 3).clamp(0, text.length);
    final infoEnd = openLineContentEnd.clamp(infoStart, text.length);

    bool isWs(int cu) => cu == 32 || cu == 9;

    var tokenStart = infoStart;
    while (tokenStart < infoEnd && isWs(text.codeUnitAt(tokenStart))) {
      tokenStart++;
    }
    var tokenEnd = tokenStart;
    while (tokenEnd < infoEnd && !isWs(text.codeUnitAt(tokenEnd))) {
      tokenEnd++;
    }

    final existingToken =
        tokenStart < tokenEnd ? text.substring(tokenStart, tokenEnd) : '';
    final existingNormalized = existingToken.isNotEmpty
        ? SovereignCodeHighlighter.normalizeFenceTag(
            existingToken.trim().toLowerCase(),
          )
        : null;
    final hasLanguageTag = existingNormalized != null;

    final newTag = (fenceTag ?? '').trim();
    final wantsLanguage = newTag.isNotEmpty;

    int replaceStart;
    int replaceEnd;
    String replacement;

    if (!wantsLanguage) {
      if (!hasLanguageTag) return false;

      replaceStart = infoStart;
      replaceEnd = tokenEnd;
      if (replaceEnd < infoEnd && isWs(text.codeUnitAt(replaceEnd))) {
        replaceEnd++;
      }
      replacement = '';
    } else {
      if (hasLanguageTag) {
        replaceStart = tokenStart;
        replaceEnd = tokenEnd;
        replacement = newTag;
      } else {
        final remainder = text.substring(tokenStart, infoEnd);
        replaceStart = infoStart;
        replaceEnd = infoEnd;
        replacement = remainder.isEmpty ? newTag : '$newTag $remainder';
      }
    }

    final oldSegment = text.substring(replaceStart, replaceEnd);
    if (oldSegment == replacement) return false;

    final newText = text.replaceRange(replaceStart, replaceEnd, replacement);

    int shiftOffset(int offset) {
      if (offset <= replaceStart) return offset;
      if (offset >= replaceEnd) {
        return offset + (replacement.length - oldSegment.length);
      }
      return replaceStart + replacement.length;
    }

    final newSel = TextSelection(
      baseOffset: shiftOffset(sel.baseOffset),
      extentOffset: shiftOffset(sel.extentOffset),
      affinity: sel.affinity,
      isDirectional: sel.isDirectional,
    );

    final desiredOffset = newSel.baseOffset.clamp(0, newText.length);
    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: newSel,
        composing: TextRange.empty,
      ),
    );

    if (_host.selection.isValid && _host.selection.isCollapsed) {
      final actual = _host.selection.baseOffset;
      if (actual != desiredOffset) {
        _host.selection = TextSelection.collapsed(offset: desiredOffset);
      }
    }
    return true;
  }
}
