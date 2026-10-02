import 'dart:async';

/// Call [listeners] in order, over a copy so that a listener may add or
/// remove listeners.
///
/// A listener that throws does not stop the rest. The change it was told
/// about is already published and cannot be withdrawn, so its error goes to
/// the zone of the code that made the change, and every other listener
/// still hears of it.
void notifyEach(List<void Function()> listeners) {
  for (final listener in List.of(listeners)) {
    try {
      listener();
    } catch (error, stackTrace) {
      Zone.current.handleUncaughtError(error, stackTrace);
    }
  }
}
