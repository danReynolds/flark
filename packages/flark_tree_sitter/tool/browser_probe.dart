import 'dart:convert';
import 'dart:js_interop';

import 'worker_probe.dart' as probe;

@JS('navigator.userAgent')
external String get _userAgent;
@JS('fetch')
external JSPromise<JSObject> _fetch(String uri, JSObject options);
extension type _Response(JSObject _) implements JSObject {
  external bool get ok;
}

@JS('document.querySelector')
external _Element _element(String selector);
extension type _Element(JSObject _) implements JSObject {
  external set textContent(String value);
}

Future<void> main() async {
  try {
    final runtime = Uri.base.queryParameters['runtime'] == 'wasm'
        ? 'dart-wasm'
        : 'js';
    final result = {
      'runtime': runtime,
      'userAgent': _userAgent,
      ...await probe.run(),
    };
    final json = jsonEncode(result);
    _element('#result').textContent = json;
    final response = _Response(
      await _fetch(
        '/receipt',
        {
              'method': 'POST',
              'headers': {'Content-Type': 'application/json'},
              'body': json,
            }.jsify()!
            as JSObject,
      ).toDart,
    );
    if (!response.ok) throw StateError('Could not save browser receipt');
    _element('#status').textContent = 'Passed (receipt saved)';
  } catch (error, stack) {
    _element('#result').textContent = '$error\n$stack';
    _element('#status').textContent = 'Failed';
  }
}
