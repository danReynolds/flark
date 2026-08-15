import 'dart:async';
import 'dart:isolate';
import 'dart:math' as math;

import 'models.dart';
import 'native/native_document.dart';

const _notReadySourceGapStatus = 8;

enum FlarkCoreHistoryDisposition { retained, disabled, overBudget }

enum FlarkCoreEditIntentV1 {
  insertParagraphBreak,
  deleteBackward,
  deleteForward,
  toggleTaskChecked,
  indentListItem,
  outdentListItem,
}

enum FlarkCoreEditIntentDispositionV1 {
  applied,
  handledNoChange,
  notApplicable,
  needsCurrentSemantics,
}

enum FlarkCoreEditPresentationTransitionV1 {
  none,
  splitParagraph,
  continueList,
  exitList,
  mergeParagraph,
  liftList,
  continueBlockQuote,
  exitBlockQuote,
  liftBlockQuote,
  exitHeading,
  liftHeading,
  outdentList,
  continueIndentedCode,
  joinIndentedCode,
  liftIndentedCode,
  deleteThematicBreak,
  outdentBlockQuote,
  toggleTaskChecked,
  indentList,
  retainParagraphGap,
}

/// An opaque, one-shot handle to inverse source retained by the native core.
final class FlarkCoreHistoryToken {
  FlarkCoreHistoryToken._(this._value, this._owner);

  final int _value;
  final Object _owner;
  bool _consumed = false;
}

/// An opaque handle to a native source-stable anchor.
///
/// The runtime keeps every anchor at the current revision, so a resolved
/// offset is always valid for the document's current source.
final class FlarkCoreAnchor {
  FlarkCoreAnchor._(this._value, this._owner);

  final int _value;
  final Object _owner;
  bool _released = false;
}

final class FlarkCoreSessionInspection {
  const FlarkCoreSessionInspection({
    required this.sessionState,
    required this.revision,
    required this.liveTransactions,
    required this.liveContinuations,
    required this.liveAnchors,
    required this.liveHistoryTokens,
  });

  final int sessionState;
  final int revision;
  final int liveTransactions;
  final int liveContinuations;
  final int liveAnchors;
  final int liveHistoryTokens;
}

final class FlarkCoreNativeException implements Exception {
  const FlarkCoreNativeException(
    this.operation,
    this.status, [
    this.detail = 0,
  ]);

  final String operation;
  final int status;
  final int detail;

  @override
  String toString() =>
      'FlarkCoreNativeException($operation, status: $status, detail: $detail)';
}

/// Typed fail-stop raised when the document worker, its native session, or a
/// protected mutation's terminal receipt is lost. A mutation awaiting a reply
/// must treat its commit state as unknown.
final class FlarkCoreWorkerException implements Exception {
  const FlarkCoreWorkerException(this.reason);

  final String reason;

  @override
  String toString() => 'FlarkCoreWorkerException($reason)';
}

final class FlarkCoreEditReceipt {
  const FlarkCoreEditReceipt({
    required this.revision,
    required this.sourceByteLength,
    required this.sourceUtf16Length,
    required this.historyToken,
    required this.historyDisposition,
  });

  final int revision;
  final int sourceByteLength;
  final int sourceUtf16Length;
  final FlarkCoreHistoryToken? historyToken;
  final FlarkCoreHistoryDisposition historyDisposition;
}

/// Complete authoritative result of one caller-known literal transaction.
/// The replacement is omitted because Core already owns that bounded input;
/// every coordinate and the required inverse token come from the native
/// linearization receipt.
final class FlarkCoreSourceTransactionReceiptV1 {
  const FlarkCoreSourceTransactionReceiptV1({
    required this.baseRevision,
    required this.resultRevision,
    required this.baseByteStart,
    required this.baseByteEnd,
    required this.baseUtf16Start,
    required this.baseUtf16End,
    required this.resultByteStart,
    required this.resultByteEnd,
    required this.resultUtf16Start,
    required this.resultUtf16End,
    required this.resultSelectionBaseUtf16,
    required this.resultSelectionExtentUtf16,
    required this.resultSourceByteLength,
    required this.resultSourceUtf16Length,
    required this.historyToken,
    required this.historyCompositeExtended,
    required this.parserPending,
    required this.logicalEditId,
    required this.requestDigest,
    required this.telemetry,
  });

  final int baseRevision;
  final int resultRevision;
  final int baseByteStart;
  final int baseByteEnd;
  final int baseUtf16Start;
  final int baseUtf16End;
  final int resultByteStart;
  final int resultByteEnd;
  final int resultUtf16Start;
  final int resultUtf16End;
  final int resultSelectionBaseUtf16;
  final int resultSelectionExtentUtf16;
  final int resultSourceByteLength;
  final int resultSourceUtf16Length;
  final FlarkCoreHistoryToken historyToken;
  final bool historyCompositeExtended;
  final bool parserPending;
  final int logicalEditId;
  final int requestDigest;
  final FlarkCoreEditIntentTelemetryV1 telemetry;

  FlarkCoreSourceTransactionReceiptV1 withCoreTelemetry({
    required int coreQueueMicros,
    required int coreAdoptionMicros,
  }) => FlarkCoreSourceTransactionReceiptV1(
    baseRevision: baseRevision,
    resultRevision: resultRevision,
    baseByteStart: baseByteStart,
    baseByteEnd: baseByteEnd,
    baseUtf16Start: baseUtf16Start,
    baseUtf16End: baseUtf16End,
    resultByteStart: resultByteStart,
    resultByteEnd: resultByteEnd,
    resultUtf16Start: resultUtf16Start,
    resultUtf16End: resultUtf16End,
    resultSelectionBaseUtf16: resultSelectionBaseUtf16,
    resultSelectionExtentUtf16: resultSelectionExtentUtf16,
    resultSourceByteLength: resultSourceByteLength,
    resultSourceUtf16Length: resultSourceUtf16Length,
    historyToken: historyToken,
    historyCompositeExtended: historyCompositeExtended,
    parserPending: parserPending,
    logicalEditId: logicalEditId,
    requestDigest: requestDigest,
    telemetry: telemetry.withCoreStages(
      coreQueueMicros: coreQueueMicros,
      coreAdoptionMicros: coreAdoptionMicros,
    ),
  );
}

/// Complete authoritative result of one semantic edit command. The committed
/// splice is descriptive source truth; Flutter applies it to bounded caches
/// and never interprets it as a recipe.
final class FlarkCoreEditIntentReceiptV1 {
  const FlarkCoreEditIntentReceiptV1({
    required this.disposition,
    required this.baseRevision,
    required this.resultRevision,
    required this.baseByteStart,
    required this.baseByteEnd,
    required this.baseUtf16Start,
    required this.baseUtf16End,
    required this.resultByteStart,
    required this.resultByteEnd,
    required this.resultUtf16Start,
    required this.resultUtf16End,
    required this.replacement,
    required this.resultSelectionUtf16,
    required this.resultSourceByteLength,
    required this.resultSourceUtf16Length,
    required this.historyToken,
    required this.parserPending,
    required this.logicalEditId,
    required this.requestDigest,
    required this.telemetry,
    required this.presentationTransition,
  });

  final FlarkCoreEditIntentDispositionV1 disposition;
  final int baseRevision;
  final int resultRevision;
  final int baseByteStart;
  final int baseByteEnd;
  final int baseUtf16Start;
  final int baseUtf16End;
  final int resultByteStart;
  final int resultByteEnd;
  final int resultUtf16Start;
  final int resultUtf16End;
  final String replacement;
  final int resultSelectionUtf16;
  final int resultSourceByteLength;
  final int resultSourceUtf16Length;
  final FlarkCoreHistoryToken? historyToken;
  final bool parserPending;
  final int logicalEditId;
  final int requestDigest;
  final FlarkCoreEditIntentTelemetryV1 telemetry;
  final FlarkCoreEditPresentationTransitionV1 presentationTransition;

  bool get hasCommit => disposition == FlarkCoreEditIntentDispositionV1.applied;

  FlarkCoreEditIntentReceiptV1 withCoreTelemetry({
    required int coreQueueMicros,
    required int coreAdoptionMicros,
  }) => FlarkCoreEditIntentReceiptV1(
    disposition: disposition,
    baseRevision: baseRevision,
    resultRevision: resultRevision,
    baseByteStart: baseByteStart,
    baseByteEnd: baseByteEnd,
    baseUtf16Start: baseUtf16Start,
    baseUtf16End: baseUtf16End,
    resultByteStart: resultByteStart,
    resultByteEnd: resultByteEnd,
    resultUtf16Start: resultUtf16Start,
    resultUtf16End: resultUtf16End,
    replacement: replacement,
    resultSelectionUtf16: resultSelectionUtf16,
    resultSourceByteLength: resultSourceByteLength,
    resultSourceUtf16Length: resultSourceUtf16Length,
    historyToken: historyToken,
    parserPending: parserPending,
    logicalEditId: logicalEditId,
    requestDigest: requestDigest,
    telemetry: telemetry.withCoreStages(
      coreQueueMicros: coreQueueMicros,
      coreAdoptionMicros: coreAdoptionMicros,
    ),
    presentationTransition: presentationTransition,
  );
}

/// Causal durations for one semantic command. These are diagnostic timings,
/// not wall-clock claim evidence; the Flutter harness joins them to a frame.
final class FlarkCoreEditIntentTelemetryV1 {
  const FlarkCoreEditIntentTelemetryV1({
    required this.coreQueueMicros,
    required this.workerRoundTripMicros,
    required this.workerQueueMicros,
    required this.nativeFfiMicros,
    required this.coreAdoptionMicros,
  });

  final int coreQueueMicros;
  final int workerRoundTripMicros;
  final int workerQueueMicros;
  final int nativeFfiMicros;
  final int coreAdoptionMicros;

  FlarkCoreEditIntentTelemetryV1 withCoreStages({
    required int coreQueueMicros,
    required int coreAdoptionMicros,
  }) => FlarkCoreEditIntentTelemetryV1(
    coreQueueMicros: coreQueueMicros,
    workerRoundTripMicros: workerRoundTripMicros,
    workerQueueMicros: workerQueueMicros,
    nativeFfiMicros: nativeFfiMicros,
    coreAdoptionMicros: coreAdoptionMicros,
  );
}

/// Headless Dart document actor backed by the Rust Flark runtime.
///
/// A single persistent isolate owns the native session. Calls are serialized by
/// its mailbox, so parsing and source reads never execute on Flutter's UI
/// isolate and revision order cannot race.
final class FlarkCoreDocument {
  FlarkCoreDocument._(
    this._isolate,
    this._commands, {
    required ReceivePort workerErrors,
    required ReceivePort workerExits,
    required StreamSubscription<Object?> workerErrorSubscription,
    required StreamSubscription<Object?> workerExitSubscription,
    required Completer<FlarkCoreWorkerException> workerFailure,
    required Duration editIntentReplyTimeout,
    required int revision,
    required int sourceByteLength,
    required int sourceUtf16Length,
    required bool ready,
  }) : _workerErrors = workerErrors,
       _workerExits = workerExits,
       _workerErrorSubscription = workerErrorSubscription,
       _workerExitSubscription = workerExitSubscription,
       _workerFailure = workerFailure,
       _editIntentReplyTimeout = editIntentReplyTimeout,
       _revision = revision,
       _sourceByteLength = sourceByteLength,
       _sourceUtf16Length = sourceUtf16Length,
       _ready = ready;

  final Isolate _isolate;
  final SendPort _commands;
  final ReceivePort _workerErrors;
  final ReceivePort _workerExits;
  final StreamSubscription<Object?> _workerErrorSubscription;
  final StreamSubscription<Object?> _workerExitSubscription;
  final Completer<FlarkCoreWorkerException> _workerFailure;
  final Duration _editIntentReplyTimeout;
  final Object _historyOwner = Object();

  int _revision;
  int _sourceByteLength;
  int _sourceUtf16Length;
  bool _ready;
  bool _disposed = false;

  int get revision => _revision;
  int get sourceByteLength => _sourceByteLength;
  int get sourceUtf16Length => _sourceUtf16Length;
  bool get isReady => _ready;

  static Future<FlarkCoreDocument> open(
    String source, {
    String? libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
    Duration editIntentReplyTimeout = const Duration(milliseconds: 250),
    bool debugDropFirstEditIntentReply = false,
  }) async {
    if (historyBudgetBytes < 0) {
      throw RangeError.value(historyBudgetBytes, 'historyBudgetBytes');
    }
    if (editIntentReplyTimeout.inMicroseconds <= 0) {
      throw ArgumentError.value(
        editIntentReplyTimeout,
        'editIntentReplyTimeout',
      );
    }
    final startup = ReceivePort();
    final errors = ReceivePort();
    final exits = ReceivePort();
    final workerFailure = Completer<FlarkCoreWorkerException>();
    final errorSubscription = errors.listen((Object? error) {
      if (!workerFailure.isCompleted) {
        workerFailure.complete(
          FlarkCoreWorkerException('worker error: ${_workerErrorText(error)}'),
        );
      }
    });
    final exitSubscription = exits.listen((Object? _) {
      if (!workerFailure.isCompleted) {
        workerFailure.complete(const FlarkCoreWorkerException('worker exited'));
      }
    });
    final isolate = await Isolate.spawn<List<Object?>>(
      _documentWorker,
      [
        startup.sendPort,
        source,
        libraryPath,
        historyBudgetBytes,
        debugDropFirstEditIntentReply,
      ],
      onError: errors.sendPort,
      onExit: exits.sendPort,
      errorsAreFatal: true,
      debugName: 'flark-core-document',
    );
    var monitoringTransferred = false;
    try {
      final message = await Future.any<Object?>([
        startup.first,
        workerFailure.future.then(
          (failure) => <Object?, Object?>{'workerFailure': failure},
        ),
      ]);
      final envelope = message! as Map<Object?, Object?>;
      if (envelope case {'nativeError': final Map<Object?, Object?> error}) {
        isolate.kill(priority: Isolate.immediate);
        throw _decodeNativeException(error);
      }
      if (envelope case {
        'workerFailure': final FlarkCoreWorkerException error,
      }) {
        isolate.kill(priority: Isolate.immediate);
        throw error;
      }
      if (envelope case {'error': final Object error}) {
        isolate.kill(priority: Isolate.immediate);
        throw StateError(error.toString());
      }
      final document = FlarkCoreDocument._(
        isolate,
        envelope['commands']! as SendPort,
        workerErrors: errors,
        workerExits: exits,
        workerErrorSubscription: errorSubscription,
        workerExitSubscription: exitSubscription,
        workerFailure: workerFailure,
        editIntentReplyTimeout: editIntentReplyTimeout,
        revision: envelope['revision']! as int,
        sourceByteLength: envelope['sourceByteLength']! as int,
        sourceUtf16Length: envelope['sourceUtf16Length']! as int,
        ready: envelope['ready']! as bool,
      );
      monitoringTransferred = true;
      return document;
    } finally {
      startup.close();
      if (!monitoringTransferred) {
        isolate.kill(priority: Isolate.immediate);
        await errorSubscription.cancel();
        await exitSubscription.cancel();
        errors.close();
        exits.close();
      }
    }
  }

  Future<FlarkCoreEditReceipt> applyEditUtf16(
    int startUtf16,
    int endUtf16,
    String replacement,
  ) async {
    final result = await _request('edit', {
      'start': startUtf16,
      'end': endUtf16,
      'replacement': replacement,
    });
    _revision = result['revision']! as int;
    _sourceByteLength = result['sourceByteLength']! as int;
    _sourceUtf16Length = result['sourceUtf16Length']! as int;
    _ready = false;
    return _editReceipt(result);
  }

  Future<FlarkCoreSourceTransactionReceiptV1> applySourceTransactionV1({
    required int expectedRevision,
    required FlarkCoreAnchor selectionBaseAnchor,
    required FlarkCoreAnchor selectionExtentAnchor,
    required int logicalEditId,
    required int requestDigest,
    required int acknowledgePreviousLogicalEditId,
    required int selectionGeneration,
    required int historyGroupId,
    required int startUtf16,
    required int endUtf16,
    required String replacement,
    required int resultSelectionBaseUtf16,
    required int resultSelectionExtentUtf16,
    required bool selectionAffinityDownstream,
    required bool selectionDirectional,
  }) async {
    _requireOwnedAnchor(selectionBaseAnchor);
    _requireOwnedAnchor(selectionExtentAnchor);
    final arguments = <String, Object?>{
      'expectedRevision': expectedRevision,
      'selectionBaseAnchor': selectionBaseAnchor._value,
      'selectionExtentAnchor': selectionExtentAnchor._value,
      'logicalEditId': logicalEditId,
      'requestDigest': requestDigest,
      'acknowledgePreviousLogicalEditId': acknowledgePreviousLogicalEditId,
      'selectionGeneration': selectionGeneration,
      'historyGroupId': historyGroupId,
      'startUtf16': startUtf16,
      'endUtf16': endUtf16,
      'replacement': replacement,
      'resultSelectionBaseUtf16': resultSelectionBaseUtf16,
      'resultSelectionExtentUtf16': resultSelectionExtentUtf16,
      'selectionAffinityDownstream': selectionAffinityDownstream,
      'selectionDirectional': selectionDirectional,
      'dispatchEpochMicros': DateTime.now().microsecondsSinceEpoch,
    };
    final roundTrip = Stopwatch()..start();
    final result = await _requestMutationTerminal(
      'sourceTransactionV1',
      arguments,
    );
    roundTrip.stop();
    _revision = result['resultRevision']! as int;
    _sourceByteLength = result['resultSourceByteLength']! as int;
    _sourceUtf16Length = result['resultSourceUtf16Length']! as int;
    _ready = !(result['parserPending']! as bool);
    return FlarkCoreSourceTransactionReceiptV1(
      baseRevision: result['baseRevision']! as int,
      resultRevision: result['resultRevision']! as int,
      baseByteStart: result['baseByteStart']! as int,
      baseByteEnd: result['baseByteEnd']! as int,
      baseUtf16Start: result['baseUtf16Start']! as int,
      baseUtf16End: result['baseUtf16End']! as int,
      resultByteStart: result['resultByteStart']! as int,
      resultByteEnd: result['resultByteEnd']! as int,
      resultUtf16Start: result['resultUtf16Start']! as int,
      resultUtf16End: result['resultUtf16End']! as int,
      resultSelectionBaseUtf16: result['resultSelectionBaseUtf16']! as int,
      resultSelectionExtentUtf16: result['resultSelectionExtentUtf16']! as int,
      resultSourceByteLength: _sourceByteLength,
      resultSourceUtf16Length: _sourceUtf16Length,
      historyToken: FlarkCoreHistoryToken._(
        result['historyToken']! as int,
        _historyOwner,
      ),
      historyCompositeExtended: result['historyCompositeExtended']! as bool,
      parserPending: result['parserPending']! as bool,
      logicalEditId: result['logicalEditId']! as int,
      requestDigest: result['requestDigest']! as int,
      telemetry: FlarkCoreEditIntentTelemetryV1(
        coreQueueMicros: 0,
        workerRoundTripMicros: roundTrip.elapsedMicroseconds,
        workerQueueMicros: result['workerQueueMicros']! as int,
        nativeFfiMicros: result['nativeFfiMicros']! as int,
        coreAdoptionMicros: 0,
      ),
    );
  }

  Future<FlarkCoreSourceTransactionReceiptV1> applyStagedSourceTransactionV1({
    required int expectedRevision,
    required FlarkCoreAnchor selectionBaseAnchor,
    required FlarkCoreAnchor selectionExtentAnchor,
    required int logicalEditId,
    required int requestDigest,
    required int acknowledgePreviousLogicalEditId,
    required int selectionGeneration,
    required int startUtf16,
    required int endUtf16,
    required String replacement,
    required int resultSelectionUtf16,
  }) async {
    _requireOwnedAnchor(selectionBaseAnchor);
    _requireOwnedAnchor(selectionExtentAnchor);
    final arguments = <String, Object?>{
      'expectedRevision': expectedRevision,
      'selectionBaseAnchor': selectionBaseAnchor._value,
      'selectionExtentAnchor': selectionExtentAnchor._value,
      'logicalEditId': logicalEditId,
      'requestDigest': requestDigest,
      'acknowledgePreviousLogicalEditId': acknowledgePreviousLogicalEditId,
      'selectionGeneration': selectionGeneration,
      'startUtf16': startUtf16,
      'endUtf16': endUtf16,
      'replacement': replacement,
      'resultSelectionUtf16': resultSelectionUtf16,
      'dispatchEpochMicros': DateTime.now().microsecondsSinceEpoch,
    };
    final roundTrip = Stopwatch()..start();
    final result = await _requestMutationTerminal(
      'stagedSourceTransactionV1',
      arguments,
    );
    roundTrip.stop();
    _revision = result['resultRevision']! as int;
    _sourceByteLength = result['resultSourceByteLength']! as int;
    _sourceUtf16Length = result['resultSourceUtf16Length']! as int;
    _ready = !(result['parserPending']! as bool);
    return FlarkCoreSourceTransactionReceiptV1(
      baseRevision: result['baseRevision']! as int,
      resultRevision: result['resultRevision']! as int,
      baseByteStart: result['baseByteStart']! as int,
      baseByteEnd: result['baseByteEnd']! as int,
      baseUtf16Start: result['baseUtf16Start']! as int,
      baseUtf16End: result['baseUtf16End']! as int,
      resultByteStart: result['resultByteStart']! as int,
      resultByteEnd: result['resultByteEnd']! as int,
      resultUtf16Start: result['resultUtf16Start']! as int,
      resultUtf16End: result['resultUtf16End']! as int,
      resultSelectionBaseUtf16: result['resultSelectionBaseUtf16']! as int,
      resultSelectionExtentUtf16: result['resultSelectionExtentUtf16']! as int,
      resultSourceByteLength: _sourceByteLength,
      resultSourceUtf16Length: _sourceUtf16Length,
      historyToken: FlarkCoreHistoryToken._(
        result['historyToken']! as int,
        _historyOwner,
      ),
      historyCompositeExtended: false,
      parserPending: result['parserPending']! as bool,
      logicalEditId: result['logicalEditId']! as int,
      requestDigest: result['requestDigest']! as int,
      telemetry: FlarkCoreEditIntentTelemetryV1(
        coreQueueMicros: 0,
        workerRoundTripMicros: roundTrip.elapsedMicroseconds,
        workerQueueMicros: result['workerQueueMicros']! as int,
        nativeFfiMicros: result['nativeFfiMicros']! as int,
        coreAdoptionMicros: 0,
      ),
    );
  }

  Future<FlarkCoreEditIntentReceiptV1> applyEditIntentV1({
    required FlarkCoreEditIntentV1 intent,
    required int expectedRevision,
    required FlarkCoreAnchor selectionBaseAnchor,
    required FlarkCoreAnchor selectionExtentAnchor,
    FlarkCoreAnchor? targetAnchor,
    required int logicalEditId,
    required int requestDigest,
    required int acknowledgePreviousLogicalEditId,
    required int selectionGeneration,
    required bool compositionActive,
  }) async {
    _requireOwnedAnchor(selectionBaseAnchor);
    _requireOwnedAnchor(selectionExtentAnchor);
    if (targetAnchor != null) _requireOwnedAnchor(targetAnchor);
    if ((intent == FlarkCoreEditIntentV1.toggleTaskChecked) !=
        (targetAnchor != null)) {
      throw ArgumentError(
        'Only selection-independent semantic actions accept a target anchor',
      );
    }
    final arguments = <String, Object?>{
      'intent': intent.index,
      'expectedRevision': expectedRevision,
      'selectionBaseAnchor': selectionBaseAnchor._value,
      'selectionExtentAnchor': selectionExtentAnchor._value,
      'targetAnchor': targetAnchor?._value ?? 0,
      'logicalEditId': logicalEditId,
      'requestDigest': requestDigest,
      'acknowledgePreviousLogicalEditId': acknowledgePreviousLogicalEditId,
      'selectionGeneration': selectionGeneration,
      'compositionActive': compositionActive,
      'dispatchEpochMicros': DateTime.now().microsecondsSinceEpoch,
    };
    final roundTrip = Stopwatch()..start();
    final result = await _requestSemanticTerminal(arguments);
    roundTrip.stop();
    final disposition =
        FlarkCoreEditIntentDispositionV1.values[result['disposition']! as int];
    final hasCommit = disposition == FlarkCoreEditIntentDispositionV1.applied;
    if (hasCommit) {
      _revision = result['resultRevision']! as int;
      _sourceByteLength = result['resultSourceByteLength']! as int;
      _sourceUtf16Length = result['resultSourceUtf16Length']! as int;
    }
    _ready = !(result['parserPending']! as bool);
    final token = result['historyToken'] as int?;
    return FlarkCoreEditIntentReceiptV1(
      disposition: disposition,
      baseRevision: result['baseRevision']! as int,
      resultRevision: result['resultRevision']! as int,
      baseByteStart: result['baseByteStart']! as int,
      baseByteEnd: result['baseByteEnd']! as int,
      baseUtf16Start: result['baseUtf16Start']! as int,
      baseUtf16End: result['baseUtf16End']! as int,
      resultByteStart: result['resultByteStart']! as int,
      resultByteEnd: result['resultByteEnd']! as int,
      resultUtf16Start: result['resultUtf16Start']! as int,
      resultUtf16End: result['resultUtf16End']! as int,
      replacement: result['replacement']! as String,
      resultSelectionUtf16: result['resultSelectionUtf16']! as int,
      resultSourceByteLength: result['resultSourceByteLength']! as int,
      resultSourceUtf16Length: result['resultSourceUtf16Length']! as int,
      historyToken: token == null
          ? null
          : FlarkCoreHistoryToken._(token, _historyOwner),
      parserPending: result['parserPending']! as bool,
      logicalEditId: result['logicalEditId']! as int,
      requestDigest: result['requestDigest']! as int,
      telemetry: FlarkCoreEditIntentTelemetryV1(
        coreQueueMicros: 0,
        workerRoundTripMicros: roundTrip.elapsedMicroseconds,
        workerQueueMicros: result['workerQueueMicros']! as int,
        nativeFfiMicros: result['nativeFfiMicros']! as int,
        coreAdoptionMicros: 0,
      ),
      presentationTransition: FlarkCoreEditPresentationTransitionV1
          .values[result['presentationTransition']! as int],
    );
  }

  /// Replays and consumes [token]. The returned receipt contains the inverse
  /// token for the opposite history direction when native retention succeeds.
  Future<FlarkCoreEditReceipt> replayHistory(
    FlarkCoreHistoryToken token,
  ) async {
    _requireOwnedHistoryToken(token);
    final result = await _request('replayHistory', {
      'historyToken': token._value,
    });
    token._consumed = true;
    _revision = result['revision']! as int;
    _sourceByteLength = result['sourceByteLength']! as int;
    _sourceUtf16Length = result['sourceUtf16Length']! as int;
    _ready = false;
    return _editReceipt(result);
  }

  /// Releases [token] without changing source.
  Future<void> releaseHistory(FlarkCoreHistoryToken token) async {
    _requireOwnedHistoryToken(token);
    await _request('releaseHistory', {'historyToken': token._value});
    token._consumed = true;
  }

  Future<bool> pump({int workUnits = 512}) async {
    final result = await _request('pump', {'workUnits': workUnits});
    _ready = result['ready']! as bool;
    return _ready;
  }

  Future<void> pumpUntilReady({int workUnits = 512}) async {
    final result = await _request('pumpUntilReady', {'workUnits': workUnits});
    _ready = result['ready']! as bool;
  }

  Future<FlarkViewport> queryViewport({
    int startByte = 0,
    int? endByte,
    int maxRows = 256,
  }) async {
    final result = await _request('queryViewport', {
      'startByte': startByte,
      'endByte': endByte,
      'maxRows': maxRows,
    });
    return FlarkViewport.fromMessage(
      result['viewport']! as Map<Object?, Object?>,
    );
  }

  Future<FlarkSemanticTarget?> querySemanticTarget(FlarkInlineFact fact) async {
    late final Map<Object?, Object?> result;
    try {
      result = await _request('querySemanticTarget', {
        'fact': fact.toMessage(),
      });
    } on FlarkCoreNativeException catch (error) {
      // A target lookup is an optional presentation query. Source edits may
      // temporarily retire parser facts; callers should simply offer no target
      // until the current revision is certified, while real native failures
      // remain visible.
      if (error.status == _notReadySourceGapStatus) return null;
      rethrow;
    }
    return switch (result['target']) {
      final Map<Object?, Object?> target => FlarkSemanticTarget.fromMessage(
        target,
      ),
      _ => null,
    };
  }

  Future<FlarkViewport> queryViewportNext(
    FlarkViewport previous, {
    int maxRows = 256,
  }) async {
    final result = await _request('queryViewportNext', {
      'viewport': previous.toMessage(),
      'maxRows': maxRows,
    });
    return FlarkViewport.fromMessage(
      result['viewport']! as Map<Object?, Object?>,
    );
  }

  Future<void> releaseViewportContinuation(FlarkViewport viewport) async {
    if (viewport.continuation == 0) return;
    await _request('releaseViewportContinuation', {
      'viewport': viewport.toMessage(),
    });
  }

  /// Creates a source-stable anchor at a UTF-16 scalar boundary. The native
  /// runtime transforms it through every later edit; [downstream] selects the
  /// splice edge it follows when an edit lands exactly on or across it.
  Future<FlarkCoreAnchor> createAnchorUtf16(
    int utf16Position, {
    required bool downstream,
  }) async {
    final result = await _request('createAnchor', {
      'utf16': utf16Position,
      'downstream': downstream,
    });
    return FlarkCoreAnchor._(result['anchor']! as int, _historyOwner);
  }

  /// Resolves [anchor] to a UTF-16 offset at the current revision.
  Future<int> resolveAnchorUtf16(FlarkCoreAnchor anchor) async {
    _requireOwnedAnchor(anchor);
    final result = await _request('resolveAnchor', {'anchor': anchor._value});
    return result['utf16']! as int;
  }

  Future<void> releaseAnchor(FlarkCoreAnchor anchor) async {
    _requireOwnedAnchor(anchor);
    await _request('releaseAnchor', {'anchor': anchor._value});
    anchor._released = true;
  }

  Future<FlarkCoreSessionInspection> inspectSession() async {
    final result = await _request('inspect', const {});
    return FlarkCoreSessionInspection(
      sessionState: result['sessionState']! as int,
      revision: result['revision']! as int,
      liveTransactions: result['liveTransactions']! as int,
      liveContinuations: result['liveContinuations']! as int,
      liveAnchors: result['liveAnchors']! as int,
      liveHistoryTokens: result['liveHistoryTokens']! as int,
    );
  }

  Future<String> readSource() async {
    final result = await _request('readSource', const {});
    return result['source']! as String;
  }

  Future<String> readSourceRange(int startByte, int endByte) async {
    final result = await _request('readSourceRange', {
      'startByte': startByte,
      'endByte': endByte,
    });
    return result['source']! as String;
  }

  Future<String> readSourceUtf16Range(int startUtf16, int endUtf16) async {
    final result = await _request('readSourceUtf16Range', {
      'startUtf16': startUtf16,
      'endUtf16': endUtf16,
    });
    return result['source']! as String;
  }

  /// Forces the document worker to exit so containment and fail-stop behavior
  /// can be verified without depending on an actual native crash.
  Future<void> debugCrashWorkerForTesting() async {
    await _request('debugCrashWorkerForTesting', const {});
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    try {
      if (!_workerFailure.isCompleted) {
        await _send('dispose', const {});
      }
    } on FlarkCoreWorkerException {
      // The worker is already gone; contained local teardown still completes.
    } finally {
      _isolate.kill(priority: Isolate.immediate);
      await _workerErrorSubscription.cancel();
      await _workerExitSubscription.cancel();
      _workerErrors.close();
      _workerExits.close();
    }
  }

  Future<Map<Object?, Object?>> _request(
    String operation,
    Map<String, Object?> arguments,
  ) {
    if (_disposed) throw StateError('FlarkCoreDocument is disposed');
    return _send(operation, arguments);
  }

  Future<Map<Object?, Object?>> _requestSemanticTerminal(
    Map<String, Object?> arguments,
  ) => _requestMutationTerminal('editIntentV1', arguments);

  Future<Map<Object?, Object?>> _requestMutationTerminal(
    String operation,
    Map<String, Object?> arguments,
  ) async {
    try {
      return await _send(
        operation,
        arguments,
        replyTimeout: _editIntentReplyTimeout,
      );
    } on TimeoutException {
      // The native terminal slot makes this exact logical ID/digest replay
      // idempotent whether the first request committed or was merely delayed.
      try {
        return await _send(
          operation,
          arguments,
          replyTimeout: _editIntentReplyTimeout,
        );
      } on FlarkCoreWorkerException {
        rethrow;
      } on Object catch (error) {
        throw FlarkCoreWorkerException(
          '$operation terminal recovery failed: $error',
        );
      }
    }
  }

  Future<Map<Object?, Object?>> _send(
    String operation,
    Map<String, Object?> arguments, {
    Duration? replyTimeout,
  }) async {
    if (_workerFailure.isCompleted) throw await _workerFailure.future;
    final reply = ReceivePort();
    try {
      _commands.send([operation, arguments, reply.sendPort]);
      final response = Future.any<Map<Object?, Object?>>([
        reply.first.then((value) => value! as Map<Object?, Object?>),
        _workerFailure.future.then<Map<Object?, Object?>>(
          (failure) => throw failure,
        ),
      ]);
      final envelope = replyTimeout == null
          ? await response
          : await response.timeout(replyTimeout);
      if (envelope case {'nativeError': final Map<Object?, Object?> error}) {
        throw _decodeNativeException(error);
      }
      if (envelope case {'error': final Object error}) {
        throw StateError('Flark $operation failed: $error');
      }
      return envelope;
    } finally {
      reply.close();
    }
  }

  FlarkCoreEditReceipt _editReceipt(Map<Object?, Object?> result) {
    final token = result['historyToken'] as int?;
    return FlarkCoreEditReceipt(
      revision: _revision,
      sourceByteLength: _sourceByteLength,
      sourceUtf16Length: _sourceUtf16Length,
      historyToken: token == null
          ? null
          : FlarkCoreHistoryToken._(token, _historyOwner),
      historyDisposition: FlarkCoreHistoryDisposition
          .values[result['historyDisposition']! as int],
    );
  }

  void _requireOwnedHistoryToken(FlarkCoreHistoryToken token) {
    if (!identical(token._owner, _historyOwner)) {
      throw ArgumentError.value(token, 'token', 'belongs to another document');
    }
    if (token._consumed) {
      throw StateError('Flark history token was already consumed');
    }
  }

  void _requireOwnedAnchor(FlarkCoreAnchor anchor) {
    if (!identical(anchor._owner, _historyOwner)) {
      throw ArgumentError.value(
        anchor,
        'anchor',
        'belongs to another document',
      );
    }
    if (anchor._released) {
      throw StateError('Flark anchor was already released');
    }
  }
}

FlarkCoreNativeException _decodeNativeException(Map<Object?, Object?> error) =>
    FlarkCoreNativeException(
      error['operation']! as String,
      error['status']! as int,
      error['detail']! as int,
    );

String _workerErrorText(Object? error) {
  if (error is List && error.isNotEmpty) return error.first.toString();
  return error.toString();
}

Future<void> _documentWorker(List<Object?> startup) async {
  final startupPort = startup[0]! as SendPort;
  try {
    final document = FlarkNativeDocument.open(
      startup[1]! as String,
      libraryPath: startup[2] as String?,
      historyBudgetBytes: startup[3]! as int,
    );
    final commands = ReceivePort();
    var dropNextMutationReply = startup[4]! as bool;
    startupPort.send({
      'commands': commands.sendPort,
      'revision': document.revision,
      'sourceByteLength': document.sourceByteLength,
      'sourceUtf16Length': document.sourceUtf16Length,
      'ready': document.isReady,
    });
    await for (final raw in commands) {
      final message = raw! as List<Object?>;
      final operation = message[0]! as String;
      final arguments = message[1]! as Map<Object?, Object?>;
      final reply = message[2]! as SendPort;
      if (operation == 'debugCrashWorkerForTesting') {
        throw StateError('debug worker crash');
      }
      try {
        switch (operation) {
          case 'edit':
            final receipt = document.applyEditUtf16(
              arguments['start']! as int,
              arguments['end']! as int,
              arguments['replacement']! as String,
            );
            reply.send({
              'revision': receipt.revision,
              'sourceByteLength': receipt.sourceByteLength,
              'sourceUtf16Length': receipt.sourceUtf16Length,
              'historyToken': receipt.historyToken,
              'historyDisposition': receipt.historyDisposition.index,
            });
          case 'sourceTransactionV1':
            final workerReceivedEpochMicros =
                DateTime.now().microsecondsSinceEpoch;
            final nativeWatch = Stopwatch()..start();
            final receipt = document.applySourceTransactionV1(
              expectedRevision: arguments['expectedRevision']! as int,
              selectionBaseAnchor: arguments['selectionBaseAnchor']! as int,
              selectionExtentAnchor: arguments['selectionExtentAnchor']! as int,
              logicalEditId: arguments['logicalEditId']! as int,
              requestDigest: arguments['requestDigest']! as int,
              acknowledgePreviousLogicalEditId:
                  arguments['acknowledgePreviousLogicalEditId']! as int,
              selectionGeneration: arguments['selectionGeneration']! as int,
              historyGroupId: arguments['historyGroupId']! as int,
              startUtf16: arguments['startUtf16']! as int,
              endUtf16: arguments['endUtf16']! as int,
              replacement: arguments['replacement']! as String,
              resultSelectionBaseUtf16:
                  arguments['resultSelectionBaseUtf16']! as int,
              resultSelectionExtentUtf16:
                  arguments['resultSelectionExtentUtf16']! as int,
              selectionAffinityDownstream:
                  arguments['selectionAffinityDownstream']! as bool,
              selectionDirectional: arguments['selectionDirectional']! as bool,
            );
            nativeWatch.stop();
            final envelope = <Object?, Object?>{
              'baseRevision': receipt.baseRevision,
              'resultRevision': receipt.resultRevision,
              'baseByteStart': receipt.baseByteStart,
              'baseByteEnd': receipt.baseByteEnd,
              'baseUtf16Start': receipt.baseUtf16Start,
              'baseUtf16End': receipt.baseUtf16End,
              'resultByteStart': receipt.resultByteStart,
              'resultByteEnd': receipt.resultByteEnd,
              'resultUtf16Start': receipt.resultUtf16Start,
              'resultUtf16End': receipt.resultUtf16End,
              'resultSelectionBaseUtf16': receipt.resultSelectionBaseUtf16,
              'resultSelectionExtentUtf16': receipt.resultSelectionExtentUtf16,
              'resultSourceByteLength': receipt.resultSourceByteLength,
              'resultSourceUtf16Length': receipt.resultSourceUtf16Length,
              'historyToken': receipt.historyToken,
              'historyCompositeExtended': receipt.historyCompositeExtended,
              'parserPending': receipt.parserPending,
              'logicalEditId': receipt.logicalEditId,
              'requestDigest': receipt.requestDigest,
              'workerQueueMicros': math.max(
                0,
                workerReceivedEpochMicros -
                    (arguments['dispatchEpochMicros']! as int),
              ),
              'nativeFfiMicros': nativeWatch.elapsedMicroseconds,
            };
            if (dropNextMutationReply) {
              dropNextMutationReply = false;
            } else {
              reply.send(envelope);
            }
          case 'stagedSourceTransactionV1':
            final workerReceivedEpochMicros =
                DateTime.now().microsecondsSinceEpoch;
            final nativeWatch = Stopwatch()..start();
            final receipt = document.applyStagedSourceTransactionV1(
              expectedRevision: arguments['expectedRevision']! as int,
              selectionBaseAnchor: arguments['selectionBaseAnchor']! as int,
              selectionExtentAnchor: arguments['selectionExtentAnchor']! as int,
              logicalEditId: arguments['logicalEditId']! as int,
              requestDigest: arguments['requestDigest']! as int,
              acknowledgePreviousLogicalEditId:
                  arguments['acknowledgePreviousLogicalEditId']! as int,
              selectionGeneration: arguments['selectionGeneration']! as int,
              startUtf16: arguments['startUtf16']! as int,
              endUtf16: arguments['endUtf16']! as int,
              replacement: arguments['replacement']! as String,
              resultSelectionUtf16: arguments['resultSelectionUtf16']! as int,
            );
            nativeWatch.stop();
            final envelope = <Object?, Object?>{
              'baseRevision': receipt.baseRevision,
              'resultRevision': receipt.resultRevision,
              'baseByteStart': receipt.baseByteStart,
              'baseByteEnd': receipt.baseByteEnd,
              'baseUtf16Start': receipt.baseUtf16Start,
              'baseUtf16End': receipt.baseUtf16End,
              'resultByteStart': receipt.resultByteStart,
              'resultByteEnd': receipt.resultByteEnd,
              'resultUtf16Start': receipt.resultUtf16Start,
              'resultUtf16End': receipt.resultUtf16End,
              'resultSelectionBaseUtf16': receipt.resultSelectionBaseUtf16,
              'resultSelectionExtentUtf16': receipt.resultSelectionExtentUtf16,
              'resultSourceByteLength': receipt.resultSourceByteLength,
              'resultSourceUtf16Length': receipt.resultSourceUtf16Length,
              'historyToken': receipt.historyToken,
              'parserPending': receipt.parserPending,
              'logicalEditId': receipt.logicalEditId,
              'requestDigest': receipt.requestDigest,
              'workerQueueMicros': math.max(
                0,
                workerReceivedEpochMicros -
                    (arguments['dispatchEpochMicros']! as int),
              ),
              'nativeFfiMicros': nativeWatch.elapsedMicroseconds,
            };
            if (dropNextMutationReply) {
              dropNextMutationReply = false;
            } else {
              reply.send(envelope);
            }
          case 'editIntentV1':
            final workerReceivedEpochMicros =
                DateTime.now().microsecondsSinceEpoch;
            final nativeWatch = Stopwatch()..start();
            final receipt = document.applyEditIntentV1(
              intent:
                  FlarkNativeEditIntentV1.values[arguments['intent']! as int],
              expectedRevision: arguments['expectedRevision']! as int,
              selectionBaseAnchor: arguments['selectionBaseAnchor']! as int,
              selectionExtentAnchor: arguments['selectionExtentAnchor']! as int,
              targetAnchor: arguments['targetAnchor']! as int,
              logicalEditId: arguments['logicalEditId']! as int,
              requestDigest: arguments['requestDigest']! as int,
              acknowledgePreviousLogicalEditId:
                  arguments['acknowledgePreviousLogicalEditId']! as int,
              selectionGeneration: arguments['selectionGeneration']! as int,
              compositionActive: arguments['compositionActive']! as bool,
            );
            nativeWatch.stop();
            final envelope = <Object?, Object?>{
              'disposition': receipt.disposition.index,
              'baseRevision': receipt.baseRevision,
              'resultRevision': receipt.resultRevision,
              'baseByteStart': receipt.baseByteStart,
              'baseByteEnd': receipt.baseByteEnd,
              'baseUtf16Start': receipt.baseUtf16Start,
              'baseUtf16End': receipt.baseUtf16End,
              'resultByteStart': receipt.resultByteStart,
              'resultByteEnd': receipt.resultByteEnd,
              'resultUtf16Start': receipt.resultUtf16Start,
              'resultUtf16End': receipt.resultUtf16End,
              'replacement': receipt.replacement,
              'resultSelectionUtf16': receipt.resultSelectionUtf16,
              'resultSourceByteLength': receipt.resultSourceByteLength,
              'resultSourceUtf16Length': receipt.resultSourceUtf16Length,
              'historyToken': receipt.historyToken,
              'parserPending': receipt.parserPending,
              'logicalEditId': receipt.logicalEditId,
              'requestDigest': receipt.requestDigest,
              'presentationTransition': receipt.presentationTransition.index,
              'workerQueueMicros': math.max(
                0,
                workerReceivedEpochMicros -
                    (arguments['dispatchEpochMicros']! as int),
              ),
              'nativeFfiMicros': nativeWatch.elapsedMicroseconds,
            };
            if (dropNextMutationReply) {
              dropNextMutationReply = false;
            } else {
              reply.send(envelope);
            }
          case 'replayHistory':
            final receipt = document.replayHistory(
              arguments['historyToken']! as int,
            );
            reply.send({
              'revision': receipt.revision,
              'sourceByteLength': receipt.sourceByteLength,
              'sourceUtf16Length': receipt.sourceUtf16Length,
              'historyToken': receipt.historyToken,
              'historyDisposition': receipt.historyDisposition.index,
            });
          case 'releaseHistory':
            document.releaseHistory(arguments['historyToken']! as int);
            reply.send(const <Object?, Object?>{});
          case 'pump':
            final ready = document.pump(
              workUnits: arguments['workUnits']! as int,
            );
            reply.send({'ready': ready});
          case 'pumpUntilReady':
            document.pumpUntilReady(workUnits: arguments['workUnits']! as int);
            reply.send({'ready': true});
          case 'queryViewport':
            final viewport = document.queryViewport(
              startByte: arguments['startByte']! as int,
              endByte: arguments['endByte'] as int?,
              maxRows: arguments['maxRows']! as int,
            );
            reply.send({'viewport': viewport.toMessage()});
          case 'querySemanticTarget':
            final target = document.querySemanticTarget(
              FlarkInlineFact.fromMessage(
                arguments['fact']! as Map<Object?, Object?>,
              ),
            );
            reply.send({'target': target?.toMessage()});
          case 'queryViewportNext':
            final viewport = document.queryViewportNext(
              FlarkViewport.fromMessage(
                arguments['viewport']! as Map<Object?, Object?>,
              ),
              maxRows: arguments['maxRows']! as int,
            );
            reply.send({'viewport': viewport.toMessage()});
          case 'releaseViewportContinuation':
            document.releaseViewportContinuation(
              FlarkViewport.fromMessage(
                arguments['viewport']! as Map<Object?, Object?>,
              ),
            );
            reply.send(const <Object?, Object?>{});
          case 'createAnchor':
            reply.send({
              'anchor': document.createAnchorUtf16(
                arguments['utf16']! as int,
                downstream: arguments['downstream']! as bool,
              ),
            });
          case 'resolveAnchor':
            reply.send({
              'utf16': document.resolveAnchorUtf16(arguments['anchor']! as int),
            });
          case 'releaseAnchor':
            document.releaseAnchor(arguments['anchor']! as int);
            reply.send(const <Object?, Object?>{});
          case 'inspect':
            final inspection = document.inspect();
            reply.send({
              'sessionState': inspection.sessionState,
              'revision': inspection.revision,
              'liveTransactions': inspection.liveTransactions,
              'liveContinuations': inspection.liveContinuations,
              'liveAnchors': inspection.liveAnchors,
              'liveHistoryTokens': inspection.liveHistoryTokens,
            });
          case 'readSource':
            reply.send({'source': document.readSource()});
          case 'readSourceRange':
            reply.send({
              'source': document.readSourceRange(
                arguments['startByte']! as int,
                arguments['endByte']! as int,
              ),
            });
          case 'readSourceUtf16Range':
            reply.send({
              'source': document.readSourceUtf16Range(
                arguments['startUtf16']! as int,
                arguments['endUtf16']! as int,
              ),
            });
          case 'dispose':
            document.close();
            reply.send(const <Object?, Object?>{});
            commands.close();
          default:
            throw UnsupportedError('Unknown operation: $operation');
        }
      } on FlarkNativeException catch (error) {
        reply.send({
          'nativeError': {
            'operation': error.operation,
            'status': error.status,
            'detail': error.detail,
          },
        });
      } catch (error, stackTrace) {
        reply.send({'error': '$error\n$stackTrace'});
      }
      if (operation == 'dispose') break;
    }
  } on FlarkNativeException catch (error) {
    startupPort.send({
      'nativeError': {
        'operation': error.operation,
        'status': error.status,
        'detail': error.detail,
      },
    });
  } catch (error, stackTrace) {
    startupPort.send({'error': '$error\n$stackTrace'});
  }
}
