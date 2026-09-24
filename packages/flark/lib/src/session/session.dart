import 'dart:async';
import 'dart:convert';
import '../kernel/commands.dart';
import '../kernel/editor.dart';
import '../parse/backend.dart';
import 'backend_loader.dart';
import 'platform_limits.dart';
import 'state.dart';

/// Host-independent ownership, readiness and publication. UI adapters own only
/// their input/focus integration; this object owns its parser and editing state.
final class FlarkSession {
  /// [syncLimit] is the UTF-8 size rendered live, [flarkDefaultLiveBytes]
  /// unless set; [liveLimits] bounds the document's shape. A document beyond
  /// either opens and edits in source mode.
  FlarkSession({
    String markdown = '',
    Future<FlarkBackendLease> Function()? backendLoader,
    int? syncLimit,
    this.liveLimits = const FlarkLiveLimits(),
  }) : _markdown = markdown,
       _loader = backendLoader ?? loadFlarkBackend,
       syncLimit = syncLimit ?? flarkDefaultLiveBytes {
    validateFlarkSourceText(markdown);
    if (utf8.encode(markdown).length > 1024 * 1024) {
      throw ArgumentError('document exceeds writable source limit');
    }
    _publish();
    _start();
  }
  final Future<FlarkBackendLease> Function() _loader;
  final int syncLimit;
  final FlarkLiveLimits liveLimits;
  final _listeners = <void Function()>[];
  final _changes = StreamController<String>.broadcast(sync: true);
  FlarkBackendLease? _lease;
  FlarkEditor? _editor;
  FlarkEditor? get engine => _editor;
  String _markdown;
  int _revision = 0;
  FlarkStatus _status = FlarkStatus.loading;
  Object? _error;
  bool _loadingDocument = false;
  int _publicationDepth = 0;
  bool _sendingChanges = false;
  final _pendingChanges = <String>[];
  late FlarkState _state;
  Completer<void>? _attempt;
  Object? _attachment;

  /// The host's IME-aware command path. Null for headless sessions.
  bool Function(FlarkCommand, int)? commandHandler;
  Future<FlarkEditResult> Function(bool image)? resourcePresenter;

  FlarkState get state => _state;
  Stream<String> get changes => _changes.stream;
  Future<void> get ready => _attempt!.future;
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);

  void attach(Object owner) {
    if (_status == FlarkStatus.disposed) {
      throw StateError('Controller disposed');
    }
    if (_attachment != null && !identical(_attachment, owner)) {
      throw StateError('A FlarkController supports one attached editor.');
    }
    _attachment = owner;
  }

  void detach(Object owner) {
    if (!identical(_attachment, owner)) return;
    _attachment = null;
    resourcePresenter = null;
  }

  void _publish() {
    _state = FlarkState(
      markdown: _markdown,
      revision: _revision,
      status: _status,
      error: _error,
      editor: _status == FlarkStatus.ready ? _editor : null,
    );
    _publicationDepth++;
    try {
      for (final listener in List.of(_listeners)) {
        listener();
      }
    } finally {
      _publicationDepth--;
      if (_publicationDepth == 0 && !_sendingChanges) {
        _sendingChanges = true;
        try {
          while (_pendingChanges.isNotEmpty && !_changes.isClosed) {
            _changes.add(_pendingChanges.removeAt(0));
          }
        } finally {
          _sendingChanges = false;
        }
      }
    }
  }

  void _edited() {
    final before = _markdown;
    _markdown = _editor!.source;
    _revision++;
    if (!_loadingDocument && before != _markdown) {
      _pendingChanges.add(_markdown);
    }
    // Nested application edits triggered by a load notification are edits.
    _loadingDocument = false;
    _publish();
  }

  void _start() {
    final attempt = _attempt = Completer<void>();
    // Widget-only consumers are not required to await readiness to observe errors.
    unawaited(
      attempt.future.then<void>((_) {}, onError: (Object _, StackTrace _) {}),
    );
    _status = FlarkStatus.loading;
    _error = null;
    _publish();
    unawaited(() async {
      FlarkBackendLease? lease;
      try {
        lease = await _loader();
        if (_status == FlarkStatus.disposed || !identical(_attempt, attempt)) {
          lease.dispose();
          return;
        }
        final editor = FlarkEditor(
          lease.backend,
          text: _markdown,
          syncLimit: syncLimit,
          liveLimits: liveLimits,
        );
        _lease = lease;
        _editor = editor..addListener(_edited);
        _status = FlarkStatus.ready;
        _publish();
        if (!attempt.isCompleted) attempt.complete();
      } catch (error, stack) {
        lease?.dispose();
        if (_status == FlarkStatus.disposed || !identical(_attempt, attempt)) {
          return;
        }
        _error = error;
        _status = FlarkStatus.failed;
        _publish();
        if (!attempt.isCompleted) attempt.completeError(error, stack);
      }
    }());
  }

  Future<void> retryLoading() {
    if (_status == FlarkStatus.failed) _start();
    return ready;
  }

  FlarkEditResult? _guard(int? expectedRevision, {bool allowLoading = false}) {
    if (_status == FlarkStatus.disposed) {
      return const FlarkEditResult.rejected(FlarkEditRejection.disposed);
    }
    if (expectedRevision != null && expectedRevision != _revision) {
      return const FlarkEditResult.rejected(FlarkEditRejection.staleRevision);
    }
    if (!allowLoading && _status != FlarkStatus.ready) {
      return const FlarkEditResult.rejected(FlarkEditRejection.notReady);
    }
    return null;
  }

  FlarkEditResult _result(bool changed) {
    if (changed) return const FlarkEditResult.changed();
    final reason = _editor!.lastRejection;
    return reason == null
        ? const FlarkEditResult.unchanged()
        : FlarkEditResult.rejected(switch (reason) {
            FlarkRejection.staleRevision => FlarkEditRejection.staleRevision,
            FlarkRejection.unsupportedEdit =>
              FlarkEditRejection.unsupportedEdit,
            FlarkRejection.invalidSource => FlarkEditRejection.invalidSource,
            FlarkRejection.sourceLimit => FlarkEditRejection.sourceLimit,
            FlarkRejection.extractionDeviation =>
              FlarkEditRejection.extractionDeviation,
          });
  }

  FlarkEditResult command(FlarkCommand command, {int? expectedRevision}) {
    final rejected = _guard(expectedRevision);
    if (rejected != null) return rejected;
    if (command is Undo && !_state.canUndo ||
        command is Redo && !_state.canRedo) {
      return const FlarkEditResult.unchanged();
    }
    final handler = commandHandler;
    return _result(
      handler == null
          ? _editor!.applyAfterComposition(command)
          : handler(command, _editor!.revision),
    );
  }

  FlarkEditResult replaceSourceRange(
    int start,
    int end,
    String replacement, {
    int? expectedRevision,
    bool replaceAll = false,
  }) {
    final rejected = _guard(expectedRevision);
    if (rejected != null) return rejected;
    return _result(
      _editor!.replaceSourceRange(
        start,
        end,
        replacement,
        replaceAll: replaceAll,
      ),
    );
  }

  FlarkEditResult loadMarkdown(String markdown, {int? expectedRevision}) {
    final rejected = _guard(expectedRevision, allowLoading: true);
    if (rejected != null) return rejected;
    try {
      validateFlarkSourceText(markdown);
    } on FormatException {
      return const FlarkEditResult.rejected(FlarkEditRejection.invalidSource);
    }
    if (utf8.encode(markdown).length > 1024 * 1024) {
      return const FlarkEditResult.rejected(FlarkEditRejection.sourceLimit);
    }
    if (_status != FlarkStatus.ready) {
      _markdown = markdown;
      _revision++;
      _publish();
      return const FlarkEditResult.changed();
    }
    _loadingDocument = true;
    try {
      return _result(_editor!.loadMarkdown(markdown));
    } finally {
      _loadingDocument = false;
    }
  }

  FlarkEditResult updateImage(
    String destination, {
    String? alt,
    String? title,
    int? expectedRevision,
  }) {
    final rejected = _guard(expectedRevision);
    if (rejected != null) return rejected;
    if (_editor!.sourceMode ||
        _editor!.document.resourceAt(_editor!.selection, image: true) == null) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unsupportedEdit);
    }
    return command(
      SetImage(destination, alt: alt, title: title),
      expectedRevision: expectedRevision,
    );
  }

  FlarkEditResult selectAll({
    FlarkSelectionScope scope = FlarkSelectionScope.document,
    int? expectedRevision,
  }) {
    final rejected = _guard(expectedRevision);
    if (rejected != null) return rejected;
    return _result(
      _editor!.selectAll(codeBlock: scope == FlarkSelectionScope.codeBlock),
    );
  }

  FlarkEditResult setSourceMode(bool enabled, {int? expectedRevision}) {
    final rejected = _guard(expectedRevision);
    if (rejected != null) return rejected;
    if (enabled == _editor!.sourceMode) {
      return const FlarkEditResult.unchanged();
    }
    _editor!.setSourceMode(enabled);
    return const FlarkEditResult.changed();
  }

  Future<FlarkEditResult> showResourceEditor(bool image) async {
    final rejected = _guard(null);
    if (rejected != null) return rejected;
    final presenter = resourcePresenter;
    if (_attachment == null || presenter == null) {
      return const FlarkEditResult.rejected(FlarkEditRejection.unavailable);
    }
    return presenter(image);
  }

  void dispose() {
    if (_status == FlarkStatus.disposed) return;
    _status = FlarkStatus.disposed;
    _editor?.removeListener(_edited);
    _editor = null;
    _lease?.dispose();
    _lease = null;
    _attachment = null;
    _pendingChanges.clear();
    commandHandler = null;
    resourcePresenter = null;
    if (_attempt?.isCompleted == false) {
      _attempt!.completeError(
        StateError('FlarkController disposed during loading'),
      );
    }
    _publish();
    _listeners.clear();
    unawaited(_changes.close());
  }
}

/// Shared callable API for Flutter and Fleury controllers.
mixin FlarkActions {
  FlarkSession get session;
  FlarkState get state => session.state;
  String get markdown => state.markdown;
  Future<void> get ready => session.ready;
  Stream<String> get changes => session.changes;
  Future<void> retryLoading() => session.retryLoading();
  FlarkEditResult toggleStyle(FlarkStyle style, {int? expectedRevision}) =>
      session.command(
        ToggleStyle(style.kernelStyle),
        expectedRevision: expectedRevision,
      );
  FlarkEditResult setStyle(
    FlarkStyle style, {
    required bool enabled,
    int? expectedRevision,
  }) => session.command(
    SetStyle(style.kernelStyle, enabled: enabled),
    expectedRevision: expectedRevision,
  );
  FlarkEditResult insertText(String text, {int? expectedRevision}) =>
      session.command(InsertText(text), expectedRevision: expectedRevision);
  FlarkEditResult replaceSourceRange({
    required int start,
    required int end,
    required String markdown,
    int? expectedRevision,
  }) => session.replaceSourceRange(
    start,
    end,
    markdown,
    expectedRevision: expectedRevision,
  );
  FlarkEditResult replaceMarkdown(String markdown, {int? expectedRevision}) =>
      session.replaceSourceRange(
        0,
        this.markdown.length,
        markdown,
        replaceAll: true,
        expectedRevision: expectedRevision,
      );
  FlarkEditResult loadMarkdown(String markdown, {int? expectedRevision}) =>
      session.loadMarkdown(markdown, expectedRevision: expectedRevision);
  FlarkEditResult setSelection(int base, int extent, {int? expectedRevision}) =>
      session.command(
        SetSelection(base, extent),
        expectedRevision: expectedRevision,
      );
  FlarkEditResult selectAll({
    FlarkSelectionScope scope = FlarkSelectionScope.document,
    int? expectedRevision,
  }) => session.selectAll(scope: scope, expectedRevision: expectedRevision);
  FlarkEditResult setParagraph({int? expectedRevision}) =>
      setHeading(0, expectedRevision: expectedRevision);
  FlarkEditResult setHeading(int level, {int? expectedRevision}) => session
      .command(SetHeadingLevel(level), expectedRevision: expectedRevision);
  FlarkEditResult setCodeLanguage(String language, {int? expectedRevision}) =>
      session.command(
        SetCodeLanguage(language),
        expectedRevision: expectedRevision,
      );
  FlarkEditResult indent({int? expectedRevision}) =>
      session.command(const Indent(), expectedRevision: expectedRevision);
  FlarkEditResult outdent({int? expectedRevision}) =>
      session.command(const Outdent(), expectedRevision: expectedRevision);
  FlarkEditResult toggleTask({int? expectedRevision}) =>
      session.command(const ToggleTask(), expectedRevision: expectedRevision);
  FlarkEditResult setLink(
    String destination, {
    String? text,
    String? title,
    int? expectedRevision,
  }) => session.command(
    SetLink(destination, text: text, title: title),
    expectedRevision: expectedRevision,
  );
  FlarkEditResult removeLink({int? expectedRevision}) =>
      session.command(const RemoveLink(), expectedRevision: expectedRevision);
  FlarkEditResult insertImage(
    String destination, {
    String? alt,
    String? title,
    int? expectedRevision,
  }) => session.command(
    SetImage(destination, alt: alt, title: title),
    expectedRevision: expectedRevision,
  );
  FlarkEditResult updateImage(
    String destination, {
    String? alt,
    String? title,
    int? expectedRevision,
  }) => session.updateImage(
    destination,
    alt: alt,
    title: title,
    expectedRevision: expectedRevision,
  );
  FlarkEditResult removeImage({int? expectedRevision}) =>
      session.command(const RemoveImage(), expectedRevision: expectedRevision);
  FlarkEditResult undo({int? expectedRevision}) =>
      session.command(const Undo(), expectedRevision: expectedRevision);
  FlarkEditResult redo({int? expectedRevision}) =>
      session.command(const Redo(), expectedRevision: expectedRevision);
  FlarkEditResult setSourceMode(bool enabled, {int? expectedRevision}) =>
      session.setSourceMode(enabled, expectedRevision: expectedRevision);
  Future<FlarkEditResult> showLinkEditor() => session.showResourceEditor(false);
  Future<FlarkEditResult> showImageEditor() => session.showResourceEditor(true);
}
