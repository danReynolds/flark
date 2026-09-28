import 'dart:js_interop';
import 'browser_limits.dart';

/// UTF-8 bytes rendered live by default in this browser: 32 KiB on desktop,
/// 16 KiB on phones and tablets.
int get flarkDefaultLiveBytes => _bytes;

final int _bytes = browserLiveBytes(
  _navigator.userAgent,
  _navigator.maxTouchPoints?.toDartInt ?? 0,
);

@JS('navigator')
external _Navigator get _navigator;

extension type _Navigator._(JSObject _) implements JSObject {
  external String get userAgent;
  external JSNumber? get maxTouchPoints;
}
