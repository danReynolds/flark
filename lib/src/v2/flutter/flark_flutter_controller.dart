import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../projection/projection.dart';
import '../render_plan/render_plan.dart';
import 'flark_parse_scheduler.dart';
import 'flark_text_delta_adapter.dart';

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
    _ensureScheduler().start();
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
    final scheduler = _ensureScheduler();
    try {
      await scheduler.parseNow();
    } catch (error, stackTrace) {
      _onParseError?.call(error, stackTrace);
    }
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

  static String _exitMarkerFor(FlarkMarkdownInlineStyle style, int? adjacentChar) {
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
        selectionAfter: FlarkSelection.collapsed(range.start + converted.length),
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
      applyTransaction(mutedExit);
      _lastEditRequestsImmediateParse = true;
      return true;
    }

    // An armed insertion wrap turns one keystroke into a `**x**`-style run; flag
    // it so the surface parses immediately and the markers hide right away
    // (otherwise they show raw until the debounced parse, and a backspace in
    // that window cannot expand over the not-yet-recognized markers).
    final insertionWrap = _pendingInsertionWrap();
    final transaction = _projectedTextEditAdapter.transactionFromDisplayEdit(
      currentMarkdown: markdown,
      projection: projection,
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
      sourceSelectionBefore: selection,
      undoGroupId: undoGroupId,
      fallbackInsertionAffinity: fallbackInsertionAffinity,
      insertionWrap: insertionWrap,
    );
    if (transaction == null) return false;
    applyTransaction(transaction);
    // Forming a task marker (`- [`, `- [ `, …) parses immediately so the
    // optimistic-checkbox reconciler renders the checkbox right away, instead
    // of the just-typed bracket flashing literal until the debounced parse.
    _lastEditRequestsImmediateParse =
        insertionWrap != null ||
        (selection.isCollapsed &&
            FlarkOptimisticCheckbox.isFormingCheckboxLine(
              markdown,
              selection.extentOffset,
            ));
    return true;
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
  FlarkTransaction? _wrapSelectionRecognizer(
    _SelectionReplacement replacement,
    int? undoGroupId,
  ) {
    final pair = _wrapPairFor(replacement.inserted);
    if (pair == null) return null;
    final range = replacement.range;
    final wrapped = '${pair.open}${replacement.content}${pair.close}';
    final innerStart = range.start + pair.open.length;
    return FlarkTransaction.single(
      FlarkSourceOperation.replace(replacedRange: range, replacementText: wrapped),
      selectionBefore: selection,
      selectionAfter: FlarkSelection(
        baseOffset: innerStart,
        extentOffset: innerStart + replacement.content.length,
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
      FlarkSourceOperation.replace(replacedRange: range, replacementText: linked),
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
  FlarkTransaction? _mutedExitTransaction({
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

    for (final style in _mutedInlineStyles) {
      final run = FlarkMarkdownCommandQueries.enclosingInlineRun(state, style);
      if (run != null) return _runExitTransaction(run, caret, text, undoGroupId);
    }
    return null;
  }

  FlarkTransaction _runExitTransaction(
    FlarkInlineRunRange run,
    int caret,
    String text,
    int? undoGroupId,
  ) {
    final FlarkSourceRange range;
    final String replacement;
    final int caretAfter;
    if (caret >= run.closeStart) {
      // Trailing edge: step out past the closing marker. A switched-in style
      // (last action wins) wraps the exited text into a sibling run, picking a
      // delimiter that won't merge with this run's closing marker.
      final wrap = _armedExitWrap(
        markdown.substring(run.closeStart, run.closeEnd),
      );
      range = FlarkSourceRange(run.closeEnd, run.closeEnd);
      replacement = wrap == null ? text : '${wrap.open}$text${wrap.close}';
      caretAfter =
          run.closeEnd + (wrap == null ? 0 : wrap.open.length) + text.length;
    } else if (caret <= run.contentStart) {
      // Leading edge: step out before the opening marker.
      final wrap = _armedExitWrap(
        markdown.substring(run.openStart, run.contentStart),
      );
      range = FlarkSourceRange(run.openStart, run.openStart);
      replacement = wrap == null ? text : '${wrap.open}$text${wrap.close}';
      caretAfter =
          run.openStart + (wrap == null ? 0 : wrap.open.length) + text.length;
    } else {
      // Middle: close the run, drop the plain text, reopen the run.
      final marker = markdown.substring(run.closeStart, run.closeEnd);
      range = FlarkSourceRange(caret, caret);
      replacement = '$marker$text$marker';
      caretAfter = caret + marker.length + text.length;
    }
    return FlarkTransaction.single(
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
    );
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
    _projection = adoption.projection;
    _renderPlan = adoption.renderPlan;
    _renderPlanRevision = parseResult.revision;
    _lastProjectionPrediction = null;
    // A typed closing marker is the user speaking markdown source: the run
    // closes, the caret stays after the marker, and continued typing is
    // outside the run. (Re-entry is left-arrow across the trailing edge.)
    _emitEvent(
      kind: FlarkControllerEventKind.parseAdopted,
      previousState: state,
    );
    notifyListeners();
    return true;
  }

  void _adoptRuntimeResult(
    FlarkEditorRuntimeResult result, {
    FlarkControllerEventKind? eventKind,
  }) {
    if (identical(result.runtime, _runtime)) return;

    // Any adopted runtime change — a typed run, a selection move, an undo —
    // disarms pending and muted inline styles. Only arming (toggle…InlineStyle)
    // bypasses this chokepoint, so only arming preserves them. The armed run
    // wrap and muted exit read the sets before applying, so clearing is correct.
    if (_pendingInlineStyles.isNotEmpty) {
      _pendingInlineStyles = <FlarkMarkdownInlineStyle>{};
    }
    if (_mutedInlineStyles.isNotEmpty) {
      _mutedInlineStyles = <FlarkMarkdownInlineStyle>{};
    }

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
      _lastProjectionPrediction = structuralPrediction == null
          ? prediction
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
