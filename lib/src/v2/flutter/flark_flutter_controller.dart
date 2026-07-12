import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/inline/flark_inline_delimiter_placement.dart';
import '../markdown/inline/flark_inline_run_scanner.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../projection/projection.dart';
import '../render_plan/render_plan.dart';
import 'flark_parse_scheduler.dart';
import 'flark_text_delta_adapter.dart';

/// Kill switch for the RFC 022 adoption-time confirmation (debug builds
/// only). Leave enabled; a test exercising deliberately divergent predictions
/// may disable it around the divergence.
@visibleForTesting
bool flarkDebugValidatePredictionAdoption = true;

/// RFC 022 §4 telemetry: predicted hidden/replacement ranges *outside* the
/// edited region that the authoritative parse did not confirm. Markdown is
/// non-local (a fence opener restructures everything after it), so a nonzero
/// count is not by itself a bug — it measures how often geometry-only
/// prediction goes stale, and which flows need honest
/// `projectionInvalidationRange` metadata (Phase 1) before this can be
/// promoted to an assertion.
@visibleForTesting
int flarkDebugUnconfirmedPredictionRanges = 0;

enum FlarkControllerEventKind {
  runtimeChanged,
  selectionChanged,
  projectionPredicted,
  parseAdopted,
  undo,
  redo,
  pendingInlineStylesChanged,
}

final class FlarkControllerEvent {
  const FlarkControllerEvent({
    required this.kind,
    required this.revision,
    required this.previousRevision,
    required this.markdownChanged,
    required this.selectionChanged,
  });

  final FlarkControllerEventKind kind;
  final int revision;
  final int previousRevision;
  final bool markdownChanged;
  final bool selectionChanged;
}

/// Controller for shared Markdown editor and preview state.
///
/// ## Observing changes
///
/// There is one semantic event model: the [events] stream. Each logical change
/// emits exactly one [FlarkControllerEvent]. For the common cases prefer the
/// typed projections [markdownChanges] and [selectionChanges] instead of
/// hand-filtering event kinds.
///
/// This class is also a [ChangeNotifier]; `addListener` is the low-level
/// "something changed, rebuild" signal used by the editor/preview widgets to
/// resync. Application code should observe [events] (or its projections) rather
/// than `addListener` — they fire together, but only [events] carries what
/// changed.
final class FlarkFlutterController extends ChangeNotifier {
  FlarkFlutterController({
    required FlarkEditorRuntime runtime,
    FlarkProjection? projection,
    FlarkRenderPlan? renderPlan,
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile parseProfile = FlarkMarkdownProfile.commonMarkGfm,
    Duration parseDebounce = const Duration(milliseconds: 80),
    void Function(Object error, StackTrace stackTrace)? onParseError,
    FlarkTextDeltaAdapter textDeltaAdapter = const FlarkTextDeltaAdapter(),
    FlarkProjectedTextEditAdapter projectedTextEditAdapter =
        const FlarkProjectedTextEditAdapter(),
  }) : _runtime = runtime,
       _projection =
           projection ??
           FlarkProjection(textLength: runtime.state.document.length),
       _renderPlan = renderPlan ?? _staleRenderPlan(runtime.state.revision),
       _renderPlanRevision = renderPlan == null ? null : runtime.state.revision,
       _parseBackend = parseBackend,
       _parseProfile = parseProfile,
       _parseDebounce = parseDebounce,
       _onParseError = onParseError,
       _textDeltaAdapter = textDeltaAdapter,
       _projectedTextEditAdapter = projectedTextEditAdapter;

  factory FlarkFlutterController.fromMarkdown(
    String markdown, {
    FlarkExtensionSet? extensions,
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile parseProfile = FlarkMarkdownProfile.commonMarkGfm,
    Duration parseDebounce = const Duration(milliseconds: 80),
    void Function(Object error, StackTrace stackTrace)? onParseError,
  }) {
    return FlarkFlutterController(
      runtime: FlarkEditorRuntime.fromMarkdown(
        markdown,
        extensions: extensions ?? FlarkMarkdownEditingExtensions.standard(),
      ),
      parseBackend: parseBackend,
      parseProfile: parseProfile,
      parseDebounce: parseDebounce,
      onParseError: onParseError,
    );
  }

  FlarkEditorRuntime _runtime;
  FlarkProjection _projection;
  FlarkRenderPlan _renderPlan;
  int? _renderPlanRevision;
  FlarkProjectionPrediction? _lastProjectionPrediction;

  // Delimiter ranges the last adopted edit itself authored and pre-hid (debug
  // builds only). The documented contract (live_edit_intent_pipeline.md) is
  // that the following parse re-derives these exact hidden ranges; adoption
  // asserts that so an invalid authored write fails loudly in every test.
  List<FlarkSourceRange>? _debugAuthoredMarkerRanges;
  FlarkMarkdownParseBackend? _parseBackend;
  FlarkMarkdownProfile _parseProfile;
  Duration _parseDebounce;
  void Function(Object error, StackTrace stackTrace)? _onParseError;
  FlarkParseScheduler? _parseScheduler;
  bool _parseStarted = false;
  int _parseSurfaceCount = 0;
  bool _disposed = false;
  final FlarkTextDeltaAdapter _textDeltaAdapter;
  final FlarkProjectedTextEditAdapter _projectedTextEditAdapter;
  Set<FlarkMarkdownInlineStyle> _pendingInlineStyles =
      <FlarkMarkdownInlineStyle>{};
  Set<FlarkMarkdownInlineStyle> _mutedInlineStyles =
      <FlarkMarkdownInlineStyle>{};
  Set<FlarkMarkdownInlineStyle>? _armedContinuationOverride;
  List<FlarkAuthoredMarker>? _pendingAuthoredMarkers;
  bool _lastEditRequestsImmediateParse = false;
  final StreamController<FlarkControllerEvent> _events =
      StreamController<FlarkControllerEvent>.broadcast();

  /// Inline styles "armed" for the collapsed caret but not yet applied to any
  /// source text.
  ///
  /// Toggling an inline style with a collapsed caret arms it here instead of
  /// editing the document (see [togglePendingInlineStyle]). The next typed run
  /// is wrapped in the armed markers, and any selection change or other edit
  /// clears the set. Selection-based toggling never touches this.
  static const List<FlarkMarkdownInlineStyle> _pendingInlineStyleOrder = [
    FlarkMarkdownInlineStyle.emphasis,
    FlarkMarkdownInlineStyle.strong,
    FlarkMarkdownInlineStyle.strikethrough,
    FlarkMarkdownInlineStyle.inlineCode,
  ];

  /// A single bare URL (no whitespace): `http(s)://…` or `www.…`. Used to
  /// detect a URL pasted over a selection so it can be wrapped as a link.
  static final RegExp _urlPattern = RegExp(r'^(?:https?://|www\.)\S+$');

  FlarkEditorRuntime get runtime => _runtime;

  /// Whether the controller-owned background parser is running (debounced
  /// re-parsing in response to edits).
  bool get isParsing => _parseStarted;

  /// Starts the controller-owned background parser if it is not already running.
  ///
  /// This is idempotent. Editor and preview surfaces that share one controller
  /// all call this, but a single parser is created per controller — one
  /// document is parsed once, regardless of how many widgets observe it.
  ///
  /// The default Comrak backend is resolved lazily here (not at construction),
  /// so headless controllers created for tests or server-side render plans do
  /// not require the native bridge until a surface actually starts parsing.
  void ensureParsing() {
    if (_disposed) return;
    final FlarkParseScheduler scheduler;
    try {
      // The lazily resolved default backend can throw when the native bridge
      // fails to load; route it to the callback instead of crashing the surface
      // that called this from initState (matches [tryParseSync]).
      scheduler = _ensureScheduler();
    } catch (error, stackTrace) {
      _onParseError?.call(error, stackTrace);
      return;
    }
    scheduler.start();
    _parseStarted = true;
  }

  /// Registers an editing surface that needs background parsing.
  ///
  /// The controller keeps a single parser running while at least one surface is
  /// attached, and stops it when the last surface detaches (see
  /// [detachParsingSurface]). Widgets call this in `initState`; app code that
  /// drives a controller without a widget can call [ensureParsing] instead.
  void attachParsingSurface() {
    if (_disposed) return;
    _parseSurfaceCount += 1;
    ensureParsing();
  }

  /// Detaches a surface previously registered with [attachParsingSurface].
  ///
  /// When the last attached surface detaches, the background parser is stopped
  /// so a controller observed only by disposed widgets does not keep timers
  /// pending. The controller and its current render plan remain usable.
  void detachParsingSurface() {
    if (_parseSurfaceCount > 0) _parseSurfaceCount -= 1;
    if (_parseSurfaceCount > 0 || !_parseStarted) return;
    _parseScheduler?.dispose();
    _parseScheduler = null;
    _parseStarted = false;
  }

  /// Reconfigures the controller-owned parser, restarting it if running.
  ///
  /// Only non-null arguments override existing configuration. Pass
  /// [clearOnParseError] to drop a previously configured error callback.
  void configureParsing({
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile? parseProfile,
    Duration? parseDebounce,
    void Function(Object error, StackTrace stackTrace)? onParseError,
    bool clearOnParseError = false,
  }) {
    // Only backend/profile/debounce changes need a scheduler restart. The
    // error callback is read through a stable forwarder, so swapping it in
    // place keeps the debounce timer running — widget rebuilds that pass a
    // fresh inline closure must not restart parsing every frame.
    final restartNeeded =
        parseBackend != null || parseProfile != null || parseDebounce != null;
    if (parseBackend != null) _parseBackend = parseBackend;
    if (parseProfile != null) _parseProfile = parseProfile;
    if (parseDebounce != null) _parseDebounce = parseDebounce;
    if (onParseError != null || clearOnParseError) {
      _onParseError = onParseError;
    }
    if (_parseScheduler == null || !restartNeeded) return;
    final wasStarted = _parseStarted;
    _parseScheduler!.dispose();
    _parseScheduler = null;
    _parseStarted = false;
    if (wasStarted) ensureParsing();
  }

  /// Parses until the current revision has an authoritative render plan,
  /// bypassing the debounce window.
  ///
  /// Resolves immediately when the plan is already authoritative, and chains
  /// onto an in-flight parse instead of silently returning, so the returned
  /// future means "the plan is current". If [ensureParsing] has not been
  /// called, this performs a one-shot parse without installing a background
  /// debounce loop, so advanced widgets that drive parsing per structural
  /// edit do not leak pending timers. Errors are routed to the configured
  /// parse-error callback rather than thrown.
  Future<void> parseNow() async {
    if (_disposed) return;
    try {
      // _ensureScheduler resolves the default backend lazily and can throw when
      // the native bridge fails to load — keep it inside the try so the failure
      // routes to the callback rather than escaping this never-throw method.
      final scheduler = _ensureScheduler();
      await scheduler.parseNow();
    } catch (error, stackTrace) {
      _onParseError?.call(error, stackTrace);
    }
  }

  /// Attempts to parse the current revision synchronously, so a caller about
  /// to build a preview can hold an authoritative [renderPlan] before the
  /// first frame paints — no raw-source flash.
  ///
  /// Returns whether the plan is authoritative afterwards. Requires a backend
  /// implementing [FlarkSyncCapableParseBackend] (the default Comrak backend
  /// qualifies for documents small enough to parse on the calling isolate);
  /// otherwise returns false and an async parse ([parseNow] or the scheduled
  /// parser) is the fallback. Like [parseNow], this does not install the
  /// background debounce loop.
  ///
  /// Adopting the plan notifies listeners **synchronously**. Call this before
  /// widgets or other listeners attach (e.g. on a freshly created controller,
  /// as the standalone [FlarkMarkdown] preview does) — invoking it mid-build
  /// on a controller that already has attached listeners can mark built
  /// elements dirty during the build phase. Errors (including backend load
  /// failures) are routed to the configured parse-error callback and report
  /// as false, never thrown.
  bool tryParseSync() {
    if (_disposed) return false;
    final FlarkParseScheduler scheduler;
    try {
      // The lazily resolved default backend can throw when the native bridge
      // fails to load; the documented contract here is false-and-fall-back.
      scheduler = _ensureScheduler();
    } catch (error, stackTrace) {
      _onParseError?.call(error, stackTrace);
      return false;
    }
    return scheduler.tryParseSync();
  }

  FlarkParseScheduler _ensureScheduler() {
    return _parseScheduler ??= FlarkParseScheduler(
      controller: this,
      backend: _parseBackend ?? FlarkNativeComrakParseBackend.requiredDefault(),
      profile: _parseProfile,
      debounce: _parseDebounce,
      // A stable forwarder, so configureParsing can swap the callback
      // without restarting the scheduler.
      onError: (error, stackTrace) => _onParseError?.call(error, stackTrace),
    );
  }

  Stream<FlarkControllerEvent> get events => _events.stream;

  /// Emits the current [markdown] whenever the document text changes.
  ///
  /// A typed projection of [events] for the most common observation case —
  /// selection-only changes do not emit here.
  Stream<String> get markdownChanges =>
      events.where((event) => event.markdownChanged).map((_) => markdown);

  /// Emits the current [selection] whenever it changes.
  ///
  /// A typed projection of [events]; document edits that also move the caret
  /// emit here as well as on [markdownChanges].
  Stream<FlarkSelection> get selectionChanges =>
      events.where((event) => event.selectionChanged).map((_) => selection);

  FlarkEditorState get state => _runtime.state;

  String get markdown => state.markdown;

  FlarkSelection get selection => state.selection;

  /// The inline styles currently armed for the collapsed caret.
  ///
  /// Empty unless a style was toggled on an empty/collapsed selection and no
  /// edit or selection change has cleared it since. Toolbars can read this (or
  /// the unified `commands.strongActive`/`isInlineActive`) to reflect armed
  /// formatting before any text is typed.
  Set<FlarkMarkdownInlineStyle> get pendingInlineStyles =>
      Set<FlarkMarkdownInlineStyle>.unmodifiable(_pendingInlineStyles);

  /// Arms or disarms an inline [style] for the collapsed caret.
  ///
  /// With a collapsed caret there is no range to wrap, so instead of editing
  /// the document this flips the style's membership in [pendingInlineStyles]:
  /// the next typed run is wrapped in the armed markers. Toggling the same
  /// style again before typing disarms it. This does not change the document
  /// or selection, so it is not recorded in history.
  void togglePendingInlineStyle(FlarkMarkdownInlineStyle style) {
    if (_disposed) return;
    if (!_pendingInlineStyles.add(style)) {
      _pendingInlineStyles.remove(style);
    }
    _emitEvent(
      kind: FlarkControllerEventKind.pendingInlineStylesChanged,
      previousState: state,
    );
    notifyListeners();
  }

  /// The inline styles "armed off" for the collapsed caret: toggled off while
  /// the caret sits inside a run of that style, so the next typed character
  /// leaves the run (at an edge) or splits it (mid-run) instead of unwrapping
  /// the text already written.
  Set<FlarkMarkdownInlineStyle> get mutedInlineStyles =>
      Set<FlarkMarkdownInlineStyle>.unmodifiable(_mutedInlineStyles);

  /// Whether the most recent [applyProjectedTextEdit] turned a keystroke into
  /// new Markdown structure — an armed wrap (`**x**`), a selection wrap, a
  /// smart-link paste, or a muted-run split. Editing surfaces parse immediately
  /// when true so the structure renders without waiting for the debounced
  /// parse, matching what happens when the same markers are typed by hand.
  bool get lastEditRequestsImmediateParse => _lastEditRequestsImmediateParse;

  /// Arms or disarms removing an inline [style] for the collapsed caret.
  ///
  /// Used when toggling a style off with the caret inside its run: the run is
  /// left intact and the next typed character is placed outside it. Toggling
  /// again before typing re-enables the style. Changes neither the document nor
  /// the selection, so it is not recorded in history.
  void toggleMutedInlineStyle(FlarkMarkdownInlineStyle style) {
    if (_disposed) return;
    if (!_mutedInlineStyles.add(style)) {
      _mutedInlineStyles.remove(style);
    }
    _emitEvent(
      kind: FlarkControllerEventKind.pendingInlineStylesChanged,
      previousState: state,
    );
    notifyListeners();
  }

  /// The open/close marker pair for the currently armed styles, or null when
  /// none are armed. Opening markers nest outer-to-inner in a fixed canonical
  /// order; closing markers mirror them. Bold + italic therefore yields
  /// `***…***`, and inline code stays innermost so its delimiters hug content.
  ({String open, String close})? _pendingInsertionWrap() {
    if (_pendingInlineStyles.isEmpty) return null;
    final ordered = [
      for (final style in _pendingInlineStyleOrder)
        if (_pendingInlineStyles.contains(style)) style,
    ];
    return (
      open: ordered.map((style) => style.marker).join(),
      close: ordered.reversed.map((style) => style.marker).join(),
    );
  }

  /// Whether arming [style] now would actually wrap the next typed run.
  ///
  /// Returns false when the wrap's marker would merge with an adjacent marker
  /// character and be dropped at type time — for example arming italic at a
  /// bold run's trailing edge, where the would-be `**a*b***` is not
  /// representable in CommonMark (the inner emphasis parses as literal). A
  /// collapsed-caret toggle consults this so the toolbar never lights up a
  /// style that the next keystroke would silently drop. Disarming an
  /// already-armed style, and any non-collapsed (selection) toggle, always
  /// apply.
  bool wouldArmInlineStyleApply(FlarkMarkdownInlineStyle style) {
    final selection = state.selection;
    if (!selection.isCollapsed) return true;
    if (_pendingInlineStyles.contains(style)) return true;
    final source = markdown;
    final caret = selection.start;
    if (caret < 0 || caret > source.length) return true;
    final ordered = [
      for (final candidate in _pendingInlineStyleOrder)
        if (candidate == style || _pendingInlineStyles.contains(candidate))
          candidate,
    ];
    if (ordered.isEmpty) return true;
    return !FlarkProjectedTextEditAdapter.wrapMarkersWouldMerge(
      source,
      caret,
      open: ordered.map((s) => s.marker).join(),
      close: ordered.reversed.map((s) => s.marker).join(),
    );
  }

  /// Switches the next typed run to [style], dropping the inline run(s) the
  /// caret currently sits inside ("last action wins").
  ///
  /// Used when [style] cannot combine with those runs at the caret — italic at
  /// a bold run's trailing edge has no canonical nesting (`**a*b***` parses as
  /// literal), so instead of doing nothing, the bold run is muted (the next
  /// character exits it) and italic is armed, starting a clean sibling run.
  /// Returns false when the caret is not inside any run to switch out of, so
  /// the caller can leave the toggle a no-op.
  bool switchToInlineStyle(FlarkMarkdownInlineStyle style) {
    if (_disposed) return false;
    final enclosing = [
      for (final candidate in FlarkMarkdownInlineStyle.values)
        if (candidate != style &&
            FlarkMarkdownCommandQueries.enclosingInlineRun(state, candidate) !=
                null)
          candidate,
    ];
    if (enclosing.isEmpty) return false;
    _mutedInlineStyles = {..._mutedInlineStyles, ...enclosing}..remove(style);
    _pendingInlineStyles = {..._pendingInlineStyles, style};
    _emitEvent(
      kind: FlarkControllerEventKind.pendingInlineStylesChanged,
      previousState: state,
    );
    notifyListeners();
    return true;
  }

  /// The marker pair wrapping text that exits a muted run while a style is
  /// armed, or null when nothing is armed.
  ///
  /// Each armed emphasis/strong style uses its alternate delimiter (`_`/`__`
  /// instead of `*`/`**`) when its default would sit flush against
  /// [adjacentMarker] and merge into a corrupt run. An armed italic exiting a
  /// `**…**` bold run therefore wraps as `_x_`, yielding the canonical sibling
  /// `**bold**_x_` (strong then emphasis) rather than the literal `**bold***x*`.
  ({String open, String close})? _armedExitWrap(String adjacentMarker) {
    if (_pendingInlineStyles.isEmpty) return null;
    final adjacentChar = adjacentMarker.isEmpty
        ? null
        : adjacentMarker.codeUnitAt(0);
    final markers = [
      for (final style in _pendingInlineStyleOrder)
        if (_pendingInlineStyles.contains(style))
          _exitMarkerFor(style, adjacentChar),
    ];
    if (markers.isEmpty) return null;
    return (open: markers.join(), close: markers.reversed.join());
  }

  static String _exitMarkerFor(
    FlarkMarkdownInlineStyle style,
    int? adjacentChar,
  ) {
    final alternate = _alternateInlineMarker(style);
    if (alternate != null && style.marker.codeUnitAt(0) == adjacentChar) {
      return alternate;
    }
    return style.marker;
  }

  /// The same-meaning delimiter built from the other character, for the two
  /// styles whose markers can collide with an adjacent run (`*`↔`_`, `**`↔`__`).
  /// Inline code and strikethrough have no colliding alternate.
  static String? _alternateInlineMarker(FlarkMarkdownInlineStyle style) {
    return switch (style) {
      FlarkMarkdownInlineStyle.emphasis => '_',
      FlarkMarkdownInlineStyle.strong => '__',
      _ => null,
    };
  }

  FlarkProjection get projection => _projection;

  FlarkRenderPlan get renderPlan => _renderPlan;

  FlarkProjectionPrediction? get lastProjectionPrediction {
    return _lastProjectionPrediction;
  }

  bool get hasAuthoritativeRenderPlan {
    return _renderPlanRevision == state.revision;
  }

  /// Whether [renderPlan] is renderable by block-based surfaces.
  ///
  /// True for an authoritative plan of the current revision, and for a
  /// non-empty predicted plan mapped through recent edits. False only when the
  /// plan is a stale placeholder (or a prediction emptied of blocks), in which
  /// case surfaces fall back to plain projected text until the next parse.
  bool get hasUsableRenderPlan {
    assert(
      _renderPlan.blocks.isEmpty ||
          _renderPlan.fidelity != FlarkRenderPlanFidelity.stale,
      'Stale render plans must not carry blocks.',
    );
    return hasAuthoritativeRenderPlan || _renderPlan.blocks.isNotEmpty;
  }

  FlarkEditorRuntimeResult dispatch<TPayload>({
    required FlarkCommand<TPayload> command,
    required TPayload payload,
  }) {
    final result = _runtime.dispatch(command: command, payload: payload);
    _adoptRuntimeResult(result);
    return result;
  }

  FlarkEditorRuntimeResult applyTransaction(FlarkTransaction transaction) {
    final result = _runtime.applyTransaction(transaction);
    _adoptRuntimeResult(result);
    return result;
  }

  bool applyTextEditingDelta(TextEditingDelta delta) {
    final transaction = _textDeltaAdapter.transactionFromDelta(
      delta,
      currentMarkdown: markdown,
    );
    if (transaction == null) return false;
    applyTransaction(transaction);
    return true;
  }

  /// Converts [html] (e.g. the clipboard's `text/html` flavor) to Markdown and
  /// inserts it at the caret, replacing any selection. Returns false when the
  /// HTML converts to nothing.
  ///
  /// Flark stays platform-agnostic and does not read the clipboard itself: an
  /// app wires its own paste handler to read the clipboard's `text/html` flavor
  /// (e.g. via the `super_clipboard` package, or `clipboardData` on the web)
  /// and call this. The raw conversion is also available as
  /// [FlarkHtmlMarkdown.convert].
  bool insertHtmlAsMarkdown(String html, {int? undoGroupId}) {
    final converted = FlarkHtmlMarkdown.convert(html);
    if (converted.isEmpty) return false;
    final range = FlarkSourceRange(selection.start, selection.end);
    applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: range,
          replacementText: converted,
        ),
        selectionBefore: selection,
        selectionAfter: FlarkSelection.collapsed(
          range.start + converted.length,
        ),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.paste,
          userEvent: 'input.htmlPaste',
          undoGroupId: undoGroupId,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
    );
    return true;
  }

  bool applyProjectedTextEdit({
    required String oldDisplayText,
    required String newDisplayText,
    int? newDisplayCaret,
    int? undoGroupId,
    FlarkMapAffinity fallbackInsertionAffinity = FlarkMapAffinity.downstream,
  }) {
    // Input recognizers run before the plain edit adapter: a change that
    // replaces the whole selection with a single token can mean "wrap the
    // selection" rather than "replace it". Each recognizer returns null to fall
    // through to the next, then to the adapter. Order matters only when two
    // could match the same token (they currently cannot).
    _lastEditRequestsImmediateParse = false;

    final replacement = _selectionReplacement(
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
    );
    if (replacement != null) {
      final recognized =
          _wrapSelectionRecognizer(replacement, undoGroupId) ??
          _smartLinkPasteRecognizer(replacement, undoGroupId);
      if (recognized != null) {
        applyTransaction(recognized);
        _lastEditRequestsImmediateParse = true;
        return true;
      }
    }

    // A style toggled off inside its run (muted) places the next typed
    // character outside the run instead of extending it.
    final mutedExit = _mutedExitTransaction(
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
      undoGroupId: undoGroupId,
    );
    if (mutedExit != null) {
      _armedContinuationOverride = _continuationStylesFor(
        mutedExit.continuationMarker,
      );
      _pendingAuthoredMarkers = mutedExit.authoredMarkers;
      applyTransaction(mutedExit.transaction);
      _armedContinuationOverride = null;
      _pendingAuthoredMarkers = null;
      _lastEditRequestsImmediateParse = true;
      return true;
    }

    // An armed insertion wrap turns one keystroke into a `**x**`-style run; flag
    // it so the surface parses immediately and the markers hide right away
    // (otherwise they show raw until the debounced parse, and a backspace in
    // that window cannot expand over the not-yet-recognized markers).
    final insertionWrap = _pendingInsertionWrap();
    final resolution = _projectedTextEditAdapter.resolveDisplayEdit(
      currentMarkdown: markdown,
      projection: projection,
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
      sourceSelectionBefore: selection,
      newDisplayCaret: newDisplayCaret,
      undoGroupId: undoGroupId,
      fallbackInsertionAffinity: fallbackInsertionAffinity,
      insertionWrap: insertionWrap,
    );
    if (resolution == null) return false;
    // Whitespace committed outside a run's delimiters keeps the run's styles
    // armed across the edit (the adoption chokepoint would otherwise clear
    // them), so the next styled keystroke re-enters the run.
    _armedContinuationOverride = _continuationStylesFor(
      resolution.continuationMarker,
    );
    _pendingAuthoredMarkers = resolution.authoredMarkers;
    applyTransaction(resolution.transaction);
    _armedContinuationOverride = null;
    _pendingAuthoredMarkers = null;
    _lastEditRequestsImmediateParse = resolution.requestsImmediateParse;
    // Typing the `]` that completes a bare task marker (`- [ ]`) auto-inserts
    // the trailing space GFM requires before content, so the next character
    // stays in the checkbox (`- [ ] f`) instead of breaking the task back into
    // a plain bullet (`- [ ]f`). Only on a net insertion, so backspacing the
    // space is not fought.
    if (insertionWrap == null &&
        newDisplayText.length > oldDisplayText.length) {
      _autoSpaceCompletedTaskMarker(undoGroupId);
    }
    return true;
  }

  /// Canonicalizes a boundary-resolved inline deletion of [range] — the range
  /// a Backspace/forward-Delete resolver produced after stepping past hidden
  /// markers — through the inline placement repairs, so a keyboard deletion
  /// never leaves invalid markdown: stranded edge whitespace (`**foo x**`
  /// backspacing `x` → `**foo** `, never `**foo **`), fused adjacent runs
  /// (`**a** **b**` minus the gap → `**ab**`), or an orphaned crossing marker.
  /// The repair is applied with predictive marker hiding and armed-continuation
  /// threading, exactly like the projected-edit path.
  ///
  /// Returns true when a repair was applied. Returns false when the plain
  /// deletion of [range] is already valid (no repair applies) — the caller
  /// then performs its own deletion, keeping block-aware handling intact.
  bool applyResolvedInlineDeletion(
    FlarkSourceRange range, {
    int? undoGroupId,
    String userEvent = 'input.inlineDeletionRepair',
  }) {
    if (_disposed || range.isCollapsed) return false;
    final source = markdown;
    final runs = projection.inlineRunScans(source);
    final repair =
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: source,
          start: range.start,
          end: range.end,
          text: '',
          runs: runs,
        ) ??
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: source,
          start: range.start,
          end: range.end,
          text: '',
          runs: projection.inlineRunScans(source, includeCodeSpans: true),
        ) ??
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: range.start,
          end: range.end,
          text: '',
          runs: runs,
        );
    if (repair == null) return false;
    _armedContinuationOverride = _continuationStylesFor(
      repair.continuationMarker,
    );
    _pendingAuthoredMarkers = repair.authoredMarkers;
    applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: repair.range,
          replacementText: repair.replacement,
        ),
        selectionBefore: selection,
        selectionAfter: FlarkSelection.collapsed(repair.caretAfter),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: userEvent,
          undoGroupId: undoGroupId,
          parseInvalidationRange: repair.range,
          projectionInvalidationRange: repair.range,
        ),
      ),
    );
    _armedContinuationOverride = null;
    _pendingAuthoredMarkers = null;
    _lastEditRequestsImmediateParse = true;
    return true;
  }

  /// Inserts the trailing space after a task marker the caret just completed
  /// (`- [ ]` → `- [ ] `), leaving the caret after it.
  void _autoSpaceCompletedTaskMarker(int? undoGroupId) {
    final selection = this.selection;
    if (!selection.isCollapsed) return;
    final at = _completedTaskMarkerCaret(markdown, selection.extentOffset);
    if (at == null) return;
    applyTransaction(
      FlarkTransaction.single(
        FlarkSourceOperation.insert(at, ' '),
        selectionBefore: selection,
        selectionAfter: FlarkSelection.collapsed(at + 1),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.taskMarkerAutoSpace',
          undoGroupId: undoGroupId,
          parseInvalidationRange: FlarkSourceRange(at, at),
          projectionInvalidationRange: FlarkSourceRange(at, at),
        ),
      ),
    );
    _lastEditRequestsImmediateParse = true;
  }

  static final RegExp _bareTaskMarkerLine = RegExp(r'^[ \t]*[-*+] \[[ xX]\]$');

  /// The caret offset at which to auto-insert a task marker's trailing space —
  /// when [caret] sits at the end of a line that is exactly a bare task marker
  /// (`- [ ]`/`- [x]`), the state the completing `]` just produced — else null.
  static int? _completedTaskMarkerCaret(String source, int caret) {
    if (caret <= 0 || caret > source.length) return null;
    var start = caret;
    while (start > 0 && source.codeUnitAt(start - 1) != 0x0A) {
      start -= 1;
    }
    var end = caret;
    while (end < source.length && source.codeUnitAt(end) != 0x0A) {
      end += 1;
    }
    if (caret != end) return null;
    if (!_bareTaskMarkerLine.hasMatch(source.substring(start, end))) {
      return null;
    }
    return caret;
  }

  /// A projected change that replaces exactly the current plain-text selection
  /// with a single inserted token, or null. Shared by the selection-wrap and
  /// smart-link-paste recognizers.
  ///
  /// "Plain" means the selected source equals the selected display (no hidden
  /// markers inside the selection), so a recognizer can wrap/replace the source
  /// range directly.
  _SelectionReplacement? _selectionReplacement({
    required String oldDisplayText,
    required String newDisplayText,
  }) {
    final selection = this.selection;
    if (selection.isCollapsed) return null;
    if (projection.projectText(markdown) != oldDisplayText) return null;

    final displayStart = projection.sourceToDisplayOffset(selection.start);
    final displayEnd = projection.sourceToDisplayOffset(selection.end);
    if (displayStart >= displayEnd || displayEnd > oldDisplayText.length) {
      return null;
    }

    final prefix = oldDisplayText.substring(0, displayStart);
    final suffix = oldDisplayText.substring(displayEnd);
    if (!newDisplayText.startsWith(prefix) ||
        !newDisplayText.endsWith(suffix) ||
        newDisplayText.length < prefix.length + suffix.length) {
      return null;
    }
    final content = markdown.substring(selection.start, selection.end);
    if (content.isEmpty ||
        content != oldDisplayText.substring(displayStart, displayEnd)) {
      return null;
    }
    return _SelectionReplacement(
      range: FlarkSourceRange(selection.start, selection.end),
      content: content,
      inserted: newDisplayText.substring(
        prefix.length,
        newDisplayText.length - suffix.length,
      ),
    );
  }

  /// Typing a delimiter or bracket/quote over a selection wraps it (`*foo*`,
  /// `(foo)`) instead of replacing it, leaving the inner text selected so a
  /// second keystroke nests (`*foo*` → `**foo**`).
  ///
  /// Emphasis-family delimiters hug the selection's core: edge whitespace
  /// stays outside the markers (`foo ` + `*` → `*foo* `), and a
  /// whitespace-only selection falls through to a plain replacement — there
  /// is nothing CommonMark could style.
  FlarkTransaction? _wrapSelectionRecognizer(
    _SelectionReplacement replacement,
    int? undoGroupId,
  ) {
    final pair = _wrapPairFor(replacement.inserted);
    if (pair == null) return null;
    final range = replacement.range;
    var content = replacement.content;
    var contentStart = range.start + pair.open.length;
    final String wrapped;
    if (replacement.inserted == '*' || replacement.inserted == '_') {
      // A blank line inside the selection would make the wrap span two
      // paragraphs (`*alpha\n\nbeta*`), which CommonMark treats as literal
      // markers, not one run. Decline so the keystroke replaces the selection
      // with a literal delimiter instead of writing invalid source. (Brackets
      // and quotes below are not markdown emphasis and may span paragraphs.)
      if (_paragraphBreakPattern.hasMatch(content)) return null;
      final split = FlarkInlineDelimiterPlacement.splitEdgeWhitespace(content);
      if (split.core.isEmpty) return null;
      wrapped =
          '${split.leading}${pair.open}${split.core}${pair.close}'
          '${split.trailing}';
      content = split.core;
      contentStart = range.start + split.leading.length + pair.open.length;
    } else {
      // A code span cannot cross a blank line either (its backticks would land
      // in separate blocks and render literally); brackets and quotes are not
      // markdown delimiters and may span paragraphs.
      if (replacement.inserted == '`' &&
          _paragraphBreakPattern.hasMatch(content)) {
        return null;
      }
      wrapped = '${pair.open}$content${pair.close}';
    }
    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: range,
        replacementText: wrapped,
      ),
      selectionBefore: selection,
      selectionAfter: FlarkSelection(
        baseOffset: contentStart,
        extentOffset: contentStart + content.length,
      ),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'input.wrapSelection',
        undoGroupId: undoGroupId,
        parseInvalidationRange: range,
        projectionInvalidationRange: range,
      ),
    );
  }

  /// Pasting a URL over a selection wraps it as `[selected](url)` instead of
  /// replacing the text with the bare URL. Skips a selection that is itself a
  /// URL (a deliberate URL-for-URL replacement).
  FlarkTransaction? _smartLinkPasteRecognizer(
    _SelectionReplacement replacement,
    int? undoGroupId,
  ) {
    if (!_urlPattern.hasMatch(replacement.inserted)) return null;
    if (_urlPattern.hasMatch(replacement.content)) return null;
    final range = replacement.range;
    final linked = '[${replacement.content}](${replacement.inserted})';
    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: range,
        replacementText: linked,
      ),
      selectionBefore: selection,
      selectionAfter: FlarkSelection.collapsed(range.start + linked.length),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.paste,
        userEvent: 'input.smartLinkPaste',
        undoGroupId: undoGroupId,
        parseInvalidationRange: range,
        projectionInvalidationRange: range,
      ),
    );
  }

  /// When a style is muted (toggled off inside its run), the next typed
  /// character leaves the run rather than extending it: inserted after the
  /// closing marker at the trailing edge, before the opening marker at the
  /// leading edge, or splitting the run (`**foo**x**bar**`) in the middle.
  /// Returns null unless the change is a plain insertion at the caret inside a
  /// muted run.
  _MutedExitResolution? _mutedExitTransaction({
    required String oldDisplayText,
    required String newDisplayText,
    int? undoGroupId,
  }) {
    if (_mutedInlineStyles.isEmpty) return null;
    final selection = this.selection;
    if (!selection.isCollapsed) return null;
    if (projection.projectText(markdown) != oldDisplayText) return null;

    final caret = selection.extentOffset;
    final displayCaret = projection.sourceToDisplayOffset(caret);
    if (displayCaret > oldDisplayText.length) return null;
    final prefix = oldDisplayText.substring(0, displayCaret);
    final suffix = oldDisplayText.substring(displayCaret);
    if (!newDisplayText.startsWith(prefix) ||
        !newDisplayText.endsWith(suffix) ||
        newDisplayText.length <= prefix.length + suffix.length) {
      return null;
    }
    final text = newDisplayText.substring(
      prefix.length,
      newDisplayText.length - suffix.length,
    );

    // Exits relocate delimiters, so the run must come from the parser's own
    // pairing (via the projection), never the textual approximation — a muted
    // exit against a run the parser reads differently would rewrite literal
    // text.
    final runs = projection.inlineRunScans(markdown);
    FlarkInlineRunScan? innermostAtCaret;
    for (final run in runs) {
      if (run.contentStart <= caret &&
          caret <= run.closeStart &&
          (innermostAtCaret == null ||
              run.contentStart > innermostAtCaret.contentStart)) {
        innermostAtCaret = run;
      }
    }
    for (final style in _mutedInlineStyles) {
      // Code spans are absent from the emphasis-family scans (their backticks
      // must never be whitespace-relocated), so a muted code exit finds its
      // run through the textual backtick scan instead — safe, because a code
      // exit only writes *around* the markers, never moves them.
      if (style == FlarkMarkdownInlineStyle.inlineCode) {
        final run = FlarkMarkdownCommandQueries.enclosingInlineRun(
          state,
          style,
        );
        if (run != null) {
          return _runExitTransaction(run, caret, text, undoGroupId, runs);
        }
        continue;
      }
      FlarkInlineRunScan? enclosing;
      for (final run in runs) {
        if (run.contentStart <= caret &&
            caret <= run.closeStart &&
            (_stylesForCluster(run.marker)?.contains(style) ?? false) &&
            (enclosing == null || run.contentStart > enclosing.contentStart)) {
          enclosing = run;
        }
      }
      if (enclosing == null) continue;
      // A middle split closes and reopens the muted run at the caret; when a
      // deeper run spans the split point, those markers would land inside it
      // and overlap its delimiters — CommonMark then discards one pair and
      // the other leaks as literal text. Fall through to a plain insertion
      // instead: the text keeps its styles, which is valid and
      // display-faithful.
      final middle =
          caret > enclosing.contentStart && caret < enclosing.closeStart;
      if (middle &&
          innermostAtCaret != null &&
          innermostAtCaret.contentStart > enclosing.contentStart) {
        continue;
      }
      return _runExitTransaction(
        FlarkInlineRunRange(
          openStart: enclosing.openStart,
          contentStart: enclosing.contentStart,
          closeStart: enclosing.closeStart,
          closeEnd: enclosing.closeEnd,
        ),
        caret,
        text,
        undoGroupId,
        runs,
      );
    }
    return null;
  }

  _MutedExitResolution _runExitTransaction(
    FlarkInlineRunRange run,
    int caret,
    String text,
    int? undoGroupId,
    List<FlarkInlineRunScan> runs,
  ) {
    final FlarkSourceRange range;
    final String replacement;
    final int caretAfter;
    String? continuationMarker;
    var authoredMarkers = const <FlarkAuthoredMarker>[];
    if (caret >= run.closeStart) {
      // Trailing edge: step out past the closing marker. A switched-in style
      // (last action wins) wraps the exited text into a sibling run, picking a
      // delimiter that won't merge with this run's closing marker. The run
      // itself needs no whitespace handling: a flanking-valid run never ends
      // in whitespace, and the write paths never produce one that does.
      final wrap = _armedExitWrap(
        markdown.substring(run.closeStart, run.closeEnd),
      );
      if (wrap == null) {
        range = FlarkSourceRange(run.closeEnd, run.closeEnd);
        replacement = text;
        caretAfter = run.closeEnd + text.length;
      } else {
        // The sibling wrap goes through the canonical placement rules so
        // whitespace-edged exit text (a muted space) never strands the new
        // run's delimiters (`__ __`).
        final placement = FlarkInlineDelimiterPlacement.armedWrap(
          source: markdown,
          caret: run.closeEnd,
          text: text,
          open: wrap.open,
          close: wrap.close,
          edgeSensitive: !wrap.open.contains('`'),
          runs: runs,
        );
        range = placement.range;
        replacement = placement.replacement;
        caretAfter = placement.caretAfter;
        continuationMarker = placement.continuationMarker;
        authoredMarkers = placement.authoredMarkers;
      }
    } else if (caret <= run.contentStart) {
      // Leading edge: step out before the opening marker.
      final wrap = _armedExitWrap(
        markdown.substring(run.openStart, run.contentStart),
      );
      if (wrap == null) {
        range = FlarkSourceRange(run.openStart, run.openStart);
        replacement = text;
        caretAfter = run.openStart + text.length;
      } else {
        final placement = FlarkInlineDelimiterPlacement.armedWrap(
          source: markdown,
          caret: run.openStart,
          text: text,
          open: wrap.open,
          close: wrap.close,
          edgeSensitive: !wrap.open.contains('`'),
          runs: runs,
        );
        range = placement.range;
        replacement = placement.replacement;
        caretAfter = placement.caretAfter;
        continuationMarker = placement.continuationMarker;
        authoredMarkers = placement.authoredMarkers;
      }
    } else {
      // Middle: close the run, drop the plain text, reopen the run — moving
      // whitespace that straddles the split point between the delimiters so
      // both halves stay flanking-valid (`**foo bar**` split after `foo `
      // becomes `**foo** x**bar**`, never `**foo **x**bar**`). Code spans
      // have no flanking rules and their whitespace is content, so they keep
      // the plain split.
      final marker = markdown.substring(run.closeStart, run.closeEnd);
      if (marker.codeUnitAt(0) == 0x60) {
        range = FlarkSourceRange(caret, caret);
        replacement = '$marker$text$marker';
        caretAfter = caret + marker.length + text.length;
        authoredMarkers = [
          FlarkAuthoredMarker(
            range: FlarkSourceRange(caret, caret + marker.length),
            opens: false,
          ),
          FlarkAuthoredMarker(
            range: FlarkSourceRange(caretAfter, caretAfter + marker.length),
            opens: true,
          ),
        ];
      } else {
        final placement = FlarkInlineDelimiterPlacement.runSplit(
          source: markdown,
          contentRange: FlarkSourceRange(run.contentStart, run.closeStart),
          caret: caret,
          marker: marker,
          text: text,
        );
        range = placement.range;
        replacement = placement.replacement;
        caretAfter = placement.caretAfter;
        authoredMarkers = placement.authoredMarkers;
      }
    }
    return _MutedExitResolution(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: range,
          replacementText: replacement,
        ),
        selectionBefore: FlarkSelection.collapsed(caret),
        selectionAfter: FlarkSelection.collapsed(caretAfter),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.mutedInlineStyle',
          undoGroupId: undoGroupId,
          parseInvalidationRange: range,
          projectionInvalidationRange: range,
        ),
      ),
      continuationMarker: continuationMarker,
      authoredMarkers: authoredMarkers,
    );
  }

  /// The inline styles named by a delimiter [cluster] (`**`, `***`, `~~`, …),
  /// unioned with the currently armed styles — the set to keep armed when an
  /// edit committed whitespace outside that cluster. Null when [cluster] is
  /// null or names no emphasis-family styles (e.g. contains a backtick).
  Set<FlarkMarkdownInlineStyle>? _continuationStylesFor(String? cluster) {
    final styles = _stylesForCluster(cluster);
    if (styles == null) return null;
    return {..._pendingInlineStyles, ...styles};
  }

  /// The inline styles a delimiter [cluster] carries, or null when it names
  /// none (e.g. contains a backtick).
  static Set<FlarkMarkdownInlineStyle>? _stylesForCluster(String? cluster) {
    if (cluster == null) return null;
    final styles = <FlarkMarkdownInlineStyle>{};
    var offset = 0;
    while (offset < cluster.length) {
      final markerChar = cluster.codeUnitAt(offset);
      var runLength = 1;
      while (offset + runLength < cluster.length &&
          cluster.codeUnitAt(offset + runLength) == markerChar) {
        runLength += 1;
      }
      switch ((markerChar, runLength)) {
        case (0x2A || 0x5F, 1):
          styles.add(FlarkMarkdownInlineStyle.emphasis);
        case (0x2A || 0x5F, 2):
          styles.add(FlarkMarkdownInlineStyle.strong);
        case (0x2A || 0x5F, 3):
          styles
            ..add(FlarkMarkdownInlineStyle.emphasis)
            ..add(FlarkMarkdownInlineStyle.strong);
        case (0x7E, 1 || 2):
          // GFM styles both `~x~` and `~~x~~`.
          styles.add(FlarkMarkdownInlineStyle.strikethrough);
        default:
          return null;
      }
      offset += runLength;
    }
    return styles;
  }

  /// The open/close pair for a one-character wrap delimiter, or null.
  static ({String open, String close})? _wrapPairFor(String inserted) {
    return switch (inserted) {
      '*' => (open: '*', close: '*'),
      '_' => (open: '_', close: '_'),
      '`' => (open: '`', close: '`'),
      '(' => (open: '(', close: ')'),
      '[' => (open: '[', close: ']'),
      '{' => (open: '{', close: '}'),
      '"' => (open: '"', close: '"'),
      "'" => (open: "'", close: "'"),
      _ => null,
    };
  }

  /// Applies a display-space selection.
  ///
  /// With no explicit [affinity]:
  ///
  /// - A collapsed selection uses caret-placement mapping
  ///   ([FlarkProjection.displayCaretToSource]): a caret at the trailing
  ///   edge of an inline styled run lands inside the run so typing
  ///   continues its style.
  /// - A range selects exactly the visible content: the start maps past
  ///   hidden markers at its boundary (downstream) and the end stops
  ///   before them (upstream), so selecting a styled run's text never
  ///   silently includes a hidden marker on one side only.
  ///
  /// Pass an [affinity] to force plain boundary mapping instead.
  bool applyProjectedSelection(
    FlarkSelection displaySelection, {
    FlarkMapAffinity? affinity,
  }) {
    final FlarkSelection sourceSelection;
    if (affinity == null && displaySelection.isCollapsed) {
      sourceSelection = FlarkSelection.collapsed(
        projection.displayCaretToSource(displaySelection.extentOffset),
      );
    } else if (affinity == null) {
      final start = projection.displayToSourceOffset(
        displaySelection.start,
        affinity: FlarkMapAffinity.downstream,
      );
      final end = projection.displayToSourceOffset(
        displaySelection.end,
        affinity: FlarkMapAffinity.upstream,
      );
      if (start <= end) {
        final inverted =
            displaySelection.baseOffset > displaySelection.extentOffset;
        sourceSelection = inverted
            ? FlarkSelection(baseOffset: end, extentOffset: start)
            : FlarkSelection(baseOffset: start, extentOffset: end);
      } else {
        sourceSelection = projection.displaySelectionToSource(
          displaySelection,
          affinity: FlarkMapAffinity.downstream,
        );
      }
    } else {
      sourceSelection = projection.displaySelectionToSource(
        displaySelection,
        affinity: affinity,
      );
    }
    return applySelection(sourceSelection, userEvent: 'selection.projected');
  }

  bool applySelection(
    FlarkSelection sourceSelection, {
    String userEvent = 'selection',
  }) {
    sourceSelection.validate(state.document.length);
    if (sourceSelection == selection) return false;
    applyTransaction(
      FlarkTransaction(
        operations: const [],
        selectionAfter: sourceSelection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
    return true;
  }

  FlarkEditorRuntimeResult undo() {
    final result = _runtime.undo();
    _adoptRuntimeResult(result, eventKind: FlarkControllerEventKind.undo);
    return result;
  }

  FlarkEditorRuntimeResult redo() {
    final result = _runtime.redo();
    _adoptRuntimeResult(result, eventKind: FlarkControllerEventKind.redo);
    return result;
  }

  bool applyParseResult(FlarkMarkdownParseResult parseResult) {
    if (_disposed) return false;
    if (parseResult.revision != state.revision ||
        parseResult.sourceTextLength != state.document.length) {
      return false;
    }

    // The adopted projection + render plan run through one ordered pipeline of
    // reconciliation passes (extensions, then sticky inline-run rendering); see
    // FlarkRenderReconciler.
    final adoption = FlarkRenderReconciler.fromParseResult(
      parseResult: parseResult,
      source: state.markdown,
      selection: state.selection,
      extensions: _runtime.extensions,
    );
    assert(() {
      _debugConfirmPredictionAdoption(adoption.projection);
      return true;
    }());
    _projection = adoption.projection;
    _renderPlan = adoption.renderPlan;
    _renderPlanRevision = parseResult.revision;
    _lastProjectionPrediction = null;
    _emitEvent(
      kind: FlarkControllerEventKind.parseAdopted,
      previousState: state,
    );
    notifyListeners();
    return true;
  }

  /// RFC 022 §4 adoption-time confirmation (debug builds only; see the
  /// `flarkDebug…` globals above). Two tiers:
  ///
  /// * Authored claims are asserted: an edit that wrote delimiters and pre-hid
  ///   them promised valid markdown, and the parse must re-derive those exact
  ///   hidden ranges. A miss means a placement/wrap scanner authored markdown
  ///   the parser disagrees with — throw so the offending flow's test fails.
  /// * Mapped geometry is counted, not asserted, because the raw typing path
  ///   does not yet declare its non-local blast radius (Phase 1).
  void _debugConfirmPredictionAdoption(FlarkProjection authoritative) {
    if (!flarkDebugValidatePredictionAdoption) return;
    final authoritativeHidden = <(int, int)>{
      for (final hidden in authoritative.hiddenRanges)
        (hidden.range.start, hidden.range.end),
    };

    final authored = _debugAuthoredMarkerRanges;
    _debugAuthoredMarkerRanges = null;
    if (authored != null) {
      for (final range in authored) {
        if (authoritativeHidden.contains((range.start, range.end))) continue;
        final snippet = state.markdown.length <= 200
            ? state.markdown
            : '${state.markdown.substring(0, 200)}…';
        throw StateError(
          'RFC 022 authored-claim violation: the editor authored a delimiter '
          'at [${range.start}, ${range.end}) and pre-hid it, but the parse '
          'did not re-derive that hidden range — a placement/wrap path wrote '
          'markdown the parser disagrees with.\n'
          'markdown: "${snippet.replaceAll('\n', r'\n')}"',
        );
      }
    }

    final prediction = _lastProjectionPrediction;
    if (prediction == null ||
        prediction.touchedProjectionSensitiveRange ||
        prediction.projection.textLength != state.document.length) {
      return;
    }
    final invalidated = prediction.invalidatedRange;
    final authoritativeReplacements = <(int, int)>{
      for (final replacement in authoritative.replacementRanges)
        (replacement.range.start, replacement.range.end),
    };
    for (final hidden in prediction.projection.hiddenRanges) {
      if (invalidated != null && hidden.range.intersects(invalidated)) {
        continue;
      }
      if (!authoritativeHidden.contains((hidden.range.start, hidden.range.end))) {
        flarkDebugUnconfirmedPredictionRanges += 1;
      }
    }
    for (final replacement in prediction.projection.replacementRanges) {
      if (invalidated != null && replacement.range.intersects(invalidated)) {
        continue;
      }
      if (!authoritativeReplacements.contains((
        replacement.range.start,
        replacement.range.end,
      ))) {
        flarkDebugUnconfirmedPredictionRanges += 1;
      }
    }
  }

  void _adoptRuntimeResult(
    FlarkEditorRuntimeResult result, {
    FlarkControllerEventKind? eventKind,
  }) {
    // Every mutator that reaches notifyListeners() routes through here; guard
    // disposal for symmetry with applyParseResult so a synchronous edit on a
    // controller that outlived its widgets never notifies after dispose.
    if (_disposed) return;
    if (identical(result.runtime, _runtime)) return;

    // Any adopted runtime change — a typed run, a selection move, an undo —
    // disarms pending and muted inline styles. Only arming (toggle…InlineStyle)
    // bypasses this chokepoint, so only arming preserves them. The armed run
    // wrap and muted exit read the sets before applying, so clearing is
    // correct. One exception: an edit that committed whitespace outside a
    // run's delimiters (keeping the source valid CommonMark) re-arms that
    // run's styles so the next styled keystroke re-enters the run.
    final continuation = _armedContinuationOverride;
    _armedContinuationOverride = null;
    if (continuation != null) {
      _pendingInlineStyles = continuation;
    } else if (_pendingInlineStyles.isNotEmpty) {
      _pendingInlineStyles = <FlarkMarkdownInlineStyle>{};
    }
    if (_mutedInlineStyles.isNotEmpty) {
      _mutedInlineStyles = <FlarkMarkdownInlineStyle>{};
    }
    final authoredMarkers = _pendingAuthoredMarkers;
    _pendingAuthoredMarkers = null;
    // A new adoption invalidates any earlier authored-claim capture: the
    // ranges below are in *this* document's coordinates, and the RFC 022
    // adoption check must never compare stale coordinates against a later
    // revision's parse.
    assert(() {
      _debugAuthoredMarkerRanges = null;
      return true;
    }());

    final documentTransactions = [
      for (final transaction in result.appliedTransactions)
        if (transaction.changesDocument) transaction,
    ];
    final previousProjection = _projection;
    final previousRenderPlan = _renderPlan;
    final previousState = state;
    _runtime = result.runtime;
    if (result.appliedTransactions.isEmpty) {
      // The runtime changed without telling us how (no applied transactions).
      // There is nothing to map the projection or render plan through, so
      // reset both and let the next parse rebuild them.
      _projection = FlarkProjection(textLength: state.document.length);
      _lastProjectionPrediction = null;
      _renderPlan = _staleRenderPlan(state.revision);
      _renderPlanRevision = null;
    } else if (documentTransactions.isEmpty) {
      _projection = previousProjection;
      _lastProjectionPrediction = null;
    } else if (documentTransactions.length == 1) {
      final transaction = documentTransactions.single;
      final prediction = previousProjection.predictAfter(
        transaction,
        textLengthAfter: state.document.length,
      );
      final structuralPrediction = _predictStructuralRenderPlan(
        markdown: state.markdown,
        revision: state.revision,
        projection: prediction.projection,
        previousRenderPlan: previousRenderPlan,
        transaction: transaction,
      );
      _projection = structuralPrediction?.projection ?? prediction.projection;
      // Delimiters the edit itself authored hide immediately in the predicted
      // projection: the display (and the platform editable it syncs to) never
      // sees them raw, so nothing flashes and an active IME composition
      // survives the keystroke. The immediate parse that follows re-derives
      // the same hidden ranges authoritatively.
      var predictionForAdoption = prediction;
      if (authoredMarkers != null && authoredMarkers.isNotEmpty) {
        _projection = FlarkProjection(
          textLength: state.document.length,
          hiddenRanges: [
            ..._projection.hiddenRanges,
            for (final marker in authoredMarkers)
              FlarkHiddenRange(
                range: marker.range,
                kind: FlarkHiddenRangeKind.inlineMarker,
                opensInlineRun: marker.opens,
                closesInlineRun: !marker.opens,
              ),
          ],
          replacementRanges: _projection.replacementRanges,
          ambiguityZones: _projection.ambiguityZones,
        );
        predictionForAdoption = FlarkProjectionPrediction(
          projection: _projection,
          touchedProjectionSensitiveRange:
              prediction.touchedProjectionSensitiveRange,
          invalidatedRange: prediction.invalidatedRange,
        );
        assert(() {
          _debugAuthoredMarkerRanges = [
            for (final marker in authoredMarkers) marker.range,
          ];
          return true;
        }());
      }
      _lastProjectionPrediction = structuralPrediction == null
          ? predictionForAdoption
          : null;
      _renderPlan =
          structuralPrediction?.renderPlan ??
          _predictRenderPlan(
            previousRenderPlan: previousRenderPlan,
            transaction: transaction,
            projection: _projection,
            revision: state.revision,
            textLengthAfter: state.document.length,
          );
      _renderPlanRevision = null;
    } else {
      // Several transactions applied atomically (a grouped undo/redo entry).
      // Map the projection and render plan through each in order; the
      // intermediate text length steps by each transaction's net delta.
      var projection = previousProjection;
      var renderPlan = previousRenderPlan;
      var textLength = previousState.document.length;
      var touchedSensitiveRange = false;
      FlarkSourceRange? invalidatedRange;
      FlarkProjectionPrediction? prediction;
      for (final transaction in documentTransactions) {
        textLength += _transactionNetDelta(transaction);
        prediction = projection.predictAfter(
          transaction,
          textLengthAfter: textLength,
        );
        touchedSensitiveRange =
            touchedSensitiveRange || prediction.touchedProjectionSensitiveRange;
        final stepInvalidated = prediction.invalidatedRange;
        if (stepInvalidated != null) {
          invalidatedRange =
              invalidatedRange?.union(stepInvalidated) ?? stepInvalidated;
        }
        projection = prediction.projection;
        renderPlan = _predictRenderPlan(
          previousRenderPlan: renderPlan,
          transaction: transaction,
          projection: projection,
          revision: state.revision,
          textLengthAfter: textLength,
        );
      }
      assert(
        textLength == state.document.length,
        'Applied transactions must net to the new document length.',
      );
      _projection = projection;
      // Merge the per-step prediction metadata: a consumer of
      // lastProjectionPrediction must see the union of what the grouped
      // transactions touched, not just the final step's view.
      _lastProjectionPrediction = prediction == null
          ? null
          : FlarkProjectionPrediction(
              projection: projection,
              touchedProjectionSensitiveRange: touchedSensitiveRange,
              invalidatedRange: invalidatedRange,
            );
      _renderPlan = renderPlan;
      _renderPlanRevision = null;
    }
    _emitEvent(
      kind: eventKind ?? _eventKindForRuntimeChange(previousState),
      previousState: previousState,
    );
    notifyListeners();
  }

  static int _transactionNetDelta(FlarkTransaction transaction) {
    var delta = 0;
    for (final operation in transaction.operations) {
      delta += operation.delta;
    }
    return delta;
  }

  FlarkControllerEventKind _eventKindForRuntimeChange(
    FlarkEditorState previousState,
  ) {
    if (previousState.markdown == state.markdown &&
        previousState.selection != state.selection) {
      return FlarkControllerEventKind.selectionChanged;
    }
    if (_lastProjectionPrediction != null) {
      return FlarkControllerEventKind.projectionPredicted;
    }
    return FlarkControllerEventKind.runtimeChanged;
  }

  void _emitEvent({
    required FlarkControllerEventKind kind,
    required FlarkEditorState previousState,
  }) {
    if (_events.isClosed) return;
    _events.add(
      FlarkControllerEvent(
        kind: kind,
        revision: state.revision,
        previousRevision: previousState.revision,
        markdownChanged: previousState.markdown != state.markdown,
        selectionChanged: previousState.selection != state.selection,
      ),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _parseScheduler?.dispose();
    _parseScheduler = null;
    _events.close();
    super.dispose();
  }

  static FlarkRenderPlan _staleRenderPlan(int revision) {
    return FlarkRenderPlan(
      blocks: const [],
      metadata: {'revision': revision},
      fidelity: FlarkRenderPlanFidelity.stale,
    );
  }

  static FlarkRenderPlan _predictRenderPlan({
    required FlarkRenderPlan previousRenderPlan,
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int revision,
    required int textLengthAfter,
  }) {
    if (previousRenderPlan.blocks.isEmpty) return _staleRenderPlan(revision);
    return previousRenderPlan.predictThroughTransaction(
      transaction: transaction,
      projection: projection,
      revision: revision,
      textLengthAfter: textLengthAfter,
    );
  }

  static _PredictedStructuralRenderPlan? _predictStructuralRenderPlan({
    required String markdown,
    required int revision,
    required FlarkProjection projection,
    required FlarkRenderPlan previousRenderPlan,
    required FlarkTransaction transaction,
  }) {
    if (markdown.isEmpty) return null;
    final context = _predictiveCodeFenceContext(
      markdown: markdown,
      transaction: transaction,
    );
    if (context == null || !_canPredictCodeFence(markdown, context)) {
      return null;
    }
    final markerRanges = _predictiveCodeFenceMarkerRanges(markdown, context);
    final structuralProjection = FlarkProjection(
      textLength: markdown.length,
      hiddenRanges: [
        for (final hiddenRange in projection.hiddenRanges)
          if (!_overlapsAny(hiddenRange.range, markerRanges)) hiddenRange,
        for (final markerRange in markerRanges)
          FlarkHiddenRange(
            range: markerRange,
            kind: FlarkHiddenRangeKind.markdownMarker,
          ),
      ],
      replacementRanges: projection.replacementRanges,
      ambiguityZones: projection.ambiguityZones,
    );
    final predictedPreviousRenderPlan = previousRenderPlan
        .predictThroughTransaction(
          transaction: transaction,
          projection: structuralProjection,
          revision: revision,
          textLengthAfter: markdown.length,
        );
    final blockEnd = context.closingLineEnd ?? markdown.length;
    final predictedCodeBlock = FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.codeBlock,
      type: 'codeBlock',
      sourceRange: FlarkSourceRange(context.openingLineStart, blockEnd),
      displayRange: FlarkSourceRange(
        structuralProjection.sourceToDisplayOffset(context.openingLineStart),
        structuralProjection.sourceToDisplayOffset(blockEnd),
      ),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      codeBlock: FlarkRenderCodeBlockDescriptor(language: context.language),
    );
    final predictedBlocks = [
      for (final block in predictedPreviousRenderPlan.blocks)
        if (!_rangesOverlap(block.sourceRange, predictedCodeBlock.sourceRange))
          block,
      predictedCodeBlock,
    ];
    return _PredictedStructuralRenderPlan(
      projection: structuralProjection,
      renderPlan: FlarkRenderPlan(
        blocks: predictedBlocks,
        metadata: {
          ...predictedPreviousRenderPlan.metadata,
          'revision': revision,
        },
        fidelity: FlarkRenderPlanFidelity.predicted,
      ),
    );
  }

  static FlarkMarkdownFencedCodeContext? _predictiveCodeFenceContext({
    required String markdown,
    required FlarkTransaction transaction,
  }) {
    // One fence scan per predicted edit; both probes query the shared layout
    // so the prediction cannot disagree with the policy layer's fence model.
    final layout = FlarkMarkdownFenceLayout.scan(markdown);
    final insertedContext = _insertedCodeFenceContext(
      markdown: markdown,
      transaction: transaction,
      layout: layout,
    );
    if (insertedContext != null) return insertedContext;
    return layout.contextAt(markdown.length);
  }

  static FlarkMarkdownFencedCodeContext? _insertedCodeFenceContext({
    required String markdown,
    required FlarkTransaction transaction,
    required FlarkMarkdownFenceLayout layout,
  }) {
    var delta = 0;
    final operations = [...transaction.operations]
      ..sort((left, right) {
        final startCompare = left.replacedRange.start.compareTo(
          right.replacedRange.start,
        );
        if (startCompare != 0) return startCompare;
        return left.replacedRange.end.compareTo(right.replacedRange.end);
      });

    for (final operation in operations) {
      final insertedStart = (operation.replacedRange.start + delta).clamp(
        0,
        markdown.length,
      );
      final insertedEnd = (insertedStart + operation.insertedLength).clamp(
        insertedStart,
        markdown.length,
      );
      delta += operation.delta;
      final insertedRange = FlarkSourceRange(insertedStart, insertedEnd);
      var lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
        markdown,
        insertedStart,
      );
      while (lineStart <= insertedEnd && lineStart < markdown.length) {
        final context = layout.openerAt(lineStart);
        if (context != null &&
            _rangesOverlap(
              insertedRange,
              FlarkSourceRange(
                context.openingLineStart,
                context.openingLineEndWithBreak,
              ),
            )) {
          return context;
        }

        final next = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
          markdown,
          lineStart,
        );
        if (next <= lineStart || next >= markdown.length) break;
        lineStart = next;
      }
    }

    return null;
  }

  static bool _canPredictCodeFence(
    String markdown,
    FlarkMarkdownFencedCodeContext context,
  ) {
    if (context.openingLineEndWithBreak <= context.openingLineStart ||
        context.bodyStart > markdown.length) {
      return false;
    }
    if (context.isClosed) return context.closingLineEnd != null;
    return context.openingLineEndWithBreak < markdown.length ||
        markdown.endsWith('\n');
  }

  static List<FlarkSourceRange> _predictiveCodeFenceMarkerRanges(
    String markdown,
    FlarkMarkdownFencedCodeContext context,
  ) {
    final ranges = <FlarkSourceRange>[
      FlarkSourceRange(context.openingLineStart, context.bodyStart),
    ];
    final closingLineStart = context.closingLineStart;
    final closingLineEnd = context.closingLineEnd;
    if (closingLineStart != null && closingLineEnd != null) {
      var closingHiddenStart = closingLineStart;
      if (closingHiddenStart > context.bodyStart &&
          _isLineBreakBefore(markdown, closingHiddenStart)) {
        closingHiddenStart -= 1;
      }
      ranges.add(FlarkSourceRange(closingHiddenStart, closingLineEnd));
    }
    return ranges;
  }

  static bool _isLineBreakBefore(String markdown, int offset) {
    if (offset <= 0 || offset > markdown.length) return false;
    final codeUnit = markdown.codeUnitAt(offset - 1);
    return codeUnit == 0x0A || codeUnit == 0x0D;
  }

  static bool _rangesOverlap(FlarkSourceRange left, FlarkSourceRange right) {
    return left.start < right.end && right.start < left.end;
  }

  static bool _overlapsAny(
    FlarkSourceRange range,
    Iterable<FlarkSourceRange> others,
  ) {
    for (final other in others) {
      if (_rangesOverlap(range, other)) return true;
    }
    return false;
  }
}

final class _MutedExitResolution {
  const _MutedExitResolution(
    this.transaction, {
    this.continuationMarker,
    this.authoredMarkers = const [],
  });

  final FlarkTransaction transaction;
  final String? continuationMarker;
  final List<FlarkAuthoredMarker> authoredMarkers;
}

final class _SelectionReplacement {
  const _SelectionReplacement({
    required this.range,
    required this.content,
    required this.inserted,
  });

  /// The source range covered by the selection that was replaced.
  final FlarkSourceRange range;

  /// The selected text (equal to its display, i.e. plain — no hidden markers).
  final String content;

  /// The token the platform inserted in place of the selection.
  final String inserted;
}

final class _PredictedStructuralRenderPlan {
  const _PredictedStructuralRenderPlan({
    required this.projection,
    required this.renderPlan,
  });

  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}

/// A paragraph break: a newline followed by one or more blank lines. A single
/// soft line break stays within one paragraph, so it does not match.
final _paragraphBreakPattern = RegExp(r'\n(?:[ \t]*\n)+');
