import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:sovereign_editor/src/helpers/logger.dart';
import 'package:sovereign_editor/src/widgets/sovereign/controllers/undo_stack.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/edit_differ.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/syntax_parse_scheduler.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/syntax_engine_factory.dart';

import '../models/sovereign_state.dart';
import '../models/edit_op.dart';
import '../models/decoration_model.dart';
import '../models/line_index.dart';
import '../models/geometry_model.dart';
import '../models/block_tree.dart';
import '../engine/syntax_engine.dart';
import '../engine/syntax_snapshot.dart';
import '../engine/syntax_types.dart';

import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_style_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_geometry_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/projector.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/state/editor_session_state.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/state/editor_session_state_builder.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/selection_projection_guard.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/selection_mask_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/projection_range_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/syntax_projection_coordinator.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/predictive_edit_range_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/value_mutation_coordinator.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/undo_grouping_policy.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/edit_operation_pipeline.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/intents/input_intent_handler.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/intents/input_intent_models.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/rendering/sovereign_text_renderer.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/fence/fence_editing_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/indented_code/indented_code_enter_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_line_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_structure_query_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_structure_transform_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/navigation_line_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/sovereign_navigation_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/vertical_caret_navigation.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_line_parser.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_tab_intent_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/syntax_snapshot_mapper.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/fence_context.dart'
    as structure;
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/list_marker_context.dart'
    as structure;
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/quote_context.dart'
    as structure;

part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_fence.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_fence_pairing.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_quote.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_link.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_list.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_table.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_policies_heading.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_value_mutation_coordinator.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_input_intent_handler.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_syntax_sync_coordinator.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_controller_diagnostics.dart';
part 'package:sovereign_editor/src/widgets/sovereign/controllers/sovereign_table_tab_intent_host.dart';

/// Text editing controller for Sovereign markdown documents.
///
/// The controller owns the canonical text/selection state, synchronous line and
/// geometry metadata, asynchronous syntax snapshots, undo/redo history, and the
/// markdown-aware keyboard/editing policies used by `SovereignEditor` and
/// `SovereignMarkdownView`.
class SovereignController extends TextEditingController {
  final SovereignTextRenderer _renderer = SovereignTextRenderer();
  final SelectionProjectionGuard _selectionProjectionGuard =
      const DefaultSelectionProjectionGuard();
  final SovereignNavigationHelpers _navigationHelpers =
      const SovereignNavigationHelpers();
  final MarkdownStructureQueryService _structureQueries =
      const MarkdownStructureQueryService();
  final MarkdownStructureTransformService _structureTransforms =
      const MarkdownStructureTransformService();
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

  /// Current immutable state snapshot for the controller.
  SovereignState get state => SovereignState(value: value, revision: _revision);

  /// Stream of markdown decoration metadata emitted after edits and parses.
  final _decorationController = StreamController<DecorationModel>.broadcast();

  /// Emits the latest block, line, and projection metadata for render layers.
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

  /// Latest decoration metadata available synchronously to widgets.
  DecorationModel _latestDecoration = DecorationModel.empty();

  /// Latest decoration metadata available synchronously to widgets.
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

  /// Current line index for the controller text.
  LineIndex get lineIndex => _lineIndex;

  /// Current geometry model for block backgrounds and quote rails.
  GeometryModel get geometry => _geometry;

  /// Most recent edit operation recorded by the controller, if any.
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

  /// Creates an editable markdown controller.
  ///
  /// [text] seeds the document. [syntaxEngine] can provide a custom parser; if
  /// omitted, the package default engine is used. [markdownProfile] controls
  /// whether CommonMark core or GFM syntax is requested from the engine.
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

  /// Creates a controller optimized for read-only markdown rendering.
  ///
  /// This bootstraps decoration state synchronously so `SovereignMarkdownView`
  /// can render immediately from an initial markdown string.
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
    return _structureQueries.parseTableLineAt(
      text: text,
      line: line,
      lineIndex: _lineIndex,
      geometry: _geometry,
      rowShapeResolver: _structureQueries.matchTableRowShape,
    );
  }

  int? _tableCellIndexForCaret(ParsedTableLine row, int caret) {
    return _structureQueries.tableCellIndexForCaret(row, caret);
  }

  bool _tableRegionHasSeparator(String text, int line, int columnCount) {
    return _structureQueries.tableRegionHasSeparator(
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
    return _structureQueries.findAdjacentTableLine(
      text: text,
      line: line,
      columnCount: columnCount,
      forward: forward,
      skipSeparator: skipSeparator,
      lineIndex: _lineIndex,
      parseLineAt: _parseTableLineAt,
    );
  }

  String _emptyTableRowTemplate(int columns, {required String indent}) =>
      _structureTransforms.emptyTableRowTemplate(columns, indent: indent);

  TableTabFormattingResult? _formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) {
    final formatted = _structureTransforms.formatEstablishedTableAroundCaret(
      text,
      caret,
    );
    if (formatted == null) return null;
    return TableTabFormattingResult(
      text: formatted.text,
      caret: formatted.caret,
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

  /// Runs multiple command mutations as one command transaction.
  ///
  /// Edits performed inside [action] share command undo grouping so a toolbar
  /// action can apply several text changes while remaining one logical command.
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

  /// Applies the markdown-aware Enter behavior at the current selection.
  ///
  /// When [suppressFenceExit] is true, fenced-code exit handling is temporarily
  /// disabled for this invocation.
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

  /// Applies Tab or Shift+Tab behavior at the current selection.
  ///
  /// Returns true when the controller handled the key instead of letting focus
  /// traversal or the platform default behavior proceed.
  bool handleTabKey({required bool reverse}) =>
      _inputIntents.handleTabKey(reverse: reverse);

  /// Toggles the task checkbox state at the current selection.
  bool toggleTaskCheckboxAtSelection() =>
      _inputIntents.toggleTaskCheckboxAtSelection();

  /// Toggles the task checkbox at [offset].
  ///
  /// When [insertIfList] is true, a checkbox marker may be inserted for a list
  /// item that does not yet have one.
  bool toggleTaskCheckboxAtOffset(int offset, {bool insertIfList = false}) =>
      _inputIntents.toggleTaskCheckboxAtOffset(
        offset,
        insertIfList: insertIfList,
      );

  /// Returns the storage range for the task marker on [line], if one exists.
  TextRange? taskCheckboxMarkerRangeForLine(int line) {
    final info = _taskCheckboxLineInfoForLine(line);
    if (info == null) return null;
    return TextRange(start: info.taskStart, end: info.contentStart);
  }

  /// Returns the visual range used to paint the task marker on [line].
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
    final lineEndWithBreak = _structureQueries.lineEndWithBreak(
      lineIndex: _lineIndex,
      text: text,
      line: line,
    );
    final lineEnd = _structureQueries.lineContentEnd(
      text: text,
      lineStart: lineStart,
      lineEndWithBreak: lineEndWithBreak,
    );
    if (lineEnd <= lineStart) return null;

    final marker = _structureQueries.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (marker == null) return null;
    final task = _structureQueries.taskMarkerInfo(
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

  /// Applies Arrow Down markdown navigation behavior.
  bool handleArrowDownKey() => _inputIntents.handleArrowDownKey();

  /// Applies Arrow Up markdown navigation behavior.
  bool handleArrowUpKey() => _inputIntents.handleArrowUpKey();

  /// Attempts to exit a fenced code block with Arrow Down.
  bool tryExitFencedCodeOnArrowDown() =>
      _inputIntents.tryExitFencedCodeOnArrowDown();

  /// Attempts to exit a blockquote with Arrow Down.
  bool tryExitBlockquoteOnArrowDown() =>
      _inputIntents.tryExitBlockquoteOnArrowDown();

  /// Attempts to exit a fenced code block with Arrow Up.
  bool tryExitFencedCodeOnArrowUp() =>
      _inputIntents.tryExitFencedCodeOnArrowUp();

  /// Attempts to exit a blockquote with Arrow Up.
  bool tryExitBlockquoteOnArrowUp() =>
      _inputIntents.tryExitBlockquoteOnArrowUp();

  /// Attempts to exit a fenced code block with Enter.
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
      _structureQueries.fenceContextForCaret(
        text: text,
        caret: caret,
        lineIndex: _lineIndex,
        geometry: _geometry,
        includeUnclosedEof: includeUnclosedEof,
      );

  structure.QuoteContext? _quoteContextForLine(String text, int line) =>
      _structureQueries.quoteContextForLine(
        text: text,
        line: line,
        lineIndex: _lineIndex,
        geometry: _geometry,
      );

  bool _isQuoteLineBodyBlank(String text, int line) => _structureQueries
      .isQuoteLineBodyBlank(text: text, line: line, lineIndex: _lineIndex);

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
      _structureQueries.isUnclosedFenceAtEof(text: text, block: block);

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
      _structureQueries.fenceLanguageForBlock(
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

  /// Whether an undo operation is currently available.
  bool get canUndo => _undoStack.canUndo;

  /// Whether a redo operation is currently available.
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
