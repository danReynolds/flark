import 'dart:async';
import 'package:sovereign_editor/src/helpers/logger.dart';
import 'package:flutter/widgets.dart';

import '../models/sovereign_state.dart';
import '../models/edit_op.dart';
import '../models/decoration_model.dart';
import '../models/line_index.dart';
import '../models/geometry_model.dart';
import '../models/block_tree.dart';
import '../engine/syntax_engine.dart';
import '../engine/syntax_engine_factory.dart';
import '../engine/syntax_parse_scheduler.dart';
import '../engine/syntax_snapshot.dart';
import '../engine/syntax_types.dart';

import 'undo_stack.dart';
import '../logic/sovereign_style_scanner.dart';
import '../logic/sovereign_geometry_scanner.dart';
import '../logic/fenced_code_scanner.dart';
import '../logic/markdown_marker_grammar.dart';
import '../logic/projector.dart';
import '../core/state/editor_session_state.dart';
import '../core/state/editor_session_state_builder.dart';
import '../core/syntax/selection_projection_guard.dart';
import '../core/syntax/selection_mask_utils.dart';
import '../core/syntax/projection_range_utils.dart';
import '../core/syntax/syntax_projection_coordinator.dart';
import '../core/syntax/predictive_edit_range_utils.dart';
import '../core/pipeline/value_mutation_coordinator.dart';
import '../core/pipeline/undo_grouping_policy.dart';
import '../core/pipeline/edit_operation_pipeline.dart';
import '../core/pipeline/edit_differ.dart';
import '../core/intents/input_intent_handler.dart';
import '../core/intents/input_intent_models.dart';
import '../core/rendering/sovereign_text_renderer.dart';
import '../core/structure/markdown_line_helpers.dart';
import '../core/structure/fence/fence_editing_utils.dart';
import '../core/structure/indented_code/indented_code_enter_service.dart';
import '../core/structure/navigation/navigation_line_utils.dart';
import '../core/structure/navigation/vertical_caret_navigation.dart';
import '../core/structure/table/table_line_parser.dart';
import '../core/structure/table/table_navigation_service.dart';
import '../core/structure/table/table_tab_intent_service.dart';
import '../core/syntax/syntax_snapshot_mapper.dart';
import '../core/structure/models/fence_context.dart' as structure;
import '../core/structure/models/list_marker_context.dart' as structure;
import '../core/structure/models/quote_context.dart' as structure;
import 'sovereign_navigation_helpers.dart';

part 'sovereign_controller_policies.dart';
part 'sovereign_controller_policies_fence.dart';
part 'sovereign_controller_policies_fence_navigation.dart';
part 'sovereign_controller_policies_fence_pairing.dart';
part 'sovereign_controller_policies_fence_backspace.dart';
part 'sovereign_controller_policies_quote.dart';
part 'sovereign_controller_policies_link.dart';
part 'sovereign_controller_policies_list.dart';
part 'sovereign_controller_policies_table.dart';
part 'sovereign_controller_policies_heading.dart';
part 'sovereign_value_mutation_coordinator.dart';
part 'sovereign_input_intent_handler.dart';
part 'sovereign_syntax_sync_coordinator.dart';
part 'sovereign_controller_diagnostics.dart';
part 'sovereign_table_tab_intent_host.dart';

class SovereignController extends TextEditingController {
  final SovereignTextRenderer _renderer = SovereignTextRenderer();
  final SelectionProjectionGuard _selectionProjectionGuard =
      const DefaultSelectionProjectionGuard();
  final SovereignNavigationHelpers _navigationHelpers =
      const SovereignNavigationHelpers();
  final IndentedCodeEnterService _indentedCodeEnterService =
      const IndentedCodeEnterService();
  late final SovereignValueMutationCoordinator _valueMutation =
      SovereignValueMutationCoordinator(
    _ControllerSovereignValueMutationHost(this),
  );
  late final SovereignInputIntentHandler _inputIntents =
      SovereignInputIntentHandler(_ControllerSovereignInputIntentHost(this));
  late final SovereignSyntaxSyncCoordinator _syntaxSync =
      SovereignSyntaxSyncCoordinator(_ControllerSovereignSyntaxSyncHost(this));
  late final TableTabIntentService _tableTabIntents = TableTabIntentService(
    _ControllerTableTabIntentHost(this),
  );
  static final Logger _logger = Logger('SovereignController');

  // --- State ---

  /// Internal mutable state for revision tracking
  int _revision = 0;
  int _nextOpId = 0;

  /// Syntax Engine boundary (Phase 1).
  final SyntaxEngine _syntaxEngine;
  final MarkdownSyntaxProfile _markdownProfile;
  SyntaxParseScheduler? _syntaxParseScheduler;

  /// The Canonical State
  SovereignState get state => SovereignState(value: value, revision: _revision);

  /// Decoration Stream (Decoupled Layer)
  final _decorationController = StreamController<DecorationModel>.broadcast();
  Stream<DecorationModel> get decorationStream => _decorationController.stream;

  /// Tier 2 Rendering State (Inline Styles)
  List<StyleRun>? _authoritativeInlineRuns;
  int _authoritativeInlineRunsRevision = -1;

  /// [Phase 5] Projection State
  List<TextRange> _projectedHiddenRanges = [];
  List<TextRange> _projectedExclusionRanges = [];
  List<TextRange> _authoritativeHiddenRanges = [];
  List<TextRange> _authoritativeExclusionRanges = [];
  CursorValidationMask _projectedCursorMask =
      const PassthroughCursorValidationMask(textLength: 0);
  CursorValidationMask _authoritativeCursorMask =
      const PassthroughCursorValidationMask(textLength: 0);
  SyntaxSnapshot? _latestAuthoritativeSnapshot;
  CursorValidationMask _activeCursorMask =
      const PassthroughCursorValidationMask(textLength: 0);

  // Predictive scan tuning. Defaults match scanner defaults and can be
  // overridden in tests to deterministically exercise budgeted paths.
  int _predictiveScanTimeBudgetMicros = SovereignStyleScanner.kTimeBudgetMicros;
  int _predictiveScanSpanBudget = SovereignStyleScanner.kSpanBudget;
  int? _predictiveScanCharLimitOverride;
  int _predictiveBudgetExhaustionCount = 0;
  int _predictiveLocalFallbackCount = 0;
  int _predictiveLocalFallbackLastScannedChars = 0;

  /// Local cache of latest decoration for synchronous reads
  DecorationModel _latestDecoration = DecorationModel.empty();
  DecorationModel get decoration => _latestDecoration;

  // --- Internal State ---
  late LineIndex _lineIndex;

  // RFC 007: Synchronous Geometry Model
  GeometryModel _geometry = GeometryModel.empty;
  final _geometryScanner = const SovereignGeometryScanner();
  int? _preferredVerticalCaretColumn;
  bool _isApplyingVerticalCaretMove = false;
  int _suppressFenceExitOnEnterDepth = 0;

  // --- Public Getters ---
  LineIndex get lineIndex => _lineIndex;
  GeometryModel get geometry => _geometry;
  EditOp? get lastOp => _lastOp;

  /// [Phase 5] Projector
  late Projector _projector = Projector(_latestDecoration);

  /// Helpers
  final UndoStack _undoStack = UndoStack();
  // Redo history is stored inside UndoStack; no duplicate stack here.

  // Undo Merging State
  DateTime? _lastOpTime;
  EditOp? _lastOp;
  int _currentUndoGroup = 0;
  int _undoBoundaryDepth = 0;
  int _commandTransactionDepth = 0;
  int? _commandTransactionUndoGroupId;
  bool _forceUndoBoundaryForNextTextOp = false;
  TextEditingValue? _compositionStartValue;

  SovereignController({
    String? text,
    SyntaxEngine? syntaxEngine,
    MarkdownSyntaxProfile markdownProfile =
        MarkdownSyntaxProfile.commonMarkCore,
  }) : this._(
          text: text,
          syntaxEngine: syntaxEngine,
          markdownProfile: markdownProfile,
          bootstrapDecoration: false,
        );

  SovereignController.readOnly({
    required String text,
    SyntaxEngine? syntaxEngine,
    MarkdownSyntaxProfile markdownProfile =
        MarkdownSyntaxProfile.commonMarkCore,
  }) : this._(
          text: text,
          syntaxEngine: syntaxEngine,
          markdownProfile: markdownProfile,
          bootstrapDecoration: true,
        );

  SovereignController._({
    String? text,
    SyntaxEngine? syntaxEngine,
    required MarkdownSyntaxProfile markdownProfile,
    required bool bootstrapDecoration,
  })  : _markdownProfile = markdownProfile,
        _syntaxEngine = syntaxEngine ?? SyntaxEngineFactory.create(),
        super(text: text) {
    if (text != null && text.isNotEmpty) {
      // Initialize LineIndex
      _lineIndex = LineIndex.fromText(text);
      // Initialize Geometry
      _geometry = _geometryScanner.scan(text, _lineIndex);
    } else {
      _lineIndex = LineIndex.fromText(''); // Initialize with empty if no text
    }

    final initialMask = PassthroughCursorValidationMask(
      textLength: value.text.length,
    );
    _projectedCursorMask = initialMask;
    _authoritativeCursorMask = initialMask;
    _activeCursorMask = initialMask;

    // Initialize Async Parser
    _initParser(text);

    // Bootstrap decoration synchronously so non-edit surfaces (for example
    // read-only markdown views) render projected markdown immediately even
    // before authoritative parse snapshots arrive.
    if (bootstrapDecoration && value.text.isNotEmpty) {
      _emitDecoration(tree: _latestDecoration.tree, overrideValue: value);
    }
  }

  void _initParser(String? initialText) {
    _syntaxParseScheduler = SyntaxParseScheduler(
      runParse: _syntaxEngine.parse,
      onSnapshot: _handleSyntaxSnapshot,
      onError: (error, stackTrace, request) {
        _logger.log(
          'Sovereign syntax parse failed (revision ${request.revision}): $error',
        );
      },
    );

    // Initial request if text exists.
    if (initialText != null && initialText.isNotEmpty) {
      _scheduleParse(
        initialText,
        _revision,
        currentValue: TextEditingValue(text: initialText),
      );
    }
  }

  @override
  void dispose() {
    assert(() {
      final pendingReplace = parsePendingReplaceCount;
      final staleDrop = parseStaleDropCount;
      if (pendingReplace > 0 || staleDrop > 0) {
        _logger.log(
          'Sovereign parse telemetry '
          '(pendingReplace=$pendingReplace, staleDrop=$staleDrop)',
        );
      }
      return true;
    }());
    _syntaxParseScheduler?.dispose();
    _decorationController.close();
    super.dispose();
  }

  // --- Mutation ---

  @override
  set selection(TextSelection newSelection) {
    if (newSelection == selection) return;
    if (!_isApplyingVerticalCaretMove) {
      _preferredVerticalCaretColumn = null;
    }

    final snapped = _selectionProjectionGuard.projectAndSnap(
      requested: newSelection,
      previous: selection,
      textLength: value.text.length,
      projector: _projector,
      mask: _activeCursorMask,
    );

    super.selection = snapped;
    _updateProjection(overrideValue: value.copyWith(selection: snapped));
  }

  @override
  set value(TextEditingValue newValue) {
    _valueMutation.applyIncomingValue(newValue);
  }

  void _setControllerSuperValue(TextEditingValue newValue) {
    super.value = newValue;
  }

  TextSelection _snapSelectionWithCursorMask(
    TextSelection selection, {
    required int textLength,
    CursorValidationMask? mask,
  }) =>
      SelectionMaskUtils.snapSelectionWithCursorMask(
        selection,
        textLength: textLength,
        mask: mask ?? _activeCursorMask,
      );

  CursorValidationMask _normalizeCursorMaskToText(
    CursorValidationMask mask, {
    required int textLength,
    List<TextRange> fallbackHiddenRanges = const [],
  }) =>
      SelectionMaskUtils.normalizeCursorMaskToText(
        mask,
        textLength: textLength,
        fallbackHiddenRanges: fallbackHiddenRanges,
      );

  TextEditingValue _normalizeProjectedSelectAllDelete(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid || oldSel.isCollapsed) {
      return newValue;
    }
    if (!newSel.isCollapsed || newSel.baseOffset != 0) return newValue;
    if (oldSel.start != 0) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.isEmpty || oldSel.end <= 0 || oldSel.end >= oldText.length) {
      return newValue;
    }

    final expectedDelete = oldText.replaceRange(oldSel.start, oldSel.end, '');
    if (newText != expectedDelete) return newValue;

    final hidden = ProjectionRangeUtils.normalizeHiddenRanges(
      _projectedHiddenRanges,
      oldText.length,
    );
    if (hidden.isEmpty) return newValue;
    var hiddenLen = 0;
    for (final range in hidden) {
      hiddenLen += range.end - range.start;
    }
    final projectedVisibleLength = oldText.length - hiddenLen;
    if (projectedVisibleLength <= 0 || oldSel.end != projectedVisibleLength) {
      return newValue;
    }

    return const TextEditingValue(
      text: '',
      selection: TextSelection.collapsed(offset: 0),
      composing: TextRange.empty,
    );
  }

  TextEditingValue _applyEditTransformPipeline(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    assert(() {
      for (var i = 1; i < _kEditTransformRules.length; i++) {
        if (_kEditTransformRules[i - 1].priority >
            _kEditTransformRules[i].priority) {
          throw StateError(
            'Edit transform priorities are not sorted: '
            '${_kEditTransformRules[i - 1].name} > '
            '${_kEditTransformRules[i].name}',
          );
        }
      }
      return true;
    }());

    var current = newValue;
    final context = _PolicyContext(
      helpers: _PolicyHelpers(this),
      oldValue: oldValue,
      intent: _PolicyEditIntent.detect(oldValue, newValue),
    );
    for (final rule in _kEditTransformRules) {
      current = rule.apply(context, current);
    }
    return current;
  }

  bool _isCaretInFenceBody(String text, int caret) {
    if (caret < 0 || caret > text.length) return false;
    final context = _fenceContextForCaret(
      text,
      caret,
      includeUnclosedEof: true,
    );
    if (context == null) return false;
    final line = _lineIndex.lineAtOffset(caret.clamp(0, text.length));
    if (line <= context.openLine) return false;
    if (context.closeLine != null && line >= context.closeLine!) return false;
    return true;
  }

  bool _isRangeInFenceBody(String text, int start, int end) {
    if (start < 0 || end < start || end > text.length) return false;
    if (start == end) return _isCaretInFenceBody(text, start);

    final startContext = _fenceContextForCaret(
      text,
      start,
      includeUnclosedEof: true,
    );
    final endContext = _fenceContextForCaret(
      text,
      end - 1,
      includeUnclosedEof: true,
    );
    if (startContext == null || endContext == null) return false;

    if (startContext.block.startOffset != endContext.block.startOffset ||
        startContext.block.endOffset != endContext.block.endOffset) {
      return false;
    }

    return _isCaretInFenceBody(text, start) &&
        _isCaretInFenceBody(text, end - 1);
  }

  bool _shouldAutoPairFencedQuote(String oldText, int caret, int quoteCu) {
    if (caret > 0 && oldText.codeUnitAt(caret - 1) == 92) return false; // \
    if (caret >= oldText.length) return true;

    final next = oldText.codeUnitAt(caret);
    if (next == quoteCu) return true;
    if (next == 32 || next == 9 || next == 10 || next == 13) return true;
    if (next == 41 || next == 93 || next == 125 || next == 44 || next == 46) {
      return true;
    }
    return false;
  }

  UndoGroupingState _undoGroupingStateSnapshot() {
    return UndoGroupingState(
      currentUndoGroup: _currentUndoGroup,
      commandTransactionUndoGroupId: _commandTransactionUndoGroupId,
      lastOp: _lastOp,
      lastOpTime: _lastOpTime,
      commandTransactionDepth: _commandTransactionDepth,
      undoBoundaryDepth: _undoBoundaryDepth,
      forceUndoBoundaryForNextTextOp: _forceUndoBoundaryForNextTextOp,
    );
  }

  void _applyUndoGroupingState(UndoGroupingState state) {
    _currentUndoGroup = state.currentUndoGroup;
    _commandTransactionUndoGroupId = state.commandTransactionUndoGroupId;
    _lastOp = state.lastOp;
    _lastOpTime = state.lastOpTime;
    _forceUndoBoundaryForNextTextOp = state.forceUndoBoundaryForNextTextOp;
  }

  EditOp _createOp(
    TextEditingValue oldVal,
    TextEditingValue newVal, {
    TextEditingValue? undoBeforeOverride,
  }) {
    final result = EditOperationPipeline.create(
      oldValue: oldVal,
      newValue: newVal,
      nextOpId: _nextOpId,
      undoGroupingState: _undoGroupingStateSnapshot(),
      now: DateTime.now(),
      undoBeforeOverride: undoBeforeOverride,
    );
    _nextOpId = result.nextOpId;
    _applyUndoGroupingState(result.undoGroupingState);
    return result.op;
  }

  void _applyOp(EditOp op) {
    if (op.kind == EditOpKind.text) {
      _revision++;
      // _undoStack.push(op); // Moved to after check

      final newText = op.after.text;

      // 1. Update LineIndex (Already synchronous)
      _lineIndex = LineIndex.fromText(newText);

      // 2. RFC 007: Update Geometry (Synchronous)
      // This ensures Backgrounds are 100% in sync with Text & LineIndex.
      _geometry = _geometryScanner.scan(newText, _lineIndex);

      // 3. Schedule Async Parse (Authoritative)
      _scheduleParse(newText, _revision, currentValue: op.after);

      _lastOp = op; // Update last operation
      _lastOpTime = DateTime.now();

      // Clear redo stack on new edit
      _undoStack.clearRedo();
    } else {
      // For selection/composing ops, we don't need to re-scan geometry.
    }
    // If selection op, we typically don't parse,
    // BUT we might need to update DecorationModel's cursor-dependent highlights?
    // V1: Block backgrounds don't depend on selection.

    // IME Hardening (Phase 4):
    // If we are composing, do NOT push to undo stack.
    // We only record the final 'committed' state (when composing is invalid).
    if (op.after.composing.isValid) {
      // Do not record
    } else {
      _undoStack.push(op);
    }

    // However, if we updated LineIndex, we MUST emit a new DecorationModel
    // so the Painter gets the new index for Tier 1 mapping.
    if (op.kind == EditOpKind.text) {
      // Pass op.after because super.value hasn't updated yet!
      _emitDecoration(
        tree: _latestDecoration.tree,
        overrideValue: op.after,
        op: op,
      );
    }
  }

  // --- Parsing ---

  void _scheduleParse(
    String text,
    int revision, {
    TextEditingValue? currentValue,
  }) =>
      _syntaxSync.scheduleParse(text, revision, currentValue: currentValue);

  void _handleSyntaxSnapshot(SyntaxSnapshot snapshot) =>
      _syntaxSync.handleSyntaxSnapshot(snapshot);

  void _emitDecoration({
    required BlockTree tree,
    TextEditingValue? overrideValue,
    EditOp? op,
  }) =>
      _syntaxSync.emitDecoration(
        tree: tree,
        overrideValue: overrideValue,
        op: op,
      );

  ParsedTableLine? _parseTableLineAt(String text, int line) {
    return TableNavigationService.parseLineAt(
      text: text,
      line: line,
      lineIndex: _lineIndex,
      isLineInsideFencedGeometry: _isLineInsideFencedGeometry,
      rowShapeResolver: _TablePolicy._matchTableRowShape,
    );
  }

  int? _tableCellIndexForCaret(ParsedTableLine row, int caret) {
    return TableNavigationService.tableCellIndexForCaret(row, caret);
  }

  bool _tableRegionHasSeparator(String text, int line, int columnCount) {
    return TableNavigationService.tableRegionHasSeparator(
      text: text,
      line: line,
      columnCount: columnCount,
      lineIndex: _lineIndex,
      parseLineAt: _parseTableLineAt,
    );
  }

  ParsedTableLine? _findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    bool skipSeparator = false,
  }) {
    return TableNavigationService.findAdjacentTableLine(
      text: text,
      line: line,
      columnCount: columnCount,
      forward: forward,
      skipSeparator: skipSeparator,
      lineIndex: _lineIndex,
      parseLineAt: _parseTableLineAt,
    );
  }

  /// [Phase 5] Re-calculates active hidden ranges based on selection (Pop Scope)
  /// and emits a new DecorationModel if needed.
  void _updateProjection({
    BlockTree? newTree,
    bool treeIsAuthoritative = false,
    TextEditingValue? overrideValue,
    bool suppressPop = false,
  }) =>
      _syntaxSync.updateProjection(
        newTree: newTree,
        treeIsAuthoritative: treeIsAuthoritative,
        overrideValue: overrideValue,
        suppressPop: suppressPop,
      );

  bool _normalizeCommittedSelectionAfterProjection() {
    if (value.composing.isValid) return false;

    final current = value;
    final snapped = _selectionProjectionGuard.projectAndSnap(
      requested: current.selection,
      previous: current.selection,
      textLength: current.text.length,
      projector: _projector,
      mask: _activeCursorMask,
    );
    if (snapped == current.selection) return false;

    final updated = current.copyWith(selection: snapped);
    _preferredVerticalCaretColumn = null;
    super.value = updated;
    _updateProjection(overrideValue: updated);
    return true;
  }

  // --- Undo / Redo ---

  void undo() {
    // Phase 4: Gate Undo during composition
    if (value.composing.isValid) return;
    if (!_undoStack.canUndo) return;

    final ops =
        _undoStack.popUndo(); // Returns [C, B] (Reverse application order)
    if (ops.isEmpty) return;

    // We need to apply the INVERSE of these ops.
    // Op C: A -> B. Inverse: B -> A.
    // We act on 'value'.

    TextEditingValue accumulator = value;

    for (final op in ops) {
      // Ideally: accumulator = op.before
      // But we must chain them carefully if multiple ops.
      // Since the stack is strict LIFO of state snapshots,
      // op.before SHOULD match the state of the previous op's after?
      // Yes, if we track strict state.

      // Verification:
      // Ops: [C, B]. C.after == current. C.before == B.after.
      // So to revert C, we set state to C.before.
      // Then revert B, we set state to B.before.
      // Final state is B.before.

      accumulator = op.before;
    }

    _applyRestoration(accumulator);

    // We do NOT push to UndoStack (we just popped!).
    // We already moved them to Redo stack inside popUndo.
  }

  void redo() {
    // Phase 4: Gate Redo during composition
    if (value.composing.isValid) return;
    if (!_undoStack.canRedo) return;

    final ops = _undoStack.popRedo(); // Returns [B, C] (Forward order)

    TextEditingValue accumulator = value;
    for (final op in ops) {
      accumulator = op.after;
    }

    _applyRestoration(accumulator);
  }

  /// [Patterns] Unified Restoration Pipeline
  /// Used by Undo/Redo to bypass standard edit ops but strictly enforce
  /// synchronous geometry and projection correctness.
  void _applyRestoration(TextEditingValue newValue) {
    _revision++;

    // 1. RFC 007 Sync Geometry
    // Vital for undoing near fences.
    _lineIndex = LineIndex.fromText(newValue.text);
    _geometry = _geometryScanner.scan(newValue.text, _lineIndex);

    // 2. Commit to Framework
    _preferredVerticalCaretColumn = null;
    super.value = newValue;

    // 3. Update Visual Projection
    // Ensure hidden ranges (Pop Scope) are recalculated for the restored selection.
    _emitDecoration(tree: _latestDecoration.tree, overrideValue: newValue);

    // 4. Schedule Parse
    _scheduleParse(newValue.text, _revision, currentValue: newValue);
  }

  // --- Public Smart APIs ---
  T _runWithUndoBoundary<T>(T Function() action) {
    _undoBoundaryDepth++;
    try {
      return action();
    } finally {
      _undoBoundaryDepth--;
    }
  }

  T runInCommandTransaction<T>(T Function() action) {
    final isRoot = _commandTransactionDepth == 0;
    _commandTransactionDepth++;
    if (isRoot) {
      _commandTransactionUndoGroupId = _currentUndoGroup + 1;
    }
    try {
      return action();
    } finally {
      _commandTransactionDepth--;
      if (_commandTransactionDepth == 0) {
        _commandTransactionUndoGroupId = null;
      }
    }
  }

  void _commitProgrammaticTextEdit(TextEditingValue newValue) {
    _runWithUndoBoundary(() {
      value = newValue;
    });
  }

  void handleEnter({bool suppressFenceExit = false}) {
    if (!suppressFenceExit) {
      _inputIntents.handleEnter();
      return;
    }
    _suppressFenceExitOnEnterDepth++;
    try {
      _inputIntents.handleEnter();
    } finally {
      _suppressFenceExitOnEnterDepth--;
    }
  }

  bool handleTabKey({required bool reverse}) =>
      _inputIntents.handleTabKey(reverse: reverse);

  bool toggleTaskCheckboxAtSelection() =>
      _inputIntents.toggleTaskCheckboxAtSelection();

  bool toggleTaskCheckboxAtOffset(int offset, {bool insertIfList = false}) =>
      _inputIntents.toggleTaskCheckboxAtOffset(
        offset,
        insertIfList: insertIfList,
      );

  TextRange? taskCheckboxMarkerRangeForLine(int line) {
    final info = _taskCheckboxLineInfoForLine(line);
    if (info == null) return null;
    return TextRange(start: info.taskStart, end: info.contentStart);
  }

  TextRange? taskCheckboxVisualRangeForLine(int line) {
    final info = _taskCheckboxLineInfoForLine(line);
    if (info == null) return null;
    final start = info.isOrdered ? info.taskStart : info.markerStart;
    return TextRange(start: start, end: info.contentStart);
  }

  _TaskCheckboxLineInfo? _taskCheckboxLineInfoForLine(int line) {
    final text = value.text;
    if (text.isEmpty) return null;
    if (line < 0 || line >= _lineIndex.lineCount) return null;

    final lineStart = _lineIndex.offsetAtLine(line);
    final lineEndWithBreak = (line + 1 < _lineIndex.lineCount)
        ? _lineIndex.offsetAtLine(line + 1)
        : text.length;
    final lineEnd = (lineEndWithBreak > lineStart &&
            text.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? lineEndWithBreak - 1
        : lineEndWithBreak;
    if (lineEnd <= lineStart) return null;

    final marker = MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (marker == null) return null;
    final task = MarkdownLineHelpers.taskMarkerInfo(
      text,
      marker.markerEnd,
      lineEnd,
    );
    if (task == null) return null;
    return _TaskCheckboxLineInfo(
      markerStart: marker.markerStart,
      taskStart: marker.markerEnd,
      contentStart: task.contentStart,
      isOrdered: marker.isOrdered,
    );
  }

  bool handleArrowDownKey() => _inputIntents.handleArrowDownKey();

  bool handleArrowUpKey() => _inputIntents.handleArrowUpKey();

  bool tryExitFencedCodeOnArrowDown() =>
      _inputIntents.tryExitFencedCodeOnArrowDown();

  bool tryExitBlockquoteOnArrowDown() =>
      _inputIntents.tryExitBlockquoteOnArrowDown();

  bool tryExitFencedCodeOnArrowUp() =>
      _inputIntents.tryExitFencedCodeOnArrowUp();

  bool tryExitBlockquoteOnArrowUp() =>
      _inputIntents.tryExitBlockquoteOnArrowUp();

  bool tryExitFencedCodeOnEnter() => _inputIntents.tryExitFencedCodeOnEnter();

  /// Updates the language info string on the opening ``` fence for the fenced
  /// code block containing the current caret.
  ///
  /// Pass `null` to clear the info string (Plain text).
  bool setFencedCodeLanguageForSelection(String? fenceTag) =>
      _inputIntents.setFencedCodeLanguageForSelection(fenceTag);

  structure.FenceContext? _fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  }) =>
      _navigationHelpers.fenceContextForCaret(
        text: text,
        caret: caret,
        lineIndex: _lineIndex,
        geometry: _geometry,
        includeUnclosedEof: includeUnclosedEof,
      );

  structure.QuoteContext? _quoteContextForLine(String text, int line) =>
      _navigationHelpers.quoteContextForLine(
        text: text,
        line: line,
        lineIndex: _lineIndex,
        geometry: _geometry,
      );

  bool _isQuoteLineBodyBlank(String text, int line) => _navigationHelpers
      .isQuoteLineBodyBlank(text: text, line: line, lineIndex: _lineIndex);

  bool _isLineInsideFencedGeometry(int lineStartOffset) =>
      _navigationHelpers.isLineInsideFencedGeometry(
        lineStartOffset: lineStartOffset,
        geometry: _geometry,
      );

  bool _shouldExitBlockquoteOnArrowDown({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _navigationHelpers.shouldExitBlockquoteOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: _lineIndex,
        geometry: _geometry,
      );

  bool _shouldExitBlockquoteOnArrowUp({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _navigationHelpers.shouldExitBlockquoteOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: _lineIndex,
        geometry: _geometry,
      );

  bool _isUnclosedFenceAtEof(String text, MeasuredBlock block) =>
      _navigationHelpers.isUnclosedFenceAtEof(text: text, block: block);

  bool _shouldExitFenceOnArrowDown({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _navigationHelpers.shouldExitFenceOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: _lineIndex,
      );

  bool _shouldExitFenceOnArrowUp({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _navigationHelpers.shouldExitFenceOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: _lineIndex,
      );

  FenceEnterExitResult? _computeFenceExitOnEnter({
    required String text,
    required int caret,
    required structure.FenceContext context,
  }) =>
      _navigationHelpers.computeFenceExitOnEnter(
        text: text,
        caret: caret,
        context: context,
        lineIndex: _lineIndex,
      );

  String? _fenceLanguageForBlock(String text, int blockStartOffset) =>
      _navigationHelpers.fenceLanguageForBlock(
        text: text,
        blockStartOffset: blockStartOffset,
      );

  String _preferredOutdentUnitForLine({
    required String text,
    required MeasuredBlock block,
    required int line,
    required String currentIndent,
  }) =>
      _navigationHelpers.preferredOutdentUnitForLine(
        text: text,
        block: block,
        line: line,
        currentIndent: currentIndent,
        lineIndex: _lineIndex,
      );

  int _trailingBlankTrimStart(
    String text,
    int openLine,
    int closeLineExclusive,
  ) =>
      _navigationHelpers.trailingBlankTrimStart(
        text: text,
        openLine: openLine,
        closeLineExclusive: closeLineExclusive,
        lineIndex: _lineIndex,
      );

  bool _moveCaretVertically({required bool forward}) {
    final move = VerticalCaretNavigation.compute(
      selection: selection,
      text: value.text,
      lineIndex: _lineIndex,
      forward: forward,
      preferredColumn: _preferredVerticalCaretColumn,
    );
    if (move == null) return false;
    _preferredVerticalCaretColumn = move.preferredColumn;
    _isApplyingVerticalCaretMove = true;
    try {
      selection = TextSelection.collapsed(offset: move.targetOffset);
    } finally {
      _isApplyingVerticalCaretMove = false;
    }
    return true;
  }

  bool get canUndo => _undoStack.canUndo;
  bool get canRedo => _undoStack.canRedo;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    if (withComposing && value.composing.isValid) {
      return super.buildTextSpan(
        context: context,
        style: style,
        withComposing: withComposing,
      );
    }
    return _renderer.render(
      context: context,
      text: text,
      style: style,
      revision: _revision,
      latestDecoration: _latestDecoration,
      projectedExclusionRanges: _projectedExclusionRanges,
      authoritativeInlineRunsRevision: _authoritativeInlineRunsRevision,
      authoritativeInlineRuns: _authoritativeInlineRuns,
    );
  }
}

class _TaskCheckboxLineInfo {
  final int markerStart;
  final int taskStart;
  final int contentStart;
  final bool isOrdered;

  const _TaskCheckboxLineInfo({
    required this.markerStart,
    required this.taskStart,
    required this.contentStart,
    required this.isOrdered,
  });
}
