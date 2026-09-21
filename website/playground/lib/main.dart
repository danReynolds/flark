import 'dart:js_interop';

import 'package:flark/wasm.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:web/web.dart' as web;

import 'demo.dart';

void notifyParent(String type) {
  web.window.parent?.postMessage(
    {'type': type, 'session': Uri.base.queryParameters['session']}.jsify(),
    web.window.location.origin.toJS,
  );
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    final bytes = await rootBundle.load(
      'packages/flark/lib/assets/wasm/flark_parse.wasm',
    );
    final backend = await WasmParseBackend.fromBytes(
      bytes.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes),
    );
    final brightness = ValueNotifier(
      Uri.base.queryParameters['theme'] == 'dark'
          ? Brightness.dark
          : Brightness.light,
    );
    web.window.onMessage.listen((event) {
      if (event.origin != web.window.location.origin ||
          event.source != web.window.parent) {
        return;
      }
      final data = event.data.dartify();
      if (data is! Map ||
          data['type'] != 'flark-demo-theme' ||
          data['session'] != Uri.base.queryParameters['session']) {
        return;
      }
      if (data['theme'] == 'dark' || data['theme'] == 'light') {
        brightness.value = data['theme'] == 'dark'
            ? Brightness.dark
            : Brightness.light;
      }
    });
    runApp(
      HomepageDemo(
        backend: backend,
        brightness: brightness,
        onOpenLink: (uri) =>
            web.window.open(uri.toString(), '_blank', 'noopener,noreferrer'),
      ),
    );
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => notifyParent('flark-demo-ready'),
    );
  } catch (_) {
    notifyParent('flark-demo-error');
  }
}
