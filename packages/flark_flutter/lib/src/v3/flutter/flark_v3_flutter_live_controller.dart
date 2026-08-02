import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flark/flark_adapter.dart';

import 'flark_v3_inline_editing_presentation.dart';
import 'flark_v3_list_item_editing.dart';

abstract interface class FlarkV3FrameScheduler {
  void schedule(VoidCallback callback);
}

final class FlarkV3FlutterFrameScheduler implements FlarkV3FrameScheduler {
  const FlarkV3FlutterFrameScheduler();

  @override
  void schedule(VoidCallback callback) {
    SchedulerBinding.instance.scheduleFrameCallback((_) => callback());
  }
}

enum FlarkV3FlutterPaintMode { sourceGap, stablePaint, exactStructural }

enum FlarkV3FlutterBlockStyleKind {
  fencedCode,
  indentedCode,
  heading,
  blockQuote,
  tightListItem;

  /// Source-compatible spelling retained for the original bullet-only slice.
  static const tightBulletListItem = tightListItem;
}

enum FlarkV3FlutterAtomicBlockKind { thematicBreak }

/// Parser-originated visual atom with exact canonical source ownership.
///
/// Atomic blocks do not manufacture display text for [EditableText]. The
/// stable input client remains collapsed immediately before or after the
/// source range while Flutter paints the visual atom independently.
final class FlarkV3FlutterAtomicBlockLease {
  const FlarkV3FlutterAtomicBlockLease.thematicBreak({required this.source})
    : kind = FlarkV3FlutterAtomicBlockKind.thematicBreak;

  final FlarkV3FlutterAtomicBlockKind kind;
  final FlarkV3SourceSpan source;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3FlutterAtomicBlockLease &&
      other.kind == kind &&
      _sameSourceSpan(other.source, source);

  @override
  int get hashCode => Object.hash(
    kind,
    source.startUtf8,
    source.endUtf8,
    source.startUtf16,
    source.endUtf16,
  );
}

/// Parser-originated block typography retained only for stable paint.
///
/// This lease never authorizes semantics, hit targets, or edits. It survives a
/// source advance within the same bounded input island so typing does not flash
/// back to body typography while exact structure catches up. An exact
/// contradictory query or an input-island handoff clears it.
final class FlarkV3FlutterBlockStyleLease {
  const FlarkV3FlutterBlockStyleLease.fencedCode()
    : kind = FlarkV3FlutterBlockStyleKind.fencedCode,
      headingLevel = null,
      tightListItemConfiguration = null;

  const FlarkV3FlutterBlockStyleLease.indentedCode()
    : kind = FlarkV3FlutterBlockStyleKind.indentedCode,
      headingLevel = null,
      tightListItemConfiguration = null;

  const FlarkV3FlutterBlockStyleLease.blockQuote()
    : kind = FlarkV3FlutterBlockStyleKind.blockQuote,
      headingLevel = null,
      tightListItemConfiguration = null;

  const FlarkV3FlutterBlockStyleLease.heading(int level)
    : assert(level >= 1 && level <= 6),
      headingLevel = level,
      tightListItemConfiguration = null,
      kind = FlarkV3FlutterBlockStyleKind.heading;

  const FlarkV3FlutterBlockStyleLease.tightListItem(
    FlarkV3TightListItemConfiguration configuration,
  ) : kind = FlarkV3FlutterBlockStyleKind.tightListItem,
      headingLevel = null,
      tightListItemConfiguration = configuration;

  const FlarkV3FlutterBlockStyleLease.tightBulletListItem(
    FlarkV3TightBulletListItemConfiguration configuration,
  ) : this.tightListItem(configuration);

  final FlarkV3FlutterBlockStyleKind kind;
  final int? headingLevel;
  final FlarkV3TightListItemConfiguration? tightListItemConfiguration;

  FlarkV3TightBulletListItemConfiguration?
  get tightBulletListItemConfiguration => tightListItemConfiguration;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3FlutterBlockStyleLease &&
      other.kind == kind &&
      other.headingLevel == headingLevel &&
      other.tightListItemConfiguration == tightListItemConfiguration;

  @override
  int get hashCode =>
      Object.hash(kind, headingLevel, tightListItemConfiguration);
}

/// Selects the single owner of bounded point queries for one controller.
///
/// Standalone controllers query their borrowed host directly while deriving a
/// frame. A managed binding already owns the Dart runtime query and must adopt
/// that typed result instead of paying for a second host query on the frame.
enum FlarkV3FlutterPointQueryOwnership {
  standaloneController,
  managedCoordinator,
}

/// One frame-coherent parser-to-paint decision.
final class FlarkV3FlutterPaintState {
  const FlarkV3FlutterPaintState._({
    required this.mode,
    required this.uiSource,
    required this.sourceVersion,
    required this.ack,
    required this.viewport,
    required this.uiSourceGap,
    required this.sourceGap,
    required this.documentQuery,
    required this.blockStyleLease,
    required this.atomicBlockLease,
  });

  const FlarkV3FlutterPaintState.sourceGap({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3UiSourceGap uiSourceGap,
    FlarkV3SourceGap? sourceGap,
    FlarkV3DocumentQueryResult? documentQuery,
    FlarkV3FlutterBlockStyleLease? blockStyleLease,
    FlarkV3FlutterAtomicBlockLease? atomicBlockLease,
  }) : this._(
         mode: FlarkV3FlutterPaintMode.sourceGap,
         uiSource: uiSource,
         sourceVersion: sourceVersion,
         ack: null,
         viewport: null,
         uiSourceGap: uiSourceGap,
         sourceGap: sourceGap,
         documentQuery: documentQuery,
         blockStyleLease: blockStyleLease,
         atomicBlockLease: atomicBlockLease,
       );

  const FlarkV3FlutterPaintState.stablePaint({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3StructuralAck ack,
    required FlarkV3UiSourceGap uiSourceGap,
    FlarkV3SourceGap? sourceGap,
    FlarkV3DocumentQueryResult? documentQuery,
    FlarkV3FlutterBlockStyleLease? blockStyleLease,
    FlarkV3FlutterAtomicBlockLease? atomicBlockLease,
  }) : this._(
         mode: FlarkV3FlutterPaintMode.stablePaint,
         uiSource: uiSource,
         sourceVersion: sourceVersion,
         ack: ack,
         viewport: null,
         uiSourceGap: uiSourceGap,
         sourceGap: sourceGap,
         documentQuery: documentQuery,
         blockStyleLease: blockStyleLease,
         atomicBlockLease: atomicBlockLease,
       );

  const FlarkV3FlutterPaintState.exactStructural({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3StructuralAck ack,
    required FlarkV3HostStructuralViewport viewport,
    FlarkV3FlutterBlockStyleLease? blockStyleLease,
    FlarkV3FlutterAtomicBlockLease? atomicBlockLease,
  }) : this._(
         mode: FlarkV3FlutterPaintMode.exactStructural,
         uiSource: uiSource,
         sourceVersion: sourceVersion,
         ack: ack,
         viewport: viewport,
         uiSourceGap: null,
         sourceGap: null,
         documentQuery: null,
         blockStyleLease: blockStyleLease,
         atomicBlockLease: atomicBlockLease,
       );

  const FlarkV3FlutterPaintState.exactManagedStructural({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3StructuralAck ack,
    required FlarkV3DocumentStructuralQuery documentQuery,
    FlarkV3FlutterBlockStyleLease? blockStyleLease,
    FlarkV3FlutterAtomicBlockLease? atomicBlockLease,
  }) : this._(
         mode: FlarkV3FlutterPaintMode.exactStructural,
         uiSource: uiSource,
         sourceVersion: sourceVersion,
         ack: ack,
         viewport: null,
         uiSourceGap: null,
         sourceGap: null,
         documentQuery: documentQuery,
         blockStyleLease: blockStyleLease,
         atomicBlockLease: atomicBlockLease,
       );

  const FlarkV3FlutterPaintState.exactManagedRecursive({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion sourceVersion,
    required FlarkV3StructuralAck ack,
    required FlarkV3RecursiveGreenPointQuery documentQuery,
    FlarkV3FlutterBlockStyleLease? blockStyleLease,
    FlarkV3FlutterAtomicBlockLease? atomicBlockLease,
  }) : this._(
         mode: FlarkV3FlutterPaintMode.exactStructural,
         uiSource: uiSource,
         sourceVersion: sourceVersion,
         ack: ack,
         viewport: null,
         uiSourceGap: null,
         sourceGap: null,
         documentQuery: documentQuery,
         blockStyleLease: blockStyleLease,
         atomicBlockLease: atomicBlockLease,
       );

  final FlarkV3FlutterPaintMode mode;

  /// Exact UI-owned source visible in the editor for this paint decision.
  final FlarkV3UiSourceIdentity uiSource;

  /// Last certified metric/hash authority used by the structural host.
  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3StructuralAck? ack;
  final FlarkV3HostStructuralViewport? viewport;
  final FlarkV3UiSourceGap? uiSourceGap;
  final FlarkV3SourceGap? sourceGap;
  final FlarkV3DocumentQueryResult? documentQuery;
  final FlarkV3FlutterBlockStyleLease? blockStyleLease;
  final FlarkV3FlutterAtomicBlockLease? atomicBlockLease;

  bool get semanticActionsValid =>
      mode == FlarkV3FlutterPaintMode.exactStructural &&
      uiSource.bindsCertified(sourceVersion) &&
      (viewport != null ||
          documentQuery is FlarkV3DocumentStructuralQuery ||
          documentQuery is FlarkV3RecursiveGreenPointQuery);
  bool get accessibilitySemanticsValid => semanticActionsValid;
  bool get markdownHitTargetsValid => semanticActionsValid;
}

final class FlarkV3ExactFlutterEdit {
  const FlarkV3ExactFlutterEdit({
    required this.localStartUtf16,
    required this.localEndUtf16,
    required this.replacement,
    required this.nextSelection,
    required this.nextComposing,
  });

  final int localStartUtf16;
  final int localEndUtf16;
  final String replacement;
  final TextSelection nextSelection;
  final TextRange nextComposing;
}

/// Document-coordinate editing state owned independently of one bounded
/// [TextEditingValue]. A selection may span outside the active input island;
/// an active composing range may not.
final class FlarkV3GlobalEditingState {
  const FlarkV3GlobalEditingState({
    required this.selection,
    required this.composing,
  });

  final TextSelection selection;
  final TextRange composing;
}

/// One edit that must bypass Flutter's ordinary bounded-string lane.
///
/// The replacement is adopted by the source session as provisional UTF-16
/// before scalar validation or UTF-8 encoding. Post-edit selection and
/// composition use document coordinates because the resulting caret can be
/// arbitrarily far outside the old input island.
final class FlarkV3BulkFlutterEdit {
  const FlarkV3BulkFlutterEdit({
    required this.localStartUtf16,
    required this.localEndUtf16,
    required this.replacement,
    required this.nextGlobalEditingState,
  });

  final int localStartUtf16;
  final int localEndUtf16;
  final String replacement;
  final FlarkV3GlobalEditingState nextGlobalEditingState;
}

final class FlarkV3InputIslandHandoffReceipt {
  const FlarkV3InputIslandHandoffReceipt({
    required this.previousStartUtf16,
    required this.currentStartUtf16,
    required this.currentEndUtf16,
    required this.selectionSpansOutsideIsland,
  });

  final int previousStartUtf16;
  final int currentStartUtf16;
  final int currentEndUtf16;
  final bool selectionSpansOutsideIsland;
}

typedef FlarkV3FlutterSourceEditReceipt = FlarkDocumentEditReceipt;
typedef FlarkV3FlutterCertificationAdoptionReceipt =
    FlarkDocumentCertificationReceipt;
typedef FlarkV3FlutterSourceWorkerRestartReceipt =
    FlarkDocumentSourceWorkerRestartReceipt;
typedef FlarkV3FlutterSourceTransactionApplier =
    FlarkDocumentEditReceipt Function(FlarkV3SourceTransaction transaction);
typedef FlarkV3FlutterEditingValueAdopter =
    void Function(TextEditingValue value);

/// Hard foreground safety envelope for the v3 Flutter proof.
///
/// Application code cannot construct or raise this profile. The provisional
/// values are deliberately conservative host-informed ceilings, not launch
/// SLAs; production may select a tighter internally calibrated platform
/// profile. Anything larger must use the bulk/island-handoff path rather than
/// silently entering a synchronous Flutter string, host, or decode kernel.
final class FlarkV3FlutterForegroundProfile {
  const FlarkV3FlutterForegroundProfile._({
    required this.maximumInputIslandUtf16,
    required this.maximumOrdinaryReplacementUtf16,
    required this.documentWorkProfile,
  });

  static const prototype = FlarkV3FlutterForegroundProfile._(
    maximumInputIslandUtf16: 8 * 1024,
    maximumOrdinaryReplacementUtf16: 8 * 1024,
    documentWorkProfile: FlarkDocumentWorkProfile.prototype,
  );

  final int maximumInputIslandUtf16;
  final int maximumOrdinaryReplacementUtf16;
  int get maximumPlatformDeltasPerCallback => 64;
  int get maximumPlatformDeltaBatchWorkUtf16 => maximumInputIslandUtf16 * 16;
  final FlarkDocumentWorkProfile documentWorkProfile;

  int get maximumQueryEncodedBytes =>
      documentWorkProfile.maximumQueryEncodedBytes;
  int get maximumQueryOpenDepth => documentWorkProfile.maximumQueryOpenDepth;
  int get maximumQueryLeafCount => documentWorkProfile.maximumQueryLeafCount;
  int get maximumQueryTreeNodesVisited =>
      documentWorkProfile.maximumQueryTreeNodesVisited;
  int get maximumHostInspectBytes =>
      documentWorkProfile.maximumHostInspectBytes;
  int get maximumHostCopyBytes => documentWorkProfile.maximumHostCopyBytes;
  int get maximumHostTransitions => documentWorkProfile.maximumHostTransitions;
  int get maximumPublicationPacketBytes =>
      documentWorkProfile.maximumPublicationPacketBytes;

  void validateInputIsland(FlarkV3InputIslandSnapshot island) {
    if (island.maximumUtf16 > maximumInputIslandUtf16) {
      throw ArgumentError.value(
        island.maximumUtf16,
        'inputIsland.maximumUtf16',
        'Input island exceeds the sealed Flutter foreground envelope.',
      );
    }
  }

  void validateQueryBudget(FlarkV3HostQueryBudget budget) =>
      documentWorkProfile.validateQueryBudget(budget);
}

/// One bounded exact-source input island hosted by Flutter's EditableText.
final class FlarkV3InputIslandSnapshot {
  FlarkV3InputIslandSnapshot({
    required this.globalStartUtf16,
    required this.value,
    required this.maximumUtf16,
  }) {
    if (globalStartUtf16 < 0 ||
        maximumUtf16 <= 0 ||
        value.text.length > maximumUtf16) {
      throw RangeError('Input island is outside its declared bound.');
    }
    _validateTextEditingValue(value);
  }

  final int globalStartUtf16;
  final TextEditingValue value;
  final int maximumUtf16;

  int get globalEndUtf16 => globalStartUtf16 + value.text.length;
}

/// Internal Flutter authority bridge for the v3 parser-to-paint proof.
///
/// The exact editing transaction is supplied explicitly. This gate does not
/// infer a source edit by diffing two full strings and does not implement a
/// second Markdown grammar. A production text-input adapter must produce this
/// exact transaction from Flutter's delta input path.
final class FlarkV3FlutterLiveController extends ChangeNotifier {
  FlarkV3FlutterLiveController._({
    required this.documentSession,
    required this.editingController,
    required this.queryBudget,
    required FlarkV3FrameScheduler frameScheduler,
    required int inputIslandGlobalStartUtf16,
    required int inputIslandSourceLengthUtf16,
    required int inputIslandBaseRevision,
    required FlarkV3GlobalEditingState globalEditingState,
    required this.pointQueryOwnership,
    required this.maximumInputIslandUtf16,
    required this.foregroundProfile,
    required FlarkV3FlutterSourceTransactionApplier sourceTransactionApplier,
  }) : _frameScheduler = frameScheduler,
       _inputIslandGlobalStartUtf16 = inputIslandGlobalStartUtf16,
       _inputIslandSourceLengthUtf16 = inputIslandSourceLengthUtf16,
       _inputIslandBaseRevision = inputIslandBaseRevision,
       _globalEditingState = globalEditingState,
       _sourceTransactionApplier = sourceTransactionApplier;

  factory FlarkV3FlutterLiveController.attach({
    required FlarkDocumentSession documentSession,
    required FlarkV3InputIslandSnapshot inputIsland,
    required FlarkV3HostQueryBudget queryBudget,
    FlarkV3FrameScheduler frameScheduler = const FlarkV3FlutterFrameScheduler(),
    FlarkV3FlutterSourceTransactionApplier? sourceTransactionApplier,
    FlarkV3FlutterPointQueryOwnership pointQueryOwnership =
        FlarkV3FlutterPointQueryOwnership.standaloneController,
  }) {
    const foregroundProfile = FlarkV3FlutterForegroundProfile.prototype;
    foregroundProfile.validateInputIsland(inputIsland);
    foregroundProfile.validateQueryBudget(queryBudget);
    documentSession.workProfile.validateQueryBudget(queryBudget);
    final source = documentSession.source;
    if (inputIsland.globalEndUtf16 > source.utf16Length ||
        source.readRange(
              inputIsland.globalStartUtf16,
              inputIsland.globalEndUtf16,
            ) !=
            inputIsland.value.text) {
      throw ArgumentError(
        'Input island must equal its bounded canonical-source range.',
      );
    }
    final controller = FlarkV3FlutterLiveController._(
      documentSession: documentSession,
      editingController: FlarkV3InlineTextEditingController.fromValue(
        inputIsland.value,
      ),
      queryBudget: queryBudget,
      frameScheduler: frameScheduler,
      inputIslandGlobalStartUtf16: inputIsland.globalStartUtf16,
      inputIslandSourceLengthUtf16: inputIsland.value.text.length,
      inputIslandBaseRevision: documentSession.uiRevision,
      globalEditingState: _globalEditingStateFromLocal(
        inputIsland.value,
        inputIsland.globalStartUtf16,
      ),
      pointQueryOwnership: pointQueryOwnership,
      maximumInputIslandUtf16: inputIsland.maximumUtf16,
      foregroundProfile: foregroundProfile,
      sourceTransactionApplier:
          sourceTransactionApplier ?? documentSession.apply,
    );
    controller._paintState = controller._derivePaintState();
    return controller;
  }

  /// Caller-owned engine session borrowed by this Flutter adapter.
  ///
  /// Disposing the controller releases only Flutter resources. The caller
  /// remains responsible for closing this session after every adapter using it
  /// has detached.
  final FlarkDocumentSession documentSession;
  final TextEditingController editingController;
  final FlarkV3HostQueryBudget queryBudget;
  final FlarkV3FrameScheduler _frameScheduler;
  final FlarkV3FlutterSourceTransactionApplier _sourceTransactionApplier;
  final FlarkV3FlutterPointQueryOwnership pointQueryOwnership;
  int _inputIslandGlobalStartUtf16;
  int _inputIslandSourceLengthUtf16;
  int _inputIslandBaseRevision;
  FlarkV3GlobalEditingState _globalEditingState;
  final int maximumInputIslandUtf16;
  final FlarkV3FlutterForegroundProfile foregroundProfile;

  late FlarkV3FlutterPaintState _paintState;
  bool _frameScheduled = false;
  bool _disposed = false;
  int _scheduledFrameCallbacks = 0;
  int _appliedPresentationFrames = 0;
  FlarkV3InlineIslandPresentation? _pendingInlinePresentation;
  bool _pendingLiteralSourcePaint = false;
  FlarkV3DocumentQueryResult? _managedDocumentQuery;
  FlarkV3StructuralAck? _managedDocumentQueryAck;
  int? _managedDocumentQueryPositionUtf16;
  TextAffinity? _managedDocumentQueryAffinity;
  FlarkV3FlutterBlockStyleLease? _paintBlockStyleLease;
  FlarkV3FlutterAtomicBlockLease? _paintAtomicBlockLease;
  FlarkV3StructuralAck? _recursiveGreenRowAck;
  FlarkV3RecursiveGreenRenderableRow? _recursiveGreenRow;

  FlarkV3SourceDocument get source => documentSession.source;
  int get inputIslandGlobalStartUtf16 => _inputIslandGlobalStartUtf16;
  int get inputIslandGlobalEndUtf16 =>
      _inputIslandGlobalStartUtf16 + _inputIslandSourceLengthUtf16;
  FlarkV3GlobalEditingState get globalEditingState => _globalEditingState;
  FlarkV3SourceVersion get sourceVersion => documentSession.sourceVersion;
  FlarkV3UiSourceIdentity get uiSource => documentSession.uiSource;
  FlarkV3FlutterPaintState get paintState => _paintState;
  int get scheduledFrameCallbacks => _scheduledFrameCallbacks;
  int get appliedPresentationFrames => _appliedPresentationFrames;
  bool get sourceWorkerSynchronized => documentSession.sourceWorkerSynchronized;
  bool get hasCertifiedInlinePresentation =>
      (editingController as FlarkV3InlineTextEditingController)
          .hasCertifiedPresentation;
  bool get hasProjectedInlinePresentation =>
      (editingController as FlarkV3InlineTextEditingController)
          .hasProjectedPresentation;
  FlarkV3RecursiveGreenRenderableRow? get recursiveGreenRow =>
      _recursiveGreenRow;

  /// Installs the exact row certificate shared with passive viewport paint.
  ///
  /// This grants no authority by itself. It is admitted only when the current
  /// schema-9 point query, structural ACK, input island, frame path, and row
  /// cuts all match exactly.
  void adoptRecursiveGreenRowAuthority({
    required FlarkV3StructuralAck structuralAck,
    required FlarkV3RecursiveGreenRenderableRow row,
  }) {
    if (_disposed) return;
    final editableSource = row.editableSource;
    final presentation = documentSession.presentationState;
    final query = _managedDocumentQuery;
    if (editableSource == null ||
        presentation is! FlarkV3ExactStructuralPresentation ||
        presentation.ack != structuralAck ||
        _managedDocumentQueryAck != structuralAck ||
        query is! FlarkV3RecursiveGreenPointQuery ||
        !_recursiveGreenQueryMatchesRow(query, row) ||
        !_recursiveGreenRowAuthorizesInputIsland(row, editableSource)) {
      throw StateError(
        'Recursive-Green row authority does not match the active editor.',
      );
    }
    if (_recursiveGreenRowAck == structuralAck &&
        _sameRecursiveGreenRow(_recursiveGreenRow, row)) {
      return;
    }
    _recursiveGreenRowAck = structuralAck;
    _recursiveGreenRow = row;
    _paintBlockStyleLease = switch (row.presentationKind) {
      FlarkV3RecursiveGreenRowPresentationKind.fencedCode =>
        const FlarkV3FlutterBlockStyleLease.fencedCode(),
      _ => null,
    };
    _schedulePresentationFrame();
  }

  /// Adopts the managed runtime's already-decoded point query for paint.
  ///
  /// This is the only exact point-query input accepted in managed mode. Frame
  /// derivation then validates it against current authority and never calls the
  /// host store independently.
  void adoptManagedDocumentQuery(FlarkV3DocumentQueryResult query) {
    if (_disposed) return;
    if (pointQueryOwnership !=
        FlarkV3FlutterPointQueryOwnership.managedCoordinator) {
      throw StateError(
        'Only a managed-query controller can adopt a document query.',
      );
    }
    final presentation = documentSession.presentationState;
    final ack = presentation is FlarkV3ExactStructuralPresentation
        ? presentation.ack
        : null;
    final selection = _globalEditingState.selection;
    if (identical(_managedDocumentQuery, query) &&
        _managedDocumentQueryAck == ack &&
        _managedDocumentQueryPositionUtf16 == selection.extentOffset &&
        _managedDocumentQueryAffinity == selection.affinity) {
      return;
    }
    _managedDocumentQuery = query;
    _managedDocumentQueryAck = ack;
    _managedDocumentQueryPositionUtf16 = selection.extentOffset;
    _managedDocumentQueryAffinity = selection.affinity;
    _reconcilePaintLeases(query);
    _schedulePresentationFrame();
  }

  /// Drops a managed point result that is no longer safe to paint.
  void clearManagedDocumentQuery() {
    if (_disposed ||
        pointQueryOwnership !=
            FlarkV3FlutterPointQueryOwnership.managedCoordinator ||
        _managedDocumentQuery == null) {
      return;
    }
    _managedDocumentQuery = null;
    _managedDocumentQueryAck = null;
    _managedDocumentQueryPositionUtf16 = null;
    _managedDocumentQueryAffinity = null;
    _schedulePresentationFrame();
  }

  void _reconcilePaintLeases(FlarkV3DocumentQueryResult? query) {
    if (query is FlarkV3RecursiveGreenPointQuery) {
      _paintAtomicBlockLease = null;
      if (_managedDocumentQueryAck != _recursiveGreenRowAck ||
          !_recursiveGreenQueryMatchesRow(query, _recursiveGreenRow)) {
        _recursiveGreenRowAck = null;
        _recursiveGreenRow = null;
      }
      _paintBlockStyleLease = switch (_recursiveGreenRow) {
        FlarkV3RecursiveGreenRenderableRow(
          presentationKind: FlarkV3RecursiveGreenRowPresentationKind.fencedCode,
        ) =>
          const FlarkV3FlutterBlockStyleLease.fencedCode(),
        _ => null,
      };
      return;
    }
    if (query is! FlarkV3DocumentStructuralQuery) return;
    final presentation = documentSession.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation ||
        _managedDocumentQueryAck != presentation.ack ||
        query.sourceRevision != sourceVersion.revision ||
        query.structureRevision != sourceVersion.revision) {
      return;
    }
    final structure = query.structure;
    _paintBlockStyleLease = switch (structure) {
      FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.fencedCode,
        fencedCode: final fence?,
      )
          when _sourceSpanContainsInputIsland(fence.bodySource) =>
        const FlarkV3FlutterBlockStyleLease.fencedCode(),
      FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.indentedCode,
        source: final blockSource,
        indentedCode: FlarkV3IndentedCodeFacts(),
      )
          when _sourceSpanContainsInputIsland(blockSource) =>
        const FlarkV3FlutterBlockStyleLease.indentedCode(),
      FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.blockQuote,
        source: final blockSource,
        blockQuote: FlarkV3BlockQuoteFacts(),
      )
          when _sourceSpanContainsInputIsland(blockSource) =>
        const FlarkV3FlutterBlockStyleLease.blockQuote(),
      FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.heading,
        heading: FlarkV3HeadingFacts(:final level, :final contentSource),
      )
          when _sourceSpanContainsInputIsland(contentSource) =>
        FlarkV3FlutterBlockStyleLease.heading(level),
      _ => null,
    };
    _paintAtomicBlockLease = switch (structure) {
      FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.thematicBreak,
        source: final source,
        thematicBreak: FlarkV3ThematicBreakFacts(),
      ) =>
        FlarkV3FlutterAtomicBlockLease.thematicBreak(source: source),
      _ => null,
    };
  }

  bool _sourceSpanContainsInputIsland(FlarkV3SourceSpan source) =>
      _inputIslandGlobalStartUtf16 >= source.startUtf16 &&
      inputIslandGlobalEndUtf16 <= source.endUtf16;

  bool _recursiveGreenRowAuthorizesInputIsland(
    FlarkV3RecursiveGreenRenderableRow row,
    FlarkV3SourceSpan editableSource,
  ) {
    if (row.kind.isInlineBearing || row.kind.isTerminalEmptyItem) {
      return _inputIslandGlobalStartUtf16 == editableSource.startUtf16 &&
          inputIslandGlobalEndUtf16 == editableSource.endUtf16;
    }
    return row.kind == FlarkV3RecursiveGreenKind.fencedCode &&
        row.presentationKind ==
            FlarkV3RecursiveGreenRowPresentationKind.fencedCode &&
        row.literal &&
        row.editCapability ==
            FlarkV3RecursiveGreenRowEditCapability.contiguous &&
        _sourceSpanContainsInputIsland(editableSource);
  }

  bool _recursiveGreenFencedRowAuthorizesCurrentInput(
    FlarkV3RecursiveGreenPointQuery query,
    FlarkV3StructuralAck structuralAck,
  ) {
    final row = _recursiveGreenRow;
    final editableSource = row?.editableSource;
    return row != null &&
        editableSource != null &&
        _recursiveGreenRowAck == structuralAck &&
        _recursiveGreenQueryMatchesRow(query, row) &&
        _recursiveGreenRowAuthorizesInputIsland(row, editableSource);
  }

  bool _recursiveGreenTerminalEmptyRowAuthorizesCurrentInput(
    FlarkV3RecursiveGreenPointQuery query,
    FlarkV3StructuralAck structuralAck,
  ) {
    final row = _recursiveGreenRow;
    final editableSource = row?.editableSource;
    final lease = (editingController as FlarkV3InlineTextEditingController)
        .projectedInputLease;
    return row != null &&
        row.kind.isTerminalEmptyItem &&
        editableSource != null &&
        _recursiveGreenRowAck == structuralAck &&
        _recursiveGreenQueryMatchesRow(query, row) &&
        _recursiveGreenRowAuthorizesInputIsland(row, editableSource) &&
        lease != null &&
        lease.isCertified &&
        lease.certifiedSourceVersion == sourceVersion &&
        lease.sourceStartUtf16 == editableSource.startUtf16 &&
        lease.sourceEndUtf16 == editableSource.endUtf16;
  }

  /// Live authority guard. This becomes false synchronously on source advance
  /// and stays false until an exact-current viewport is adopted on a frame.
  bool get semanticActionsValid =>
      documentSession.presentationState is FlarkV3ExactStructuralPresentation &&
      _paintState.semanticActionsValid &&
      _paintState.uiSource == uiSource &&
      _paintState.sourceVersion == sourceVersion;

  /// Whether the stable input client is ready to replace one exact passive row.
  ///
  /// Activation first moves the bounded input island, which deliberately clears
  /// its old projection and block chrome. The virtualized surface uses this
  /// predicate to keep the target's parser-authored passive paint visible until
  /// the same current source, structural block, projected display, and chrome
  /// have all been adopted by the active editor.
  bool isExactCurrentPresentationFor({
    required FlarkV3SourceVersion targetSourceVersion,
    required FlarkV3SourceSpan targetPhysicalSource,
    required FlarkV3DocumentStructureKind targetKind,
    required String targetDisplayText,
    FlarkV3StructuralAck? targetRecursiveGreenAck,
    FlarkV3RecursiveGreenRenderableRow? targetRecursiveGreenRow,
  }) {
    if (targetRecursiveGreenRow != null || targetRecursiveGreenAck != null) {
      final targetRow = targetRecursiveGreenRow;
      final targetAck = targetRecursiveGreenAck;
      if (targetRow == null || targetAck == null) return false;
      final presentation = documentSession.presentationState;
      final query = _paintState.documentQuery;
      final editableSource = targetRow.editableSource;
      final lease = (editingController as FlarkV3InlineTextEditingController)
          .projectedInputLease;
      if (editableSource == null) return false;
      final exactRowAuthority =
          presentation is FlarkV3ExactStructuralPresentation &&
          presentation.ack == targetAck &&
          _paintState.ack == targetAck &&
          _managedDocumentQueryAck == targetAck &&
          _recursiveGreenRowAck == targetAck &&
          sourceVersion == targetSourceVersion &&
          editingController.text == targetDisplayText &&
          _sameSourceSpan(
            targetRow.presentationPhysicalSource,
            targetPhysicalSource,
          ) &&
          _sameRecursiveGreenRow(_recursiveGreenRow, targetRow) &&
          query is FlarkV3RecursiveGreenPointQuery &&
          _recursiveGreenQueryMatchesRow(query, targetRow);
      if (!exactRowAuthority) return false;
      if (!targetRow.kind.isInlineBearing &&
          !targetRow.kind.isTerminalEmptyItem) {
        return targetRow.kind == FlarkV3RecursiveGreenKind.fencedCode &&
            targetRow.presentationKind ==
                FlarkV3RecursiveGreenRowPresentationKind.fencedCode &&
            targetRow.literal &&
            targetRow.editCapability ==
                FlarkV3RecursiveGreenRowEditCapability.contiguous &&
            lease == null &&
            _inputIslandGlobalStartUtf16 == editableSource.startUtf16 &&
            inputIslandGlobalEndUtf16 == editableSource.endUtf16;
      }
      return lease != null &&
          lease.isCertified &&
          lease.certifiedSourceVersion == targetSourceVersion &&
          lease.sourceStartUtf16 == editableSource.startUtf16 &&
          lease.sourceEndUtf16 == editableSource.endUtf16;
    }
    if (!semanticActionsValid ||
        sourceVersion != targetSourceVersion ||
        editingController.text != targetDisplayText) {
      return false;
    }
    final query = _paintState.documentQuery;
    if (query is! FlarkV3DocumentStructuralQuery ||
        query.structure.kind != targetKind ||
        !_sameSourceSpan(query.structure.source, targetPhysicalSource) ||
        !_blockStyleMatchesTarget(targetKind, _paintState.blockStyleLease)) {
      return false;
    }
    if (targetKind == FlarkV3DocumentStructureKind.thematicBreak) {
      final atom = _paintState.atomicBlockLease;
      return atom?.kind == FlarkV3FlutterAtomicBlockKind.thematicBreak &&
          _sameSourceSpan(atom!.source, targetPhysicalSource);
    }
    final lease = (editingController as FlarkV3InlineTextEditingController)
        .projectedInputLease;
    return lease != null &&
        lease.isCertified &&
        lease.certifiedSourceVersion == targetSourceVersion &&
        lease.sourceStartUtf16 >= targetPhysicalSource.startUtf16 &&
        lease.sourceEndUtf16 <= targetPhysicalSource.endUtf16;
  }

  /// Injects an already-validated Dart inline projection into stable Flutter
  /// editing state. Exact source and island authority are checked again at the
  /// adapter boundary; stale or partial projections fail closed.
  void adoptInlineIslandPresentation(FlarkV3InlineIslandPresentation decision) {
    if (_disposed) {
      throw StateError('The Flutter live controller is disposed.');
    }
    if (decision is FlarkV3SourcePaintInlineIslandPresentation) {
      _adoptSourcePaintInlinePresentation(decision);
      return;
    }
    final presentation = FlarkV3FlutterInlinePresentation.fromAuthoritative(
      decision as FlarkV3AuthoritativeInlineIslandPresentation,
    );
    if (presentation.sourceVersion != sourceVersion ||
        presentation.islandStartUtf16 != _inputIslandGlobalStartUtf16 ||
        presentation.islandEndUtf16 != inputIslandGlobalEndUtf16) {
      throw StateError(
        'Inline presentation does not match the exact active editing island.',
      );
    }
    if (_hasActiveComposition(editingController.value.composing)) {
      _pendingInlinePresentation = decision;
      return;
    }
    _adoptProjectedInputLease(presentation.inputLease);
    _inputIslandBaseRevision = presentation.sourceVersion.revision;
    _schedulePresentationFrame();
  }

  /// Installs one parser-certified generic source projection as the active
  /// editable input value.
  ///
  /// This is the non-inline counterpart to [adoptInlineIslandPresentation].
  /// The caller supplies both the exact source/display geometry and the edit
  /// policy selected for the parser-certified leaf kind. Flutter does not
  /// inspect Markdown source to construct either one.
  void adoptProjectedInputLease(FlarkV3ProjectedInputLease lease) {
    if (_disposed) {
      throw StateError('The Flutter live controller is disposed.');
    }
    if (!lease.isCertified ||
        lease.certifiedSourceVersion != sourceVersion ||
        !documentSession.currentUiSourceCertified ||
        lease.sourceStartUtf16 != _inputIslandGlobalStartUtf16 ||
        lease.sourceEndUtf16 != inputIslandGlobalEndUtf16) {
      throw StateError(
        'Projected input lease does not match the exact active editing island.',
      );
    }
    if (_hasActiveComposition(editingController.value.composing)) {
      // The coordinator observes composition changes and will retry the exact
      // query after commit. Never rewrite a platform-owned composing value.
      markInlinePresentationProvisional();
      return;
    }
    _adoptProjectedInputLease(lease);
    _inputIslandBaseRevision = sourceVersion.revision;
    _schedulePresentationFrame();
  }

  /// Installs one parser-selected tight list item and its paint-only gutter.
  ///
  /// [configuration] is the same parser-decoded prefix authority supplied to
  /// the lease's edit policy. This method does not recognize list syntax; it
  /// only keeps the already-validated edit and presentation facts coherent.
  void adoptTightListItemInputLease(
    FlarkV3ProjectedInputLease lease, {
    required FlarkV3TightListItemConfiguration configuration,
  }) {
    if (_disposed) {
      throw StateError('The Flutter live controller is disposed.');
    }
    if (_hasActiveComposition(editingController.value.composing)) {
      // Lease adoption and its typed paint authority are one operation. The
      // managed coordinator retries both after the platform commits composing
      // text, so never expose a gutter for a projection that was not adopted.
      markProjectedInputLeaseProvisional();
      return;
    }
    adoptProjectedInputLease(lease);
    _paintBlockStyleLease = FlarkV3FlutterBlockStyleLease.tightListItem(
      configuration,
    );
    _schedulePresentationFrame();
  }

  void adoptTightBulletListItemInputLease(
    FlarkV3ProjectedInputLease lease, {
    required FlarkV3TightBulletListItemConfiguration configuration,
  }) => adoptTightListItemInputLease(lease, configuration: configuration);

  /// Drops semantic authority without rewriting the platform input value.
  ///
  /// Ordinary edits keep the mechanically updated hidden projection while the
  /// parser catches up. This is stable paint, not parser authority.
  void markProjectedInputLeaseProvisional() {
    if (_disposed) return;
    (editingController as FlarkV3InlineTextEditingController)
        .markProjectedInputLeaseProvisional();
    _schedulePresentationFrame();
  }

  /// Compatibility name retained for the original inline-only projection API.
  void markInlinePresentationProvisional() =>
      markProjectedInputLeaseProvisional();

  /// Compatibility name for older callers. A stale authority edge retains its
  /// mechanically exact projection while parser certification catches up.
  void clearCertifiedInlinePresentation() =>
      markInlinePresentationProvisional();

  FlarkV3ProjectedInputLease? get _projectedInputLease =>
      (editingController as FlarkV3InlineTextEditingController)
          .projectedInputLease;

  void _adoptProjectedInputLease(FlarkV3ProjectedInputLease lease) {
    if (lease.sourceStartUtf16 != _inputIslandGlobalStartUtf16 ||
        lease.sourceEndUtf16 != inputIslandGlobalEndUtf16) {
      throw StateError(
        'Projected input lease does not cover the source island.',
      );
    }
    final sourceSelection = _globalEditingState.selection;
    final selectionSpansOutsideIsland = _selectionSpansOutsideIsland(
      sourceSelection,
      lease.sourceStartUtf16,
      lease.sourceEndUtf16,
    );
    final extent = sourceSelection.extentOffset;
    if (extent < lease.sourceStartUtf16 || extent > lease.sourceEndUtf16) {
      throw StateError('Projected input lease does not contain the caret.');
    }
    final nextValue = TextEditingValue(
      text: lease.displayText,
      selection: selectionSpansOutsideIsland
          ? TextSelection.collapsed(
              offset: lease.sourceToDisplayOffset(extent),
              affinity: sourceSelection.affinity,
            )
          : lease.sourceSelectionToDisplay(sourceSelection),
      composing: TextRange.empty,
    );
    final controller = editingController as FlarkV3InlineTextEditingController;
    controller.value = nextValue;
    controller.adoptProjectedInputLease(lease);
    _inputIslandSourceLengthUtf16 = lease.sourceLengthUtf16;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
  }

  void _adoptSourcePaintInlinePresentation(
    FlarkV3SourcePaintInlineIslandPresentation decision,
  ) {
    if (decision.source.startUtf16 != _inputIslandGlobalStartUtf16 ||
        decision.source.endUtf16 != inputIslandGlobalEndUtf16) {
      throw StateError(
        'Source-paint decision does not cover the input island.',
      );
    }
    if (_hasActiveComposition(editingController.value.composing)) {
      _pendingInlinePresentation = decision;
      _pendingLiteralSourcePaint = false;
      return;
    }
    _adoptLiteralSourcePaintNow();
  }

  /// Installs exact literal source after a current fail-closed parser result.
  ///
  /// Active composition freezes the platform-delivered value, so source paint
  /// is queued until commit rather than rewriting composing text or offsets.
  void adoptLiteralSourcePaint() {
    if (_disposed) return;
    if (_hasActiveComposition(editingController.value.composing)) {
      _pendingInlinePresentation = null;
      _pendingLiteralSourcePaint = true;
      return;
    }
    _adoptLiteralSourcePaintNow();
  }

  void _adoptLiteralSourcePaintNow() {
    final text = source.readRange(
      _inputIslandGlobalStartUtf16,
      inputIslandGlobalEndUtf16,
    );
    final value = _localEditingValue(
      text: text,
      islandStartUtf16: _inputIslandGlobalStartUtf16,
      global: _globalEditingState,
    );
    final controller = editingController as FlarkV3InlineTextEditingController;
    controller.clearProjectedInputLease();
    controller.value = value;
    _inputIslandBaseRevision = documentSession.uiRevision;
    _inputIslandSourceLengthUtf16 = text.length;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
    _schedulePresentationFrame();
  }

  void _tryAdoptPendingInlinePresentation() {
    if (_hasActiveComposition(editingController.value.composing)) return;
    if (_pendingLiteralSourcePaint) {
      _pendingLiteralSourcePaint = false;
      adoptLiteralSourcePaint();
      return;
    }
    final pending = _pendingInlinePresentation;
    if (pending == null) return;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
    try {
      adoptInlineIslandPresentation(pending);
    } on StateError {
      markInlinePresentationProvisional();
    }
  }

  int _projectedSourceRangeLength(int displayStart, int displayEnd) {
    final lease = _projectedInputLease;
    if (lease == null) return displayEnd - displayStart;
    if (displayStart == displayEnd) return 0;
    final preferred = _globalEditingState.selection;
    if (preferred.isValid &&
        preferred.start >= lease.sourceStartUtf16 &&
        preferred.end <= lease.sourceEndUtf16 &&
        lease.sourceToDisplayOffset(preferred.start) == displayStart &&
        lease.sourceToDisplayOffset(preferred.end) == displayEnd) {
      return preferred.end - preferred.start;
    }
    final sourceStart = lease.displayToSourceOffset(
      displayStart,
      affinity: FlarkV3InlineProjectionAffinity.downstream,
    );
    final sourceEnd = lease.displayToSourceOffset(
      displayEnd,
      affinity: FlarkV3InlineProjectionAffinity.upstream,
    );
    return sourceEnd - sourceStart;
  }

  FlarkV3FlutterSourceEditReceipt _applyProjectedDisplayEdit({
    required int displayStartUtf16,
    required int displayEndUtf16,
    required String replacement,
    required TextEditingValue nextDisplayValue,
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    final lease = _projectedInputLease;
    if (lease == null) {
      throw StateError('No projected input lease is active.');
    }
    final projected = lease.applyDisplayEdit(
      displayStartUtf16: displayStartUtf16,
      displayEndUtf16: displayEndUtf16,
      replacement: replacement,
      nextDisplayValue: nextDisplayValue,
      preferredSourceSelection: _globalEditingState.selection,
      preferredSourceComposing: _globalEditingState.composing,
    );
    if (projected.sourceReplacement.length >
        foregroundProfile.maximumOrdinaryReplacementUtf16) {
      throw StateError(
        'Projected canonical replacement exceeds the ordinary Flutter '
        'foreground envelope.',
      );
    }
    final nextSourceLength =
        _inputIslandSourceLengthUtf16 -
        (projected.sourceEndUtf16 - projected.sourceStartUtf16) +
        projected.sourceReplacement.length;
    if (nextSourceLength > maximumInputIslandUtf16) {
      throw RangeError('Projected edit exceeds the input-island bound.');
    }
    final receipt = _sourceTransactionApplier(
      FlarkV3SourceTransaction.single(
        baseRevision: _inputIslandBaseRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: projected.sourceStartUtf16,
          endUtf16: projected.sourceEndUtf16,
          replacement: projected.sourceReplacement,
        ),
      ),
    );
    _inputIslandBaseRevision = receipt.uiSource.uiRevision;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
    _inputIslandSourceLengthUtf16 = nextSourceLength;
    _globalEditingState = FlarkV3GlobalEditingState(
      selection: projected.sourceSelection,
      composing: projected.sourceComposing,
    );
    final controller = editingController as FlarkV3InlineTextEditingController;
    _adoptEditingValue(projected.displayValue, editingValueAdopter);
    controller.adoptProjectedInputLease(projected.nextLease);
    _schedulePresentationFrame();
    return receipt;
  }

  /// Simulates a projected platform batch against bounded display and source
  /// replicas, then commits its exact final source splice once.
  ///
  /// Sequential platform deltas use the coordinate space produced by the
  /// previous delta. Reducing only their final display strings can cross
  /// hidden Markdown pieces and accidentally replace delimiters. Advancing the
  /// mechanical projection after each delta preserves those pieces without
  /// parsing, while the single final source transaction keeps the callback
  /// atomic and advances at most one revision.
  FlarkV3FlutterSourceEditReceipt? _applyProjectedTextEditingDeltaBatch(
    List<TextEditingDelta> deltas, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    final initialLease = _projectedInputLease;
    if (initialLease == null) {
      throw StateError('No projected input lease is active.');
    }
    final islandStart = _inputIslandGlobalStartUtf16;
    final beforeSource = source.readRange(
      islandStart,
      inputIslandGlobalEndUtf16,
    );
    if (initialLease.sourceStartUtf16 != islandStart ||
        initialLease.sourceEndUtf16 != inputIslandGlobalEndUtf16 ||
        beforeSource.length != initialLease.sourceLengthUtf16) {
      throw StateError(
        'Projected input lease does not match its canonical source island.',
      );
    }

    var nextSource = beforeSource;
    var nextDisplay = editingController.value;
    var nextLease = initialLease;
    var sourceSelection = _globalEditingState.selection;
    var sourceComposing = _globalEditingState.composing;

    for (final delta in deltas) {
      final updatedDisplay = _applyBoundedDeltaForBatch(
        current: nextDisplay,
        delta: delta,
        maximumUtf16: maximumInputIslandUtf16,
      );
      if (delta is TextEditingDeltaNonTextUpdate) {
        if (updatedDisplay != nextDisplay) {
          sourceSelection = nextLease.displaySelectionToSource(
            updatedDisplay.selection,
            preferredSourceSelection: sourceSelection,
          );
          sourceComposing = nextLease.displayComposingToSource(
            updatedDisplay.composing,
            preferredSourceComposing: sourceComposing,
          );
        }
        nextDisplay = updatedDisplay;
        continue;
      }

      final edit = _textEditFromDelta(delta);
      if (edit.replacement.length >
          foregroundProfile.maximumOrdinaryReplacementUtf16) {
        throw StateError(
          'A bulk text-input delta must be the callback\'s only delta.',
        );
      }
      final projected = nextLease.applyDisplayEdit(
        displayStartUtf16: edit.start,
        displayEndUtf16: edit.end,
        replacement: edit.replacement,
        nextDisplayValue: updatedDisplay,
        preferredSourceSelection: sourceSelection,
        preferredSourceComposing: sourceComposing,
        cleanupOrphanedDelimiters: false,
      );
      if (projected.sourceReplacement.length >
          foregroundProfile.maximumOrdinaryReplacementUtf16) {
        throw StateError(
          'Projected canonical replacement exceeds the ordinary Flutter '
          'foreground envelope.',
        );
      }
      if (projected.nextLease.sourceStartUtf16 != islandStart) {
        throw StateError(
          'Projected input lease changed the input-island origin.',
        );
      }
      final localSourceStart = projected.sourceStartUtf16 - islandStart;
      final localSourceEnd = projected.sourceEndUtf16 - islandStart;
      _validateUtf16Edit(
        oldText: nextSource,
        start: localSourceStart,
        end: localSourceEnd,
        replacement: projected.sourceReplacement,
        coordinateSpace: 'projected text-input delta batch',
      );
      final nextSourceLength =
          nextSource.length -
          (localSourceEnd - localSourceStart) +
          projected.sourceReplacement.length;
      if (nextSourceLength > maximumInputIslandUtf16) {
        throw StateError(
          'A projected text-input batch exceeds the input-island bound.',
        );
      }
      nextSource = nextSource.replaceRange(
        localSourceStart,
        localSourceEnd,
        projected.sourceReplacement,
      );
      nextDisplay = updatedDisplay;
      nextLease = projected.nextLease;
      sourceSelection = projected.sourceSelection;
      sourceComposing = projected.sourceComposing;
    }

    final cleanup = nextLease.cleanupOrphanedDelimiters(
      sourceSelection: sourceSelection,
      sourceComposing: sourceComposing,
    );
    for (final edit in cleanup.sourceEdits) {
      final localStart = edit.startUtf16 - islandStart;
      final localEnd = edit.endUtf16 - islandStart;
      _validateUtf16Edit(
        oldText: nextSource,
        start: localStart,
        end: localEnd,
        replacement: '',
        coordinateSpace: 'projected delimiter cleanup',
      );
      nextSource = nextSource.replaceRange(localStart, localEnd, '');
    }
    nextLease = cleanup.nextLease;
    sourceSelection = cleanup.sourceSelection;
    sourceComposing = cleanup.sourceComposing;

    final nextGlobalEditingState = FlarkV3GlobalEditingState(
      selection: sourceSelection,
      composing: sourceComposing,
    );
    final nextDocumentLength =
        source.utf16Length - beforeSource.length + nextSource.length;
    _validateGlobalEditingState(nextGlobalEditingState, nextDocumentLength);

    if (nextSource == beforeSource) {
      if (nextDisplay.text != initialLease.displayText) {
        throw StateError(
          'A source-neutral projected batch changed its display text.',
        );
      }
      _globalEditingState = nextGlobalEditingState;
      _adoptEditingValue(nextDisplay, editingValueAdopter);
      (editingController as FlarkV3InlineTextEditingController)
          .adoptProjectedInputLease(initialLease);
      _tryAdoptPendingInlinePresentation();
      _schedulePresentationFrame();
      return null;
    }

    final splice = _boundedReplacement(beforeSource, nextSource);
    final receipt = _sourceTransactionApplier(
      FlarkV3SourceTransaction.single(
        baseRevision: _inputIslandBaseRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: islandStart + splice.oldStart,
          endUtf16: islandStart + splice.oldEnd,
          replacement: nextSource.substring(splice.newStart, splice.newEnd),
        ),
      ),
    );
    _inputIslandBaseRevision = receipt.uiSource.uiRevision;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
    _inputIslandSourceLengthUtf16 = nextSource.length;
    _globalEditingState = nextGlobalEditingState;
    _adoptEditingValue(nextDisplay, editingValueAdopter);
    (editingController as FlarkV3InlineTextEditingController)
        .adoptProjectedInputLease(nextLease);
    _schedulePresentationFrame();
    return receipt;
  }

  FlarkV3FlutterSourceEditReceipt applyExactEdit(
    FlarkV3ExactFlutterEdit edit, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    final before = editingController.value;
    if (edit.localStartUtf16 < 0 ||
        edit.localEndUtf16 < edit.localStartUtf16 ||
        edit.localEndUtf16 > before.text.length) {
      throw RangeError('Exact edit escapes the bounded input island.');
    }
    _validateUtf16Edit(
      oldText: before.text,
      start: edit.localStartUtf16,
      end: edit.localEndUtf16,
      replacement: edit.replacement,
      coordinateSpace: 'exact edit',
    );
    if (edit.replacement.length >
        foregroundProfile.maximumOrdinaryReplacementUtf16) {
      throw StateError(
        'Replacement exceeds the ordinary Flutter foreground envelope; '
        'route it through the provisional bulk/island-handoff adapter.',
      );
    }
    final expectedText = before.text.replaceRange(
      edit.localStartUtf16,
      edit.localEndUtf16,
      edit.replacement,
    );
    if (expectedText.length > maximumInputIslandUtf16) {
      throw RangeError('Exact edit exceeds the input-island bound.');
    }
    final nextValue = TextEditingValue(
      text: expectedText,
      selection: edit.nextSelection,
      composing: edit.nextComposing,
    );
    _validateTextEditingValue(nextValue);
    if (_projectedInputLease != null) {
      return _applyProjectedDisplayEdit(
        displayStartUtf16: edit.localStartUtf16,
        displayEndUtf16: edit.localEndUtf16,
        replacement: edit.replacement,
        nextDisplayValue: nextValue,
        editingValueAdopter: editingValueAdopter,
      );
    }
    final nextGlobalEditingState = _globalEditingStateFromLocal(
      nextValue,
      _inputIslandGlobalStartUtf16,
    );

    final receipt = _sourceTransactionApplier(
      FlarkV3SourceTransaction.single(
        baseRevision: _inputIslandBaseRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: _inputIslandGlobalStartUtf16 + edit.localStartUtf16,
          endUtf16: _inputIslandGlobalStartUtf16 + edit.localEndUtf16,
          replacement: edit.replacement,
        ),
      ),
    );
    return _adoptAppliedSourceEdit(
      receipt: receipt,
      nextValue: nextValue,
      nextIslandStartUtf16: _inputIslandGlobalStartUtf16,
      nextGlobalEditingState: nextGlobalEditingState,
      editingValueAdopter: editingValueAdopter,
    );
  }

  /// Whether hardware Backspace currently targets a parser-hidden list prefix.
  ///
  /// Flutter receives no deletion delta at display-column zero because the
  /// marker contributes no display text. The live editor uses this predicate
  /// to install a narrowly conditional shortcut; ordinary Backspace remains
  /// owned by [EditableText] everywhere else.
  bool get canRemoveTightListItemPrefixAtCaret {
    final configuration = _paintBlockStyleLease?.tightListItemConfiguration;
    final lease = _projectedInputLease;
    final value = editingController.value;
    final selection = value.selection;
    if (configuration == null ||
        !configuration.backspaceAtStartRemovesPrefix ||
        lease == null ||
        !selection.isValid ||
        !selection.isCollapsed ||
        _hasActiveComposition(value.composing)) {
      return false;
    }
    final caret = selection.extentOffset;
    return caret == 0 ||
        caret <= value.text.length && value.text.codeUnitAt(caret - 1) == 0x0a;
  }

  bool get canRemoveTightBulletListItemPrefixAtCaret =>
      canRemoveTightListItemPrefixAtCaret;

  /// Routes collapsed Backspace through the active parser-authored edit policy.
  ///
  /// Returns `null` when the caret is not at an authorized display-line start.
  /// A successful command changes canonical source while leaving display text
  /// unchanged, preserving the existing EditableText and platform client.
  FlarkV3FlutterSourceEditReceipt? removeTightListItemPrefixAtCaret() {
    if (!canRemoveTightListItemPrefixAtCaret) return null;
    final value = editingController.value;
    final caret = value.selection.extentOffset;
    final receipt = applyExactEdit(
      FlarkV3ExactFlutterEdit(
        localStartUtf16: caret,
        localEndUtf16: caret,
        replacement: '',
        nextSelection: TextSelection.collapsed(
          offset: caret,
          affinity: value.selection.affinity,
        ),
        nextComposing: TextRange.empty,
      ),
    );
    // The local command has mechanically removed the exact marker cut. Keep
    // the provisional projection for source/display mapping, but stop painting
    // a list gutter until the parser certifies the new structure.
    _paintBlockStyleLease = null;
    _schedulePresentationFrame();
    return receipt;
  }

  FlarkV3FlutterSourceEditReceipt? removeTightBulletListItemPrefixAtCaret() =>
      removeTightListItemPrefixAtCaret();

  /// Deletes the parser-certified atomic block currently selected for paint.
  ///
  /// The operation targets canonical source directly because an atomic block
  /// deliberately contributes no fake text to [EditableText]. The stable
  /// platform client is handed to a bounded source island at the former block
  /// boundary after the transaction.
  FlarkV3FlutterSourceEditReceipt? deleteActiveAtomicBlock({
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    final atom = _paintAtomicBlockLease;
    if (atom == null || !semanticActionsValid) return null;
    if (_hasActiveComposition(editingController.value.composing)) {
      throw StateError(
        'An atomic block cannot be deleted during active composition.',
      );
    }
    final start = atom.source.startUtf16;
    final end = atom.source.endUtf16;
    if (start < 0 || start >= end || end > source.utf16Length) {
      throw StateError('Atomic block source is no longer current.');
    }

    final oldSource = source;
    final nextDocumentLength = oldSource.utf16Length - (end - start);
    final nextGlobalEditingState = FlarkV3GlobalEditingState(
      selection: TextSelection.collapsed(offset: start),
      composing: TextRange.empty,
    );
    final nextRange = _planInputIslandRange(
      documentLength: nextDocumentLength,
      maximumUtf16: maximumInputIslandUtf16,
      editingState: nextGlobalEditingState,
      codeUnitAt: (offset) => _postEditCodeUnitAt(
        oldSource: oldSource,
        editStartUtf16: start,
        editEndUtf16: end,
        replacement: '',
        postEditOffset: offset,
      ),
    );
    final receipt = _sourceTransactionApplier(
      FlarkV3SourceTransaction.single(
        baseRevision: _inputIslandBaseRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: start,
          endUtf16: end,
          replacement: '',
        ),
      ),
    );
    if (source.utf16Length != nextDocumentLength) {
      throw StateError('Atomic deletion produced an unexpected source extent.');
    }
    final islandText = source.readRange(nextRange.start, nextRange.end);
    return _adoptAppliedSourceEdit(
      receipt: receipt,
      nextValue: _localEditingValue(
        text: islandText,
        islandStartUtf16: nextRange.start,
        global: nextGlobalEditingState,
      ),
      nextIslandStartUtf16: nextRange.start,
      nextGlobalEditingState: nextGlobalEditingState,
      editingValueAdopter: editingValueAdopter,
    );
  }

  /// Consumes Flutter's typed delta model without applying it to the old
  /// island string first. Oversized insertions/replacements route directly to
  /// [applyBulkEditAndHandoff]; ordinary deltas use the bounded exact lane.
  /// A non-text delta returns `null` after updating selection/composition.
  FlarkV3FlutterSourceEditReceipt? applyTextEditingDelta(
    TextEditingDelta delta, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    if (delta.oldText != editingController.text) {
      throw StateError('Text-input delta targets a stale input island.');
    }
    if (delta is TextEditingDeltaNonTextUpdate) {
      if (!_editingRangesFit(
        selection: delta.selection,
        composing: delta.composing,
        length: delta.oldText.length,
      )) {
        // Web may report the DOM's post-input selection immediately before
        // delivering the corresponding text delta. That non-text update has
        // no exact meaning against [oldText], so fail closed without aborting
        // the input channel; the following text delta carries the authority.
        return null;
      }
      final value = TextEditingValue(
        text: delta.oldText,
        selection: delta.selection,
        composing: delta.composing,
      );
      updateLocalEditingValue(value, editingValueAdopter: editingValueAdopter);
      return null;
    }

    late final int start;
    late final int end;
    late final String replacement;
    if (delta is TextEditingDeltaInsertion) {
      start = delta.insertionOffset;
      end = delta.insertionOffset;
      replacement = _platformDeltaReplacement(delta.textInserted);
    } else if (delta is TextEditingDeltaDeletion) {
      start = delta.deletedRange.start;
      end = delta.deletedRange.end;
      replacement = '';
    } else if (delta is TextEditingDeltaReplacement) {
      start = delta.replacedRange.start;
      end = delta.replacedRange.end;
      replacement = _platformDeltaReplacement(delta.replacementText);
    } else {
      throw ArgumentError.value(delta, 'delta', 'Unsupported delta subtype.');
    }
    final oldLength = delta.oldText.length;
    if (start < 0 || end < start || end > oldLength) {
      throw RangeError('Text-input delta escapes the current island.');
    }
    _validateUtf16Edit(
      oldText: delta.oldText,
      start: start,
      end: end,
      replacement: replacement,
      coordinateSpace: 'text-input delta',
    );
    final nextLocalLength = oldLength - (end - start) + replacement.length;
    _validateEditingRanges(
      selection: delta.selection,
      composing: delta.composing,
      length: nextLocalLength,
      coordinateSpace: 'delta result',
    );
    final nextValue = TextEditingValue(
      text: delta.oldText.replaceRange(start, end, replacement),
      selection: delta.selection,
      composing: delta.composing,
    );
    if (_projectedInputLease != null) {
      if (replacement.length >
              foregroundProfile.maximumOrdinaryReplacementUtf16 ||
          _inputIslandSourceLengthUtf16 -
                  (_projectedSourceRangeLength(start, end)) +
                  replacement.length >
              maximumInputIslandUtf16) {
        throw StateError(
          'Projected bulk edits require a bounded source-mapped handoff.',
        );
      }
      return _applyProjectedDisplayEdit(
        displayStartUtf16: start,
        displayEndUtf16: end,
        replacement: replacement,
        nextDisplayValue: nextValue,
        editingValueAdopter: editingValueAdopter,
      );
    }
    final nextGlobal = FlarkV3GlobalEditingState(
      selection: _shiftSelection(delta.selection, _inputIslandGlobalStartUtf16),
      composing: _shiftRange(delta.composing, _inputIslandGlobalStartUtf16),
    );
    if (replacement.length >
            foregroundProfile.maximumOrdinaryReplacementUtf16 ||
        nextLocalLength > maximumInputIslandUtf16) {
      return applyBulkEditAndHandoff(
        FlarkV3BulkFlutterEdit(
          localStartUtf16: start,
          localEndUtf16: end,
          replacement: replacement,
          nextGlobalEditingState: nextGlobal,
        ),
        editingValueAdopter: editingValueAdopter,
      );
    }
    return applyExactEdit(
      FlarkV3ExactFlutterEdit(
        localStartUtf16: start,
        localEndUtf16: end,
        replacement: replacement,
        nextSelection: nextValue.selection,
        nextComposing: nextValue.composing,
      ),
      editingValueAdopter: editingValueAdopter,
    );
  }

  /// Applies one platform delta callback as a single source transaction.
  ///
  /// Flutter may deliver several sequential deltas in one callback. Ordinary
  /// literal-source batches are validated and reduced to one bounded
  /// replacement. Projected batches advance a bounded mechanical source map
  /// per delta before committing one final source splice, so a malformed later
  /// delta cannot partially apply and hidden Markdown pieces remain intact.
  /// A bulk mutation must be the callback's only delta because its island
  /// handoff changes the coordinate space for subsequent deltas.
  FlarkV3FlutterSourceEditReceipt? applyTextEditingDeltas(
    List<TextEditingDelta> deltas, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    if (deltas.isEmpty) return null;
    _validatePlatformDeltaBatchWork(deltas);
    if (deltas.length == 1) {
      return applyTextEditingDelta(
        deltas.single,
        editingValueAdopter: editingValueAdopter,
      );
    }
    if (_projectedInputLease != null) {
      return _applyProjectedTextEditingDeltaBatch(
        deltas,
        editingValueAdopter: editingValueAdopter,
      );
    }

    var next = editingController.value;
    for (final delta in deltas) {
      next = _applyBoundedDeltaForBatch(
        current: next,
        delta: delta,
        maximumUtf16: maximumInputIslandUtf16,
      );
    }
    return applyPlatformEditingValue(
      next,
      editingValueAdopter: editingValueAdopter,
    );
  }

  void _validatePlatformDeltaBatchWork(List<TextEditingDelta> deltas) {
    if (deltas.length > foregroundProfile.maximumPlatformDeltasPerCallback) {
      throw StateError(
        'Text-input delta batch exceeds the sealed callback-count bound.',
      );
    }
    var cumulativeUtf16 = 0;
    for (final delta in deltas) {
      cumulativeUtf16 += delta.oldText.length + _inputIslandSourceLengthUtf16;
      if (delta is! TextEditingDeltaNonTextUpdate) {
        cumulativeUtf16 += _textEditFromDelta(delta).replacement.length;
      }
      if (cumulativeUtf16 >
          foregroundProfile.maximumPlatformDeltaBatchWorkUtf16) {
        throw StateError(
          'Text-input delta batch exceeds the sealed foreground-work bound.',
        );
      }
    }
  }

  /// Adopts a complete platform value without diffing document-scale source.
  ///
  /// Autofill and platform fallbacks may bypass Flutter's delta callback. The
  /// old value is one bounded input island, so the scalar-safe splice search
  /// examines bounded old text even if the incoming value is oversized. That
  /// oversized replacement then uses the provisional bulk handoff.
  FlarkV3FlutterSourceEditReceipt? applyPlatformEditingValue(
    TextEditingValue value, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    final current = editingController.value;
    if (!value.selection.isValid &&
        !value.composing.isValid &&
        value.text == current.text) {
      _adoptEditingValue(value, editingValueAdopter);
      return null;
    }
    _validateTextEditingValue(value);
    if (value.text == current.text) {
      updateLocalEditingValue(value, editingValueAdopter: editingValueAdopter);
      return null;
    }
    final splice = _boundedReplacement(current.text, value.text);
    return applyTextEditingDelta(
      TextEditingDeltaReplacement(
        oldText: current.text,
        replacementText: value.text.substring(splice.newStart, splice.newEnd),
        replacedRange: TextRange(start: splice.oldStart, end: splice.oldEnd),
        selection: value.selection,
        composing: value.composing,
      ),
      editingValueAdopter: editingValueAdopter,
    );
  }

  /// Applies an oversized edit without ever constructing the resulting
  /// document-scale Flutter string, then installs one bounded exact island
  /// around the post-edit selection extent and composing range.
  FlarkV3FlutterSourceEditReceipt applyBulkEditAndHandoff(
    FlarkV3BulkFlutterEdit edit, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    if (_projectedInputLease != null) {
      throw StateError(
        'Projected bulk edits require a bounded source-mapped handoff.',
      );
    }
    final beforeIsland = editingController.value;
    if (edit.localStartUtf16 < 0 ||
        edit.localEndUtf16 < edit.localStartUtf16 ||
        edit.localEndUtf16 > beforeIsland.text.length) {
      throw RangeError('Bulk edit escapes the bounded input island.');
    }
    final globalStart = _inputIslandGlobalStartUtf16 + edit.localStartUtf16;
    final globalEnd = _inputIslandGlobalStartUtf16 + edit.localEndUtf16;
    final nextDocumentLength =
        source.utf16Length -
        (globalEnd - globalStart) +
        edit.replacement.length;
    _validateGlobalEditingState(
      edit.nextGlobalEditingState,
      nextDocumentLength,
    );

    // All fallible island/state planning happens before source mutation. The
    // virtual accessor reads at most individual boundary code units and never
    // concatenates old source with the potentially giant replacement.
    final nextRange = _planInputIslandRange(
      documentLength: nextDocumentLength,
      maximumUtf16: maximumInputIslandUtf16,
      editingState: edit.nextGlobalEditingState,
      codeUnitAt: (offset) => _postEditCodeUnitAt(
        oldSource: source,
        editStartUtf16: globalStart,
        editEndUtf16: globalEnd,
        replacement: edit.replacement,
        postEditOffset: offset,
      ),
    );

    final receipt = _sourceTransactionApplier(
      FlarkV3SourceTransaction.single(
        baseRevision: _inputIslandBaseRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: globalStart,
          endUtf16: globalEnd,
          replacement: edit.replacement,
        ),
      ),
    );
    if (source.utf16Length != nextDocumentLength) {
      throw StateError('Bulk source adoption produced an unexpected extent.');
    }
    final islandText = source.readRange(nextRange.start, nextRange.end);
    final nextValue = _localEditingValue(
      text: islandText,
      islandStartUtf16: nextRange.start,
      global: edit.nextGlobalEditingState,
    );
    return _adoptAppliedSourceEdit(
      receipt: receipt,
      nextValue: nextValue,
      nextIslandStartUtf16: nextRange.start,
      nextGlobalEditingState: edit.nextGlobalEditingState,
      editingValueAdopter: editingValueAdopter,
    );
  }

  /// Moves the bounded EditableText shard without changing canonical source.
  /// Cross-island selection remains document-owned; Flutter receives a local
  /// collapsed proxy at the exact global extent. Active IME composition is
  /// preserved byte-for-byte and at the same global offsets.
  FlarkV3InputIslandHandoffReceipt handoffInputIsland(
    FlarkV3GlobalEditingState nextGlobalEditingState,
  ) {
    _validateGlobalEditingState(nextGlobalEditingState, source.utf16Length);
    final range = _planInputIslandRange(
      documentLength: source.utf16Length,
      maximumUtf16: maximumInputIslandUtf16,
      editingState: nextGlobalEditingState,
      codeUnitAt: (offset) =>
          source.readRange(offset, offset + 1).codeUnitAt(0),
    );
    return _handoffInputIslandToRange(
      range,
      nextGlobalEditingState: nextGlobalEditingState,
    );
  }

  /// Moves the stable EditableText shard to one exact parser-authorized range.
  ///
  /// Unlike [handoffInputIsland], this does not plan or widen the requested
  /// source window. The range must fit the sealed foreground bound, use safe
  /// scalar/CRLF edges, contain the global selection extent, and contain any
  /// active composition. Cross-island selection remains document-owned.
  FlarkV3InputIslandHandoffReceipt handoffInputIslandToExactRange({
    required int startUtf16,
    required int endUtf16,
    required FlarkV3GlobalEditingState nextGlobalEditingState,
  }) {
    _validateGlobalEditingState(nextGlobalEditingState, source.utf16Length);
    if (startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > source.utf16Length) {
      throw RangeError('Exact input-island range escapes canonical source.');
    }
    if (endUtf16 - startUtf16 > maximumInputIslandUtf16) {
      throw RangeError('Exact input-island range exceeds its sealed bound.');
    }
    int codeUnitAt(int offset) =>
        source.readRange(offset, offset + 1).codeUnitAt(0);
    if (!_safeIslandBoundary(startUtf16, source.utf16Length, codeUnitAt) ||
        !_safeIslandBoundary(endUtf16, source.utf16Length, codeUnitAt)) {
      throw RangeError(
        'Exact input-island range splits a scalar or CRLF boundary.',
      );
    }
    final extent = nextGlobalEditingState.selection.extentOffset;
    if (extent < startUtf16 || extent > endUtf16) {
      throw StateError(
        'Exact input-island range does not contain the selection extent.',
      );
    }
    final composing = nextGlobalEditingState.composing;
    if (composing.isValid &&
        (composing.start < startUtf16 || composing.end > endUtf16)) {
      throw StateError(
        'Exact input-island range does not contain active composition.',
      );
    }
    return _handoffInputIslandToRange(
      _InputIslandRange(startUtf16, endUtf16),
      nextGlobalEditingState: nextGlobalEditingState,
    );
  }

  /// Moves the stable input shard and installs its certified marker-free
  /// source/display map as one observable controller transition.
  FlarkV3InputIslandHandoffReceipt handoffProjectedInputIslandToExactRange({
    required FlarkV3ProjectedInputLease inputLease,
    required FlarkV3GlobalEditingState nextGlobalEditingState,
  }) {
    final startUtf16 = inputLease.sourceStartUtf16;
    final endUtf16 = inputLease.sourceEndUtf16;
    _validateGlobalEditingState(nextGlobalEditingState, source.utf16Length);
    if (!inputLease.isCertified ||
        !documentSession.currentUiSourceCertified ||
        inputLease.certifiedSourceVersion != sourceVersion ||
        startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > source.utf16Length ||
        endUtf16 - startUtf16 > maximumInputIslandUtf16) {
      throw StateError(
        'Projected input-island handoff lacks exact current authority.',
      );
    }
    int codeUnitAt(int offset) =>
        source.readRange(offset, offset + 1).codeUnitAt(0);
    if (!_safeIslandBoundary(startUtf16, source.utf16Length, codeUnitAt) ||
        !_safeIslandBoundary(endUtf16, source.utf16Length, codeUnitAt)) {
      throw RangeError(
        'Projected input-island range splits a scalar or CRLF boundary.',
      );
    }
    final extent = nextGlobalEditingState.selection.extentOffset;
    if (extent < startUtf16 || extent > endUtf16) {
      throw StateError(
        'Projected input-island range does not contain the selection extent.',
      );
    }
    if (_hasActiveComposition(editingController.value.composing) ||
        _hasActiveComposition(nextGlobalEditingState.composing)) {
      throw StateError(
        'Projected input-island handoff cannot cross active composition.',
      );
    }
    return _handoffInputIslandToRange(
      _InputIslandRange(startUtf16, endUtf16),
      nextGlobalEditingState: nextGlobalEditingState,
      projectedInputLease: inputLease,
    );
  }

  /// Moves the stable EditableText shard inside one parser-authorized range.
  ///
  /// If the complete range fits the foreground bound it becomes the island.
  /// Otherwise a scalar/CRLF-safe shard is selected around the caret and any
  /// active composition. The planner may shrink only within the supplied
  /// range; it cannot expose surrounding Markdown syntax or other blocks.
  FlarkV3InputIslandHandoffReceipt handoffInputIslandWithinExactRange({
    required int startUtf16,
    required int endUtf16,
    required FlarkV3GlobalEditingState nextGlobalEditingState,
  }) {
    _validateGlobalEditingState(nextGlobalEditingState, source.utf16Length);
    if (startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > source.utf16Length) {
      throw RangeError(
        'Parser-authorized input-island range escapes canonical source.',
      );
    }
    int codeUnitAtDocumentOffset(int offset) =>
        source.readRange(offset, offset + 1).codeUnitAt(0);
    if (!_safeIslandBoundary(
          startUtf16,
          source.utf16Length,
          codeUnitAtDocumentOffset,
        ) ||
        !_safeIslandBoundary(
          endUtf16,
          source.utf16Length,
          codeUnitAtDocumentOffset,
        )) {
      throw RangeError(
        'Parser-authorized input-island range splits a scalar or CRLF '
        'boundary.',
      );
    }

    final extent = nextGlobalEditingState.selection.extentOffset;
    if (extent < startUtf16 || extent > endUtf16) {
      throw StateError(
        'Parser-authorized input-island range does not contain the selection '
        'extent.',
      );
    }
    final composing = nextGlobalEditingState.composing;
    if (composing.isValid &&
        (composing.start < startUtf16 || composing.end > endUtf16)) {
      throw StateError(
        'Parser-authorized input-island range does not contain active '
        'composition.',
      );
    }

    final rangeLength = endUtf16 - startUtf16;
    if (rangeLength <= maximumInputIslandUtf16) {
      return _handoffInputIslandToRange(
        _InputIslandRange(startUtf16, endUtf16),
        nextGlobalEditingState: nextGlobalEditingState,
      );
    }

    final localSelection = TextSelection.collapsed(
      offset: extent - startUtf16,
      affinity: nextGlobalEditingState.selection.affinity,
    );
    final localComposing = composing.isValid
        ? TextRange(
            start: composing.start - startUtf16,
            end: composing.end - startUtf16,
          )
        : TextRange.empty;
    final localRange = _planInputIslandRange(
      documentLength: rangeLength,
      maximumUtf16: maximumInputIslandUtf16,
      editingState: FlarkV3GlobalEditingState(
        selection: localSelection,
        composing: localComposing,
      ),
      codeUnitAt: (offset) => source
          .readRange(startUtf16 + offset, startUtf16 + offset + 1)
          .codeUnitAt(0),
    );
    return _handoffInputIslandToRange(
      _InputIslandRange(
        startUtf16 + localRange.start,
        startUtf16 + localRange.end,
      ),
      nextGlobalEditingState: nextGlobalEditingState,
    );
  }

  FlarkV3InputIslandHandoffReceipt _handoffInputIslandToRange(
    _InputIslandRange range, {
    required FlarkV3GlobalEditingState nextGlobalEditingState,
    FlarkV3ProjectedInputLease? projectedInputLease,
  }) {
    if (_projectedInputLease != null &&
        _hasActiveComposition(editingController.value.composing)) {
      throw StateError(
        'Input-island handoff cannot change representation during composition.',
      );
    }
    final activeComposing = _globalEditingState.composing;
    if (activeComposing.isValid &&
        !activeComposing.isCollapsed &&
        nextGlobalEditingState.composing != activeComposing) {
      throw StateError(
        'Input-island handoff cannot change an active composing range.',
      );
    }
    if (projectedInputLease != null &&
        (projectedInputLease.sourceStartUtf16 != range.start ||
            projectedInputLease.sourceEndUtf16 != range.end ||
            projectedInputLease.certifiedSourceVersion != sourceVersion)) {
      throw StateError(
        'Projected input lease does not bind the handoff range.',
      );
    }
    final text =
        projectedInputLease?.displayText ??
        source.readRange(range.start, range.end);
    final nextValue = projectedInputLease == null
        ? _localEditingValue(
            text: text,
            islandStartUtf16: range.start,
            global: nextGlobalEditingState,
          )
        : TextEditingValue(
            text: text,
            selection:
                _selectionSpansOutsideIsland(
                  nextGlobalEditingState.selection,
                  range.start,
                  range.end,
                )
                ? TextSelection.collapsed(
                    offset: projectedInputLease.sourceToDisplayOffset(
                      nextGlobalEditingState.selection.extentOffset,
                    ),
                    affinity: nextGlobalEditingState.selection.affinity,
                  )
                : projectedInputLease.sourceSelectionToDisplay(
                    nextGlobalEditingState.selection,
                  ),
            composing: TextRange.empty,
          );
    if (activeComposing.isValid && !activeComposing.isCollapsed) {
      final oldLocalStart =
          activeComposing.start - _inputIslandGlobalStartUtf16;
      final oldLocalEnd = activeComposing.end - _inputIslandGlobalStartUtf16;
      final oldComposingText = editingController.text.substring(
        oldLocalStart,
        oldLocalEnd,
      );
      final newLocalStart = activeComposing.start - range.start;
      final newLocalEnd = activeComposing.end - range.start;
      if (text.substring(newLocalStart, newLocalEnd) != oldComposingText) {
        throw StateError('Input-island handoff changed active IME text.');
      }
    }
    final previousStart = _inputIslandGlobalStartUtf16;
    final changed =
        previousStart != range.start ||
        editingController.value != nextValue ||
        !identical(_projectedInputLease, projectedInputLease) ||
        !_sameGlobalEditingState(_globalEditingState, nextGlobalEditingState);
    _paintBlockStyleLease = null;
    _paintAtomicBlockLease = null;
    _recursiveGreenRowAck = null;
    _recursiveGreenRow = null;
    _installEditingState(
      value: nextValue,
      islandStartUtf16: range.start,
      global: nextGlobalEditingState,
      projectedInputLease: projectedInputLease,
    );
    _reconcilePaintLeases(_managedDocumentQuery);
    if (changed) _schedulePresentationFrame();
    return FlarkV3InputIslandHandoffReceipt(
      previousStartUtf16: previousStart,
      currentStartUtf16: range.start,
      currentEndUtf16: range.end,
      selectionSpansOutsideIsland: _selectionSpansOutsideIsland(
        nextGlobalEditingState.selection,
        range.start,
        range.end,
      ),
    );
  }

  FlarkV3FlutterSourceEditReceipt _adoptAppliedSourceEdit({
    required FlarkDocumentEditReceipt receipt,
    required TextEditingValue nextValue,
    required int nextIslandStartUtf16,
    required FlarkV3GlobalEditingState nextGlobalEditingState,
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    if (receipt.changed) {
      _paintAtomicBlockLease = null;
      _recursiveGreenRowAck = null;
      _recursiveGreenRow = null;
    }
    final editingChanged =
        editingController.value != nextValue ||
        _inputIslandGlobalStartUtf16 != nextIslandStartUtf16 ||
        !_sameGlobalEditingState(_globalEditingState, nextGlobalEditingState);
    _installEditingState(
      value: nextValue,
      islandStartUtf16: nextIslandStartUtf16,
      global: nextGlobalEditingState,
      editingValueAdopter: editingValueAdopter,
    );
    if (receipt.changed || editingChanged) _schedulePresentationFrame();
    return receipt;
  }

  void _installEditingState({
    required TextEditingValue value,
    required int islandStartUtf16,
    required FlarkV3GlobalEditingState global,
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
    FlarkV3ProjectedInputLease? projectedInputLease,
  }) {
    _validateTextEditingValue(value);
    _validateGlobalEditingState(global, source.utf16Length);
    final sourceLengthUtf16 =
        projectedInputLease?.sourceLengthUtf16 ?? value.text.length;
    if (islandStartUtf16 < 0 ||
        islandStartUtf16 + sourceLengthUtf16 > source.utf16Length ||
        sourceLengthUtf16 > maximumInputIslandUtf16 ||
        projectedInputLease != null &&
            (editingValueAdopter != null ||
                !projectedInputLease.isCertified ||
                projectedInputLease.certifiedSourceVersion != sourceVersion ||
                projectedInputLease.sourceStartUtf16 != islandStartUtf16 ||
                projectedInputLease.sourceEndUtf16 !=
                    islandStartUtf16 + sourceLengthUtf16 ||
                projectedInputLease.displayText != value.text)) {
      throw RangeError('Installed input island escapes canonical source.');
    }
    final controller = editingController as FlarkV3InlineTextEditingController;
    if (projectedInputLease == null) controller.clearProjectedInputLease();
    _inputIslandBaseRevision = documentSession.uiRevision;
    _inputIslandGlobalStartUtf16 = islandStartUtf16;
    _inputIslandSourceLengthUtf16 = sourceLengthUtf16;
    _globalEditingState = global;
    _pendingInlinePresentation = null;
    _pendingLiteralSourcePaint = false;
    if (projectedInputLease == null) {
      _adoptEditingValue(value, editingValueAdopter);
    } else {
      controller.adoptProjectedEditingValue(value, projectedInputLease);
    }
  }

  /// Leases one bounded source-replica unit to a worker.
  ///
  /// The controller deliberately does not manufacture the acknowledgement;
  /// only an external worker may return the exact typed receipt.
  FlarkV3SourceWorkerSyncLease beginSourceWorkerSync({
    int maximumEntries = 64,
    int maximumOperations = 1024,
    int maximumPayloadUtf16 = 8192,
    int maximumSnapshotPageUtf16 = 8192,
  }) => documentSession.beginSourceWorkerSync(
    maximumEntries: maximumEntries,
    maximumOperations: maximumOperations,
    maximumPayloadUtf16: maximumPayloadUtf16,
    maximumSnapshotPageUtf16: maximumSnapshotPageUtf16,
  );

  FlarkV3SourceWorkerSyncAckReceipt acknowledgeSourceWorkerSync(
    FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
  ) => documentSession.acknowledgeSourceWorkerSync(acknowledgement);

  bool releaseSourceWorkerSyncLease(int leaseId) =>
      documentSession.releaseSourceWorkerSyncLease(leaseId);

  FlarkV3FlutterSourceWorkerRestartReceipt restartSourceWorker() =>
      documentSession.restartSourceWorker();

  /// Starts bounded derived-fact certification after replica sync completes.
  FlarkV3SourceCertificationRequest beginSourceCertification({
    int maximumPieceDescriptors = 64,
    int maximumDiscoveryNodes = 512,
  }) => documentSession.beginSourceCertification(
    maximumPieceDescriptors: maximumPieceDescriptors,
    maximumDiscoveryNodes: maximumDiscoveryNodes,
  );

  FlarkV3FlutterCertificationAdoptionReceipt applySourceCertification(
    FlarkV3SourceCertificationReceipt receipt,
  ) {
    final result = documentSession.applySourceCertification(receipt);
    if (result.hostAdoption != null) _schedulePresentationFrame();
    return result;
  }

  /// Updates local selection/composition without reparsing or changing source.
  void updateLocalEditingValue(
    TextEditingValue value, {
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  }) {
    if (value.text != editingController.text) {
      throw ArgumentError('Selection-only update cannot replace island text.');
    }
    _validateTextEditingValue(value);
    if (value == editingController.value) return;
    final lease = _projectedInputLease;
    if (lease == null) {
      _globalEditingState = _globalEditingStateFromLocal(
        value,
        _inputIslandGlobalStartUtf16,
      );
    } else {
      _globalEditingState = FlarkV3GlobalEditingState(
        selection: lease.displaySelectionToSource(
          value.selection,
          preferredSourceSelection: _globalEditingState.selection,
        ),
        composing: lease.displayComposingToSource(
          value.composing,
          preferredSourceComposing: _globalEditingState.composing,
        ),
      );
    }
    _adoptEditingValue(value, editingValueAdopter);
    _tryAdoptPendingInlinePresentation();
    _schedulePresentationFrame();
  }

  void _adoptEditingValue(
    TextEditingValue value,
    FlarkV3FlutterEditingValueAdopter? editingValueAdopter,
  ) {
    if (editingValueAdopter == null) {
      editingController.value = value;
      return;
    }
    editingValueAdopter(value);
    if (editingController.value != value) {
      throw StateError(
        'Flutter did not adopt the exact source-validated platform value.',
      );
    }
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    return documentSession.beginOffer(begin);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => documentSession.admitPacket(packet);

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => documentSession.requestCommit(request);

  FlarkV3HostCallResult<FlarkV3HostPollOutcome> pollHost(
    FlarkV3HostWorkGrant grant,
  ) {
    final result = documentSession.pollHost(grant);
    if (result is FlarkV3HostAccepted<FlarkV3HostPollOutcome> &&
        result.value is FlarkV3HostCommitted) {
      _schedulePresentationFrame();
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => documentSession.acknowledgeDelivery(ack);

  FlarkV3HostCallResult<FlarkV3HostUnit> resynchronizeHost() =>
      documentSession.resynchronizeHost();

  /// Frame-coalesced presentation edge for an external Dart session executor.
  ///
  /// Pass this method as the executor's progress callback. Parser work,
  /// SourceFacts delta promotion, and structural publication remain owned by
  /// the Dart runtime; Flutter only re-reads the already-committed document
  /// presentation on its next frame.
  void handleSessionExecutorProgress() {
    _schedulePresentationFrame();
  }

  void _schedulePresentationFrame() {
    if (_frameScheduled || _disposed) return;
    _frameScheduled = true;
    _scheduledFrameCallbacks += 1;
    _frameScheduler.schedule(() {
      if (_disposed) return;
      _frameScheduled = false;
      _paintState = _derivePaintState();
      _appliedPresentationFrames += 1;
      notifyListeners();
    });
  }

  FlarkV3FlutterPaintState _derivePaintState() {
    final presentation = documentSession.presentationState;
    if (presentation case FlarkV3StablePendingPresentation(
      :final uiSource,
      :final stablePaintAck,
      :final uiSourceGap,
      :final sourceGap,
    )) {
      if (stablePaintAck != null) {
        return FlarkV3FlutterPaintState.stablePaint(
          uiSource: uiSource,
          sourceVersion: sourceVersion,
          ack: stablePaintAck,
          uiSourceGap: uiSourceGap,
          sourceGap: sourceGap,
          blockStyleLease: _paintBlockStyleLease,
          atomicBlockLease: _paintAtomicBlockLease,
        );
      }
      return FlarkV3FlutterPaintState.sourceGap(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        uiSourceGap: uiSourceGap,
        sourceGap: sourceGap,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }

    final exact = presentation as FlarkV3ExactStructuralPresentation;
    if (pointQueryOwnership ==
        FlarkV3FlutterPointQueryOwnership.managedCoordinator) {
      return _deriveManagedPaintState(exact);
    }
    final position = _queryPosition();
    if (position == null) {
      return FlarkV3FlutterPaintState.sourceGap(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        uiSourceGap: _wholeUiSourceGap(),
        sourceGap: _wholeSourceGap(),
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    final query = documentSession.query(
      FlarkV3HostPointQuery(
        sourceVersion: sourceVersion,
        position: position,
        budget: queryBudget,
      ),
    );
    if (query case FlarkV3HostAccepted<FlarkV3HostPresentationQuery>(
      value: FlarkV3StructuralPresentationQuery(:final viewport),
    )) {
      return FlarkV3FlutterPaintState.exactStructural(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        ack: exact.ack,
        viewport: viewport,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    if (query case FlarkV3HostAccepted<FlarkV3HostPresentationQuery>(
      value: FlarkV3SourceGapPresentationQuery(:final gap),
    )) {
      return FlarkV3FlutterPaintState.sourceGap(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        uiSourceGap: _wholeUiSourceGap(),
        sourceGap: gap,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    return FlarkV3FlutterPaintState.sourceGap(
      uiSource: uiSource,
      sourceVersion: sourceVersion,
      uiSourceGap: _wholeUiSourceGap(),
      sourceGap: FlarkV3SourceGap(
        sourceVersion: sourceVersion,
        range: FlarkV3MetricRange(
          start: FlarkV3SourceMetric.zero,
          end: sourceVersion.metric,
        ),
      ),
      blockStyleLease: _paintBlockStyleLease,
      atomicBlockLease: _paintAtomicBlockLease,
    );
  }

  FlarkV3FlutterPaintState _deriveManagedPaintState(
    FlarkV3ExactStructuralPresentation exact,
  ) {
    final query = _managedDocumentQuery;
    if (query
        case FlarkV3DocumentStructuralQuery(
          :final sourceRevision,
          :final structureRevision,
        )
        when _managedDocumentQueryAck == exact.ack &&
            sourceRevision == sourceVersion.revision &&
            structureRevision == sourceVersion.revision &&
            _managedQueryCoversCurrentPoint(query)) {
      return FlarkV3FlutterPaintState.exactManagedStructural(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        ack: exact.ack,
        documentQuery: query,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    if (query
        case FlarkV3RecursiveGreenPointQuery(
          :final sourceRevision,
          :final structureRevision,
        )
        when _managedDocumentQueryAck == exact.ack &&
            sourceRevision == sourceVersion.revision &&
            structureRevision == sourceVersion.revision &&
            ((query.owner.kind?.isInlineBearing ?? false) ||
                _recursiveGreenTerminalEmptyRowAuthorizesCurrentInput(
                  query,
                  exact.ack,
                ) ||
                _recursiveGreenFencedRowAuthorizesCurrentInput(
                  query,
                  exact.ack,
                )) &&
            _managedQueryCoversCurrentPoint(query)) {
      return FlarkV3FlutterPaintState.exactManagedRecursive(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        ack: exact.ack,
        documentQuery: query,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    if (query
        case FlarkV3DocumentSourceGapQuery(
          :final sourceRevision,
          :final structureRevision,
          :final range,
          :final reason,
        )
        when _managedDocumentQueryAck == exact.ack &&
            sourceRevision == sourceVersion.revision &&
            structureRevision == sourceVersion.revision &&
            _managedQueryCoversCurrentPoint(query)) {
      return FlarkV3FlutterPaintState.sourceGap(
        uiSource: uiSource,
        sourceVersion: sourceVersion,
        uiSourceGap: _wholeUiSourceGap(),
        sourceGap: FlarkV3SourceGap(
          sourceVersion: sourceVersion,
          range: FlarkV3MetricRange(
            start: FlarkV3SourceMetric(
              bytes: range.startUtf8,
              utf16: range.startUtf16,
            ),
            end: FlarkV3SourceMetric(
              bytes: range.endUtf8,
              utf16: range.endUtf16,
            ),
          ),
          structuralReason: _hostGapReason(reason),
        ),
        documentQuery: query,
        blockStyleLease: _paintBlockStyleLease,
        atomicBlockLease: _paintAtomicBlockLease,
      );
    }
    return FlarkV3FlutterPaintState.sourceGap(
      uiSource: uiSource,
      sourceVersion: sourceVersion,
      uiSourceGap: _wholeUiSourceGap(),
      sourceGap: _wholeSourceGap(),
      blockStyleLease: _paintBlockStyleLease,
      atomicBlockLease: _paintAtomicBlockLease,
    );
  }

  bool _managedQueryCoversCurrentPoint(FlarkV3DocumentQueryResult query) {
    final selection = _globalEditingState.selection;
    final position = selection.extentOffset;
    if (position == _managedDocumentQueryPositionUtf16 &&
        selection.affinity == _managedDocumentQueryAffinity) {
      return true;
    }
    if (query is! FlarkV3DocumentStructuralQuery) return false;
    if (query.structure case FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.thematicBreak,
      source: final atom,
      thematicBreak: FlarkV3ThematicBreakFacts(),
    )) {
      return position == atom.startUtf16 &&
              selection.affinity == TextAffinity.downstream ||
          position == atom.endUtf16 &&
              selection.affinity == TextAffinity.upstream;
    }
    final leaf = query.projection.projectedSource;
    return position > leaf.startUtf16 && position < leaf.endUtf16;
  }

  FlarkV3SourceMetric? _queryPosition() {
    if (!documentSession.currentUiSourceCertified) {
      return null;
    }
    final utf16 = _globalEditingState.selection.extentOffset;
    try {
      return FlarkV3SourceMetric(
        bytes: source.utf16ToUtf8(utf16),
        utf16: utf16,
      );
    } on RangeError {
      return null;
    } on StateError {
      return null;
    } on FormatException {
      return null;
    }
  }

  FlarkV3SourceGap _wholeSourceGap() => FlarkV3SourceGap(
    sourceVersion: sourceVersion,
    range: FlarkV3MetricRange(
      start: FlarkV3SourceMetric.zero,
      end: sourceVersion.metric,
    ),
  );

  FlarkV3UiSourceGap _wholeUiSourceGap() => FlarkV3UiSourceGap.whole(uiSource);

  @override
  void dispose() {
    _disposed = true;
    editingController.dispose();
    super.dispose();
  }
}

FlarkV3HostSourceGapReason _hostGapReason(
  FlarkV3DocumentQueryGapReason reason,
) => switch (reason) {
  FlarkV3DocumentQueryGapReason.openDepthLimit =>
    FlarkV3HostSourceGapReason.openDepthLimit,
  FlarkV3DocumentQueryGapReason.encodedByteLimit =>
    FlarkV3HostSourceGapReason.encodedByteLimit,
  FlarkV3DocumentQueryGapReason.leafLimit =>
    FlarkV3HostSourceGapReason.leafLimit,
  FlarkV3DocumentQueryGapReason.treeNodeLimit =>
    FlarkV3HostSourceGapReason.treeNodeLimit,
  FlarkV3DocumentQueryGapReason.undecodableClosure =>
    FlarkV3HostSourceGapReason.undecodableClosure,
  FlarkV3DocumentQueryGapReason.unavailableFacts =>
    FlarkV3HostSourceGapReason.unavailableFacts,
};

TextEditingValue _applyBoundedDeltaForBatch({
  required TextEditingValue current,
  required TextEditingDelta delta,
  required int maximumUtf16,
}) {
  if (current.text.length > maximumUtf16 || delta.oldText != current.text) {
    throw StateError('Text-input delta batch targets a stale input island.');
  }
  if (delta is TextEditingDeltaNonTextUpdate) {
    if (!_editingRangesFit(
      selection: delta.selection,
      composing: delta.composing,
      length: current.text.length,
    )) {
      return current;
    }
    final next = TextEditingValue(
      text: current.text,
      selection: delta.selection,
      composing: delta.composing,
    );
    _validateTextEditingValue(next);
    return next;
  }

  late final int start;
  late final int end;
  late final String replacement;
  if (delta is TextEditingDeltaInsertion) {
    start = delta.insertionOffset;
    end = delta.insertionOffset;
    replacement = _platformDeltaReplacement(delta.textInserted);
  } else if (delta is TextEditingDeltaDeletion) {
    start = delta.deletedRange.start;
    end = delta.deletedRange.end;
    replacement = '';
  } else if (delta is TextEditingDeltaReplacement) {
    start = delta.replacedRange.start;
    end = delta.replacedRange.end;
    replacement = _platformDeltaReplacement(delta.replacementText);
  } else {
    throw ArgumentError.value(delta, 'delta', 'Unsupported delta subtype.');
  }
  if (start < 0 || end < start || end > current.text.length) {
    throw RangeError('Text-input delta batch has an invalid edit boundary.');
  }
  _validateUtf16Edit(
    oldText: current.text,
    start: start,
    end: end,
    replacement: replacement,
    coordinateSpace: 'text-input delta batch',
  );
  final nextLength = current.text.length - (end - start) + replacement.length;
  if (nextLength > maximumUtf16) {
    throw StateError(
      'A bulk text-input delta must be the callback\'s only delta.',
    );
  }
  final next = TextEditingValue(
    text: current.text.replaceRange(start, end, replacement),
    selection: delta.selection,
    composing: delta.composing,
  );
  _validateTextEditingValue(next);
  return next;
}

final class _TextDeltaEdit {
  const _TextDeltaEdit({
    required this.start,
    required this.end,
    required this.replacement,
  });

  final int start;
  final int end;
  final String replacement;
}

_TextDeltaEdit _textEditFromDelta(TextEditingDelta delta) {
  if (delta is TextEditingDeltaInsertion) {
    return _TextDeltaEdit(
      start: delta.insertionOffset,
      end: delta.insertionOffset,
      replacement: _platformDeltaReplacement(delta.textInserted),
    );
  }
  if (delta is TextEditingDeltaDeletion) {
    return _TextDeltaEdit(
      start: delta.deletedRange.start,
      end: delta.deletedRange.end,
      replacement: '',
    );
  }
  if (delta is TextEditingDeltaReplacement) {
    return _TextDeltaEdit(
      start: delta.replacedRange.start,
      end: delta.replacedRange.end,
      replacement: _platformDeltaReplacement(delta.replacementText),
    );
  }
  throw ArgumentError.value(delta, 'delta', 'Expected a text delta subtype.');
}

/// Flutter Web reports a physical multiline Enter as one carriage-return
/// insertion even though Flutter's editable value model is LF-normalized.
///
/// Normalize only that exact platform sentinel. Pasted CRLF and ordinary
/// replacement payloads remain byte-for-byte source input.
String _platformDeltaReplacement(String replacement) =>
    replacement == '\r' ? '\n' : replacement;

final class _BoundedReplacement {
  const _BoundedReplacement({
    required this.oldStart,
    required this.oldEnd,
    required this.newStart,
    required this.newEnd,
  });

  final int oldStart;
  final int oldEnd;
  final int newStart;
  final int newEnd;
}

_BoundedReplacement _boundedReplacement(String oldText, String newText) {
  var start = 0;
  final sharedLength = oldText.length < newText.length
      ? oldText.length
      : newText.length;
  while (start < sharedLength &&
      oldText.codeUnitAt(start) == newText.codeUnitAt(start)) {
    start += 1;
  }
  while (start > 0 &&
      (!_isUtf16Boundary(oldText, start) ||
          !_isUtf16Boundary(newText, start))) {
    start -= 1;
  }

  var oldEnd = oldText.length;
  var newEnd = newText.length;
  while (oldEnd > start &&
      newEnd > start &&
      oldText.codeUnitAt(oldEnd - 1) == newText.codeUnitAt(newEnd - 1)) {
    oldEnd -= 1;
    newEnd -= 1;
  }
  if (!_isUtf16Boundary(oldText, oldEnd) ||
      !_isUtf16Boundary(newText, newEnd)) {
    // A common suffix beginning inside a surrogate pair is not a valid edit
    // boundary. Including the bounded remainder is conservative and exact.
    oldEnd = oldText.length;
    newEnd = newText.length;
  }
  return _BoundedReplacement(
    oldStart: start,
    oldEnd: oldEnd,
    newStart: start,
    newEnd: newEnd,
  );
}

bool _isUtf16Boundary(String text, int offset) {
  if (offset <= 0 || offset >= text.length) return true;
  return !(_isHighSurrogate(text.codeUnitAt(offset - 1)) &&
      _isLowSurrogate(text.codeUnitAt(offset)));
}

void _validateUtf16Edit({
  required String oldText,
  required int start,
  required int end,
  required String replacement,
  required String coordinateSpace,
}) {
  if (!_isUtf16Boundary(oldText, start) ||
      !_isUtf16Boundary(oldText, end) ||
      !_isWellFormedUtf16(replacement)) {
    throw RangeError('$coordinateSpace splits or introduces invalid UTF-16.');
  }
}

bool _isWellFormedUtf16(String text) {
  for (var offset = 0; offset < text.length; offset += 1) {
    final codeUnit = text.codeUnitAt(offset);
    if (_isHighSurrogate(codeUnit)) {
      if (offset + 1 >= text.length ||
          !_isLowSurrogate(text.codeUnitAt(offset + 1))) {
        return false;
      }
      offset += 1;
    } else if (_isLowSurrogate(codeUnit)) {
      return false;
    }
  }
  return true;
}

final class _InputIslandRange {
  const _InputIslandRange(this.start, this.end);

  final int start;
  final int end;
}

FlarkV3GlobalEditingState _globalEditingStateFromLocal(
  TextEditingValue value,
  int islandStartUtf16,
) => FlarkV3GlobalEditingState(
  selection: _shiftSelection(value.selection, islandStartUtf16),
  composing: _shiftRange(value.composing, islandStartUtf16),
);

TextSelection _shiftSelection(TextSelection selection, int delta) =>
    TextSelection(
      baseOffset: delta + selection.baseOffset,
      extentOffset: delta + selection.extentOffset,
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    );

TextRange _shiftRange(TextRange range, int delta) => range.isValid
    ? TextRange(start: delta + range.start, end: delta + range.end)
    : TextRange.empty;

TextEditingValue _localEditingValue({
  required String text,
  required int islandStartUtf16,
  required FlarkV3GlobalEditingState global,
}) {
  final islandEndUtf16 = islandStartUtf16 + text.length;
  final selection = global.selection;
  final localSelection =
      _selectionSpansOutsideIsland(selection, islandStartUtf16, islandEndUtf16)
      ? TextSelection.collapsed(
          offset: selection.extentOffset - islandStartUtf16,
          affinity: selection.affinity,
        )
      : TextSelection(
          baseOffset: selection.baseOffset - islandStartUtf16,
          extentOffset: selection.extentOffset - islandStartUtf16,
          affinity: selection.affinity,
          isDirectional: selection.isDirectional,
        );
  final composing = global.composing.isValid
      ? TextRange(
          start: global.composing.start - islandStartUtf16,
          end: global.composing.end - islandStartUtf16,
        )
      : TextRange.empty;
  final value = TextEditingValue(
    text: text,
    selection: localSelection,
    composing: composing,
  );
  _validateTextEditingValue(value);
  return value;
}

bool _selectionSpansOutsideIsland(
  TextSelection selection,
  int islandStartUtf16,
  int islandEndUtf16,
) =>
    selection.baseOffset < islandStartUtf16 ||
    selection.baseOffset > islandEndUtf16 ||
    selection.extentOffset < islandStartUtf16 ||
    selection.extentOffset > islandEndUtf16;

bool _sameGlobalEditingState(
  FlarkV3GlobalEditingState left,
  FlarkV3GlobalEditingState right,
) => left.selection == right.selection && left.composing == right.composing;

void _validateGlobalEditingState(
  FlarkV3GlobalEditingState state,
  int documentLength,
) {
  final selection = state.selection;
  if (!selection.isValid ||
      selection.start > documentLength ||
      selection.end > documentLength) {
    throw RangeError('Global selection escapes canonical source.');
  }
  final composing = state.composing;
  if (composing.isValid &&
      (composing.start < 0 ||
          composing.end < composing.start ||
          composing.end > documentLength)) {
    throw RangeError('Global composing range escapes canonical source.');
  }
}

_InputIslandRange _planInputIslandRange({
  required int documentLength,
  required int maximumUtf16,
  required FlarkV3GlobalEditingState editingState,
  required int Function(int offset) codeUnitAt,
}) {
  _validateGlobalEditingState(editingState, documentLength);
  final composing = editingState.composing;
  final extent = editingState.selection.extentOffset;
  final requiredStart = composing.isValid
      ? (extent < composing.start ? extent : composing.start)
      : extent;
  final requiredEnd = composing.isValid
      ? (extent > composing.end ? extent : composing.end)
      : extent;
  final requiredSpan = requiredEnd - requiredStart;
  if (requiredSpan > maximumUtf16) {
    throw StateError(
      'Selection extent and composing range cannot fit one input island.',
    );
  }
  if (documentLength <= maximumUtf16) {
    return _InputIslandRange(0, documentLength);
  }

  // Try the full sealed span, then at most two units less. That is sufficient
  // to move either edge across one UTF-16 scalar pair or CRLF boundary while
  // retaining a deterministic fixed amount of planning work.
  for (var shrink = 0; shrink <= 2; shrink += 1) {
    final span = maximumUtf16 - shrink;
    if (span < requiredSpan) continue;
    final minimumStart = (requiredEnd - span).clamp(0, documentLength - span);
    final maximumStart = requiredStart.clamp(0, documentLength - span);
    if (minimumStart > maximumStart) continue;
    final centered = (requiredStart - ((span - requiredSpan) ~/ 2)).clamp(
      minimumStart,
      maximumStart,
    );
    final candidates = <int>{
      centered,
      minimumStart,
      maximumStart,
      (centered - 1).clamp(minimumStart, maximumStart),
      (centered + 1).clamp(minimumStart, maximumStart),
      (centered - 2).clamp(minimumStart, maximumStart),
      (centered + 2).clamp(minimumStart, maximumStart),
    };
    for (final start in candidates) {
      final end = start + span;
      if (_safeIslandBoundary(start, documentLength, codeUnitAt) &&
          _safeIslandBoundary(end, documentLength, codeUnitAt)) {
        return _InputIslandRange(start, end);
      }
    }
  }
  throw StateError(
    'No scalar/CRLF-safe bounded input island contains composition and caret.',
  );
}

bool _safeIslandBoundary(
  int offset,
  int documentLength,
  int Function(int offset) codeUnitAt,
) {
  if (offset == 0 || offset == documentLength) return true;
  final previous = codeUnitAt(offset - 1);
  final next = codeUnitAt(offset);
  return !((_isHighSurrogate(previous) && _isLowSurrogate(next)) ||
      (previous == 0x0D && next == 0x0A));
}

int _postEditCodeUnitAt({
  required FlarkV3SourceDocument oldSource,
  required int editStartUtf16,
  required int editEndUtf16,
  required String replacement,
  required int postEditOffset,
}) {
  if (postEditOffset < editStartUtf16) {
    return oldSource
        .readRange(postEditOffset, postEditOffset + 1)
        .codeUnitAt(0);
  }
  final replacementEnd = editStartUtf16 + replacement.length;
  if (postEditOffset < replacementEnd) {
    return replacement.codeUnitAt(postEditOffset - editStartUtf16);
  }
  final oldOffset =
      postEditOffset - replacement.length + (editEndUtf16 - editStartUtf16);
  return oldSource.readRange(oldOffset, oldOffset + 1).codeUnitAt(0);
}

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xD800 && codeUnit <= 0xDBFF;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xDC00 && codeUnit <= 0xDFFF;

bool _hasActiveComposition(TextRange range) =>
    range.isValid && !range.isCollapsed;

void _validateTextEditingValue(TextEditingValue value) {
  _validateEditingRanges(
    selection: value.selection,
    composing: value.composing,
    length: value.text.length,
    coordinateSpace: 'exact source value',
  );
}

void _validateEditingRanges({
  required TextSelection selection,
  required TextRange composing,
  required int length,
  required String coordinateSpace,
}) {
  if (!selection.isValid ||
      selection.start > length ||
      selection.end > length) {
    throw RangeError(
      'Text selection ${selection.baseOffset}..${selection.extentOffset} '
      'escapes the $coordinateSpace of length $length.',
    );
  }
  if (composing.isValid && composing.end > length) {
    throw RangeError(
      'Composing range ${composing.start}..${composing.end} escapes the '
      '$coordinateSpace of length $length.',
    );
  }
}

bool _recursiveGreenQueryMatchesRow(
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3RecursiveGreenRenderableRow? row,
) {
  final editableSource = row?.editableSource;
  if (row == null ||
      editableSource == null ||
      query.owner.frameId != row.frameId ||
      query.owner.kind != row.kind ||
      query.ancestry.length != row.path.length) {
    return false;
  }
  for (var index = 0; index < row.path.length; index += 1) {
    final queryFrame = query.ancestry[index];
    final rowFrame = row.path[index];
    if (queryFrame.frameId != rowFrame.frameId ||
        queryFrame.kind != rowFrame.kind) {
      return false;
    }
  }
  if (row.kind.isInlineBearing) {
    return _sameOptionalSourceSpan(query.paragraphSource, row.physicalSource) &&
        _sameOptionalSourceSpan(query.inlineSource, editableSource);
  }
  if (row.kind.isTerminalEmptyItem) {
    return row.presentationKind ==
            FlarkV3RecursiveGreenRowPresentationKind.inline &&
        row.editCapability ==
            FlarkV3RecursiveGreenRowEditCapability.contiguous &&
        !row.inlineCapable &&
        query.isIdentityEditableContent &&
        query.pointUtf8 == editableSource.startUtf8 &&
        query.pointUtf16 == editableSource.startUtf16 &&
        query.paragraphSource == null &&
        query.inlineSource == null &&
        query.inlineFacts == null &&
        _sourceSpanContainsSpan(row.presentationPhysicalSource, query.source);
  }
  return row.kind == FlarkV3RecursiveGreenKind.fencedCode &&
      row.presentationKind ==
          FlarkV3RecursiveGreenRowPresentationKind.fencedCode &&
      row.literal &&
      row.editCapability == FlarkV3RecursiveGreenRowEditCapability.contiguous &&
      query.isIdentityEditableContent &&
      _sourceSpanContainsSpan(editableSource, query.source);
}

bool _sourceSpanContainsSpan(
  FlarkV3SourceSpan outer,
  FlarkV3SourceSpan inner,
) =>
    inner.startUtf8 >= outer.startUtf8 &&
    inner.endUtf8 <= outer.endUtf8 &&
    inner.startUtf16 >= outer.startUtf16 &&
    inner.endUtf16 <= outer.endUtf16;

bool _sameRecursiveGreenRow(
  FlarkV3RecursiveGreenRenderableRow? left,
  FlarkV3RecursiveGreenRenderableRow right,
) {
  if (left == null ||
      left.globalOrdinal != right.globalOrdinal ||
      left.frameId != right.frameId ||
      left.kind != right.kind ||
      left.selected != right.selected ||
      left.inlineCapable != right.inlineCapable ||
      left.literal != right.literal ||
      left.presentationKind != right.presentationKind ||
      left.editCapability != right.editCapability ||
      !_sameSourceSpan(left.physicalSource, right.physicalSource) ||
      !_sameNullableSourceSpan(left.editableSource, right.editableSource) ||
      left.path.length != right.path.length) {
    return false;
  }
  for (var index = 0; index < left.path.length; index += 1) {
    final a = left.path[index];
    final b = right.path[index];
    if (a.frameId != b.frameId ||
        a.kind != b.kind ||
        !_sameSourceSpan(a.physicalSource, b.physicalSource) ||
        a.isRowOwner != b.isRowOwner ||
        a.isContainer != b.isContainer ||
        a.hasOpenFact != b.hasOpenFact ||
        a.hasCloseFact != b.hasCloseFact ||
        !_sameRecursiveGreenPathFact(a.fact, b.fact)) {
      return false;
    }
  }
  return true;
}

bool _sameNullableSourceSpan(
  FlarkV3SourceSpan? left,
  FlarkV3SourceSpan? right,
) => left == null
    ? right == null
    : right != null && _sameSourceSpan(left, right);

bool _sameRecursiveGreenPathFact(
  FlarkV3RecursiveGreenPathFact? left,
  FlarkV3RecursiveGreenPathFact? right,
) => switch ((left, right)) {
  (null, null) => true,
  (
    FlarkV3RecursiveGreenListPathFact(
      style: final leftStyle,
      bulletMarker: final leftBulletMarker,
      orderedDelimiter: final leftOrderedDelimiter,
      start: final leftStart,
      tight: final leftTight,
    ),
    FlarkV3RecursiveGreenListPathFact(
      style: final rightStyle,
      bulletMarker: final rightBulletMarker,
      orderedDelimiter: final rightOrderedDelimiter,
      start: final rightStart,
      tight: final rightTight,
    ),
  ) =>
    leftStyle == rightStyle &&
        leftBulletMarker == rightBulletMarker &&
        leftOrderedDelimiter == rightOrderedDelimiter &&
        leftStart == rightStart &&
        leftTight == rightTight,
  (
    FlarkV3RecursiveGreenItemPathFact(
      markerOffset: final leftMarkerOffset,
      padding: final leftPadding,
    ),
    FlarkV3RecursiveGreenItemPathFact(
      markerOffset: final rightMarkerOffset,
      padding: final rightPadding,
    ),
  ) =>
    leftMarkerOffset == rightMarkerOffset && leftPadding == rightPadding,
  (
    FlarkV3RecursiveGreenHeadingPathFact(
      level: final leftLevel,
      style: final leftStyle,
    ),
    FlarkV3RecursiveGreenHeadingPathFact(
      level: final rightLevel,
      style: final rightStyle,
    ),
  ) =>
    leftLevel == rightLevel && leftStyle == rightStyle,
  (
    FlarkV3RecursiveGreenCodePathFact(
      marker: final leftMarker,
      fenceOffsetColumns: final leftFenceOffset,
      minimumClosingLength: final leftMinimumClosingLength,
    ),
    FlarkV3RecursiveGreenCodePathFact(
      marker: final rightMarker,
      fenceOffsetColumns: final rightFenceOffset,
      minimumClosingLength: final rightMinimumClosingLength,
    ),
  ) =>
    leftMarker == rightMarker &&
        leftFenceOffset == rightFenceOffset &&
        leftMinimumClosingLength == rightMinimumClosingLength,
  (
    FlarkV3RecursiveGreenHtmlPathFact(blockType: final leftBlockType),
    FlarkV3RecursiveGreenHtmlPathFact(blockType: final rightBlockType),
  ) =>
    leftBlockType == rightBlockType,
  _ => false,
};

bool _sameOptionalSourceSpan(
  FlarkV3SourceSpan? left,
  FlarkV3SourceSpan right,
) => left != null && _sameSourceSpan(left, right);

bool _editingRangesFit({
  required TextSelection selection,
  required TextRange composing,
  required int length,
}) =>
    selection.isValid &&
    selection.start <= length &&
    selection.end <= length &&
    (!composing.isValid || composing.end <= length);

bool _sameSourceSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _blockStyleMatchesTarget(
  FlarkV3DocumentStructureKind target,
  FlarkV3FlutterBlockStyleLease? lease,
) {
  final expected = switch (target) {
    FlarkV3DocumentStructureKind.fencedCode =>
      FlarkV3FlutterBlockStyleKind.fencedCode,
    FlarkV3DocumentStructureKind.indentedCode =>
      FlarkV3FlutterBlockStyleKind.indentedCode,
    FlarkV3DocumentStructureKind.heading =>
      FlarkV3FlutterBlockStyleKind.heading,
    FlarkV3DocumentStructureKind.blockQuote =>
      FlarkV3FlutterBlockStyleKind.blockQuote,
    FlarkV3DocumentStructureKind.bulletList ||
    FlarkV3DocumentStructureKind.orderedList =>
      FlarkV3FlutterBlockStyleKind.tightListItem,
    _ => null,
  };
  return expected == null ? lease == null : lease?.kind == expected;
}
