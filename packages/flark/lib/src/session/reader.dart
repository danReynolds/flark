import 'dart:async';
import '../kernel/document.dart';
import '../kernel/editor.dart';
import '../kernel/notify.dart';
import 'backend_loader.dart';
import 'platform_limits.dart';
import 'state.dart';

/// Automatic loading for the dedicated read-only path. One current document,
/// no global content cache and no editing session are retained.
final class FlarkReader {
  FlarkReader(
    String markdown, {
    Future<FlarkBackendLease> Function()? backendLoader,
    int? syncLimit,
    this.liveLimits = const FlarkLiveLimits(),
  }) : _markdown = markdown,
       _loader = backendLoader ?? loadFlarkBackend,
       syncLimit = syncLimit ?? flarkDefaultLiveBytes {
    _start();
  }
  final Future<FlarkBackendLease> Function() _loader;
  final int syncLimit;
  final FlarkLiveLimits liveLimits;
  String _markdown;
  FlarkBackendLease? _lease;
  FlarkReadDocument? _document;
  FlarkStatus _status = FlarkStatus.loading;
  Object? _error;
  final _listeners = <void Function()>[];
  late Future<void> _ready;
  Completer<void>? _attempt;

  /// The current text's projection, once a parser has loaded. Text the
  /// parser cannot take is still the document: it is in source mode.
  FlarkReadDocument? get document => _document;

  /// Whether a parser is loading, ready or failed to load. A document the
  /// parser refuses does not fail the reader; only a parser that cannot
  /// load, or stops working, does.
  FlarkStatus get status => _status;

  /// Why the reader failed, while [status] is [FlarkStatus.failed].
  Object? get error => _error;

  /// The current loading attempt; [retry] starts a new one.
  Future<void> get ready => _ready;

  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() => notifyEach(_listeners);

  void _start() {
    _status = FlarkStatus.loading;
    _error = null;
    final done = _attempt = Completer<void>();
    _ready = done.future;
    unawaited(_ready.then<void>((_) {}, onError: (Object _, StackTrace _) {}));
    unawaited(() async {
      FlarkBackendLease? lease;
      try {
        lease = await _loader();
        if (_status == FlarkStatus.disposed) {
          lease.dispose();
          if (!done.isCompleted) {
            done.completeError(StateError('Reader disposed'));
          }
          return;
        }
        // Held before the first parse, so that dispose() releases the lease
        // however that parse ends: a shared parser must not be held for the
        // life of the page.
        _lease = lease;
        _document = FlarkReadDocument(
          lease.backend,
          _markdown,
          syncLimit: syncLimit,
          liveLimits: liveLimits,
        );
        _status = FlarkStatus.ready;
        _notify();
        if (!done.isCompleted) done.complete();
      } catch (e, stack) {
        if (identical(_lease, lease)) _lease = null;
        lease?.dispose();
        if (_status != FlarkStatus.disposed) {
          _error = e;
          _status = FlarkStatus.failed;
          _notify();
        }
        if (!done.isCompleted) done.completeError(e, stack);
      }
    }());
  }

  void update(String markdown) {
    if (_status == FlarkStatus.disposed || markdown == _markdown) return;
    _markdown = markdown;
    if (_status == FlarkStatus.ready) {
      // Refused text becomes a source snapshot, so only a parser that stopped
      // working throws here; retry() replaces it.
      try {
        _document!.update(markdown);
      } catch (e) {
        _error = e;
        _status = FlarkStatus.failed;
      }
    }
    _notify();
  }

  void select(FlarkSelection selection) {
    if (_status == FlarkStatus.ready && _document!.select(selection)) {
      _notify();
    }
  }

  Future<void> retry() {
    if (_status == FlarkStatus.failed) {
      _lease?.dispose();
      _lease = null;
      _document = null;
      _start();
      _notify();
    }
    return _ready;
  }

  void dispose() {
    if (_status == FlarkStatus.disposed) return;
    _status = FlarkStatus.disposed;
    if (_attempt?.isCompleted == false) {
      _attempt!.completeError(StateError('Reader disposed during loading'));
    }
    _listeners.clear();
    _document = null;
    _lease?.dispose();
    _lease = null;
  }
}

/// The opening of [markdown] that a reader's source fallback shows, about
/// 1,024 code units and an ellipsis when there is more. The fallback is
/// also where text with an unpaired surrogate ends up, which a host's text
/// layout may refuse (Flutter's) or drop (Fleury's), so the cut never splits
/// a surrogate pair and an unpaired one shows as U+FFFD. Copying still takes
/// the source exactly as written.
String flarkSourcePreview(String markdown) {
  const limit = 1024;
  final preview = StringBuffer();
  var i = 0;
  while (i < markdown.length && i < limit) {
    final unit = markdown.codeUnitAt(i);
    final next = i + 1 < markdown.length ? markdown.codeUnitAt(i + 1) : 0;
    if (unit >= 0xD800 && unit <= 0xDBFF && next >= 0xDC00 && next <= 0xDFFF) {
      preview.writeCharCode(0x10000 + ((unit - 0xD800) << 10) + next - 0xDC00);
      i += 2;
    } else {
      preview.writeCharCode(unit >= 0xD800 && unit <= 0xDFFF ? 0xFFFD : unit);
      i++;
    }
  }
  if (i < markdown.length) preview.write('…');
  return preview.toString();
}
