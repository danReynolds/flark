import 'dart:js_interop';

@JS('performance.now')
external double _now();
double nowMicros() => _now() * 1000;
