import 'dart:async';
import '../kernel/document.dart';
import '../kernel/editor.dart';
import 'backend_loader.dart';
import 'state.dart';

/// Automatic loading for the dedicated read-only path. One current document,
/// no global content cache and no editing session are retained.
final class FlarkReader {
  FlarkReader(
    String markdown, {
    Future<FlarkBackendLease> Function()? backendLoader,
  }) : _markdown = markdown,
       _loader = backendLoader ?? loadFlarkBackend {
    _start();
  }
  final Future<FlarkBackendLease> Function() _loader;
  String _markdown;
  FlarkBackendLease? _lease;
  FlarkReadDocument? document;
  FlarkStatus status = FlarkStatus.loading;
  Object? error;
  final _listeners = <void Function()>[];
  late Future<void> ready;
  Completer<void>? _attempt;
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final listener in List.of(_listeners)) {
      listener();
    }
  }

  void _start() {
    status = FlarkStatus.loading;
    error = null;
    final done = _attempt = Completer<void>();
    ready = done.future;
    unawaited(ready.then<void>((_) {}, onError: (Object _, StackTrace _) {}));
    unawaited(() async {
      FlarkBackendLease? lease;
      try {
        lease = await _loader();
        if (status == FlarkStatus.disposed) {
          lease.dispose();
          if (!done.isCompleted) {
            done.completeError(StateError('Reader disposed'));
          }
          return;
        }
        document = FlarkReadDocument(lease.backend, _markdown);
        _lease = lease;
        status = FlarkStatus.ready;
        _notify();
        if (!done.isCompleted) done.complete();
      } catch (e, stack) {
        lease?.dispose();
        if (status != FlarkStatus.disposed) {
          error = e;
          status = FlarkStatus.failed;
          _notify();
        }
        if (!done.isCompleted) done.completeError(e, stack);
      }
    }());
  }

  void update(String markdown) {
    if (status == FlarkStatus.disposed || markdown == _markdown) return;
    _markdown = markdown;
    if (status == FlarkStatus.ready) {
      try {
        document!.update(markdown);
      } catch (e) {
        error = e;
        status = FlarkStatus.failed;
      }
    }
    _notify();
  }

  void select(FlarkSelection selection) {
    if (status == FlarkStatus.ready && document!.select(selection)) _notify();
  }

  Future<void> retry() {
    if (status == FlarkStatus.failed) {
      _lease?.dispose();
      _lease = null;
      document = null;
      _start();
      _notify();
    }
    return ready;
  }

  void dispose() {
    if (status == FlarkStatus.disposed) return;
    status = FlarkStatus.disposed;
    if (_attempt?.isCompleted == false) {
      _attempt!.completeError(StateError('Reader disposed during loading'));
    }
    _listeners.clear();
    document = null;
    _lease?.dispose();
    _lease = null;
  }
}
