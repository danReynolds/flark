@JS()
library;

import 'dart:async';
import 'dart:collection';
import 'dart:js_interop';

@JS('MessageChannel')
extension type _BrowserMessageChannel._(JSObject _) implements JSObject {
  external factory _BrowserMessageChannel();

  external _BrowserMessagePort get port1;
  external _BrowserMessagePort get port2;
}

extension type _BrowserMessagePort._(JSObject _) implements JSObject {
  external JSFunction? get onmessage;
  external set onmessage(JSFunction? value);
  external void postMessage(JSAny? message);
}

final _WebEventTaskQueue _eventTasks = _WebEventTaskQueue();

/// Schedules an unclamped browser task while preserving the caller's zone.
///
/// Zero-duration timers are progressively clamped by browsers. A persistent
/// document can legitimately need many bounded retirement turns, so timer
/// clamping would turn a few milliseconds of reclamation into seconds without
/// improving the per-turn jank bound. MessageChannel provides real event-loop
/// tasks and retains the executor's independent action and elapsed-time caps.
void scheduleEventTask(void Function() callback) {
  _eventTasks.schedule(callback);
}

final class _WebEventTaskQueue {
  _WebEventTaskQueue() {
    _messageHandler = ((JSAny? _) {
      final callback = _pending.removeFirst();
      callback();
    }).toJS;
    _channel.port1.onmessage = _messageHandler;
  }

  final _BrowserMessageChannel _channel = _BrowserMessageChannel();
  final Queue<void Function()> _pending = Queue<void Function()>();
  late final JSFunction _messageHandler;

  void schedule(void Function() callback) {
    _pending.addLast(Zone.current.bindCallback(callback));
    _channel.port2.postMessage(null);
  }
}
