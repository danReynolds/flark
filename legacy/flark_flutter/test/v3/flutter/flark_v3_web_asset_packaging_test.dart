@TestOn('browser')
library;

import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:flutter_test/flutter_test.dart';

extension type _FetchResponse(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSString get statusText;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

void main() {
  test('Flutter bundle serves the external v3 Worker asset', () async {
    final workerUri = Uri.base.resolve(
      '/packages/flark_flutter/assets/worker/flark_v3_parser_worker.js',
    );
    final response =
        await (globalContext
                    .getProperty<JSFunction>('fetch'.toJS)
                    .callAsFunction(globalContext, workerUri.toString().toJS)
                as JSPromise<_FetchResponse>)
            .toDart;
    expect(
      response.ok.toDart,
      isTrue,
      reason:
          'Failed to fetch $workerUri: ${response.status.toDartInt} '
          '${response.statusText.toDart}',
    );

    final buffer = await response.arrayBuffer().toDart;
    final bytes = buffer.toDart.asUint8List();
    expect(bytes, isNotEmpty);
    final source = utf8.decode(bytes);
    expect(source, contains('WebAssembly'));
    expect(source, contains('onmessage'));
  });
}
