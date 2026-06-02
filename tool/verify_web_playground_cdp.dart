import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

Future<void> main(List<String> arguments) async {
  final uri = Uri.parse(
    arguments.isNotEmpty ? arguments.first : 'http://127.0.0.1:6200/',
  );
  final chromePath = Platform.environment['CHROME_PATH'] ??
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  final port = 9330 + Random().nextInt(200);
  final profile = await Directory.systemTemp.createTemp(
    'sovereign-web-cdp-',
  );
  Process? chrome;
  CdpClient? cdp;

  try {
    chrome = await Process.start(
      chromePath,
      [
        '--headless=new',
        '--disable-gpu',
        '--no-first-run',
        '--no-default-browser-check',
        '--window-size=1400,900',
        '--remote-debugging-port=$port',
        '--user-data-dir=${profile.path}',
        uri.toString(),
      ],
      mode: ProcessStartMode.normal,
    );
    chrome.stdout.listen((_) {});
    chrome.stderr.listen((data) => stderr.add(data));

    final target = await _waitForPageTarget(port, uri);
    cdp = await CdpClient.connect(target.webSocketDebuggerUrl);
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    await cdp.send('Input.setIgnoreInputEvents', {'ignore': false});
    await cdp.send('Page.bringToFront');
    await Future<void>.delayed(const Duration(seconds: 5));

    await _click(cdp, 930, 47); // Scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210); // Blank live editor body.
    await cdp.send('Input.insertText', {'text': '```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-triple.png');
    await _pressKey(
      cdp,
      key: 'Backspace',
      code: 'Backspace',
      windowsVirtualKeyCode: 8,
    );
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-empty-fence-backspace.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```fffffff'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-opener.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo\n```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-fence.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {
      'text': '```\n{"name":"Ada","count":2}\n```',
    });
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-auto-json-fence.png');

    await _click(cdp, 710, 224); // Code-fence language control.
    await Future<void>.delayed(const Duration(milliseconds: 500));
    await _screenshot(cdp, '/tmp/sovereign-cdp-language-menu.png');
    await _click(cdp, 710, 548); // Rust option.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _screenshot(cdp, '/tmp/sovereign-cdp-language-rust.png');
    await _click(cdp, 60, 224); // Code body after selector mutation.
    await cdp.send('Input.insertText', {'text': 'bar'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-language-type.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo\n```\nafter'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _click(cdp, 34, 256); // Start of following text.
    await Future<void>.delayed(const Duration(milliseconds: 300));
    await _pressKey(
      cdp,
      key: 'Backspace',
      code: 'Backspace',
      windowsVirtualKeyCode: 8,
    );
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-fence-backspace-from-after.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo\n```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _pressKey(
      cdp,
      key: 'ArrowDown',
      code: 'ArrowDown',
      windowsVirtualKeyCode: 40,
    );
    await Future<void>.delayed(const Duration(milliseconds: 500));
    await cdp.send('Input.insertText', {'text': 'after'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-terminal-fence-down.png');
    await _pressKey(
      cdp,
      key: 'ArrowUp',
      code: 'ArrowUp',
      windowsVirtualKeyCode: 38,
    );
    await Future<void>.delayed(const Duration(milliseconds: 500));
    await cdp.send('Input.insertText', {'text': '!'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-fence-up-from-below.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo\n```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _click(cdp, 64, 224); // Code body, after "foo".
    await Future<void>.delayed(const Duration(milliseconds: 300));
    await _pressKey(
      cdp,
      key: 'ArrowLeft',
      code: 'ArrowLeft',
      windowsVirtualKeyCode: 37,
      modifiers: 8,
    );
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-code-selection.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo bar\n```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _drag(cdp, fromX: 44, fromY: 224, toX: 103, toY: 224);
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-code-drag-selection.png');

    await _click(cdp, 930, 47); // Reset scratch.
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await _click(cdp, 60, 210);
    await cdp.send('Input.insertText', {'text': '```dart\nfoo bar\n```'});
    await Future<void>.delayed(const Duration(seconds: 1));
    await _doubleClick(cdp, 56, 224);
    await Future<void>.delayed(const Duration(seconds: 1));
    await _screenshot(cdp, '/tmp/sovereign-cdp-code-double-selection.png');

    stdout.writeln('wrote /tmp/sovereign-cdp-triple.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-empty-fence-backspace.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-opener.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-fence.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-auto-json-fence.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-language-menu.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-language-rust.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-language-type.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-fence-backspace-from-after.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-terminal-fence-down.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-fence-up-from-below.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-code-selection.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-code-drag-selection.png');
    stdout.writeln('wrote /tmp/sovereign-cdp-code-double-selection.png');
  } finally {
    await cdp?.close();
    if (chrome != null) {
      chrome.kill();
      await chrome.exitCode.timeout(
        const Duration(seconds: 2),
        onTimeout: () => -1,
      );
    }
    try {
      await profile.delete(recursive: true);
    } on FileSystemException {
      // Chrome can leave short-lived profile locks behind after headless exit.
    }
  }
}

Future<void> _click(CdpClient cdp, double x, double y) async {
  await cdp.send('Input.dispatchMouseEvent', {
    'type': 'mousePressed',
    'x': x,
    'y': y,
    'button': 'left',
    'clickCount': 1,
  });
  await cdp.send('Input.dispatchMouseEvent', {
    'type': 'mouseReleased',
    'x': x,
    'y': y,
    'button': 'left',
    'clickCount': 1,
  });
}

Future<void> _doubleClick(CdpClient cdp, double x, double y) async {
  for (var clickCount = 1; clickCount <= 2; clickCount++) {
    await cdp.send('Input.dispatchMouseEvent', {
      'type': 'mousePressed',
      'x': x,
      'y': y,
      'button': 'left',
      'clickCount': clickCount,
    });
    await cdp.send('Input.dispatchMouseEvent', {
      'type': 'mouseReleased',
      'x': x,
      'y': y,
      'button': 'left',
      'clickCount': clickCount,
    });
    await Future<void>.delayed(const Duration(milliseconds: 60));
  }
}

Future<void> _drag(
  CdpClient cdp, {
  required double fromX,
  required double fromY,
  required double toX,
  required double toY,
}) async {
  await cdp.send('Input.dispatchMouseEvent', {
    'type': 'mousePressed',
    'x': fromX,
    'y': fromY,
    'button': 'left',
    'buttons': 1,
    'clickCount': 1,
  });
  await Future<void>.delayed(const Duration(milliseconds: 80));
  await cdp.send('Input.dispatchMouseEvent', {
    'type': 'mouseMoved',
    'x': toX,
    'y': toY,
    'button': 'left',
    'buttons': 1,
  });
  await Future<void>.delayed(const Duration(milliseconds: 80));
  await cdp.send('Input.dispatchMouseEvent', {
    'type': 'mouseReleased',
    'x': toX,
    'y': toY,
    'button': 'left',
    'buttons': 0,
    'clickCount': 1,
  });
}

Future<void> _pressKey(
  CdpClient cdp, {
  required String key,
  required String code,
  required int windowsVirtualKeyCode,
  int modifiers = 0,
}) async {
  final params = {
    'key': key,
    'code': code,
    'windowsVirtualKeyCode': windowsVirtualKeyCode,
    'nativeVirtualKeyCode': windowsVirtualKeyCode,
    'modifiers': modifiers,
  };
  await cdp.send('Input.dispatchKeyEvent', {
    ...params,
    'type': 'rawKeyDown',
  });
  await cdp.send('Input.dispatchKeyEvent', {
    ...params,
    'type': 'keyUp',
  });
}

Future<void> _screenshot(CdpClient cdp, String path) async {
  final result = await cdp.send('Page.captureScreenshot', {
    'format': 'png',
    'fromSurface': true,
  });
  final data = result['data'];
  if (data is! String) {
    throw StateError('Page.captureScreenshot did not return base64 data');
  }
  await File(path).writeAsBytes(base64Decode(data));
}

Future<_Target> _waitForPageTarget(int port, Uri uri) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (DateTime.now().isBefore(deadline)) {
    try {
      final targets = await _jsonList(
        Uri.parse('http://127.0.0.1:$port/json/list'),
      );
      for (final target in targets) {
        if (target
            case {
              'type': 'page',
              'url': final String url,
              'webSocketDebuggerUrl': final String wsUrl,
            }) {
          if (url == uri.toString() || url.startsWith(uri.toString())) {
            return _Target(wsUrl);
          }
        }
      }
    } catch (_) {
      // Chrome may not have opened the debugging endpoint yet.
    }
    await Future<void>.delayed(const Duration(milliseconds: 150));
  }
  throw StateError('Timed out waiting for Chrome DevTools target');
}

Future<List<Object?>> _jsonList(Uri uri) async {
  final client = HttpClient();
  try {
    final request = await client.getUrl(uri);
    final response = await request.close();
    final body = await utf8.decodeStream(response);
    final decoded = jsonDecode(body);
    if (decoded is List<Object?>) return decoded;
    throw StateError('Expected JSON list from $uri');
  } finally {
    client.close(force: true);
  }
}

final class _Target {
  const _Target(this.webSocketDebuggerUrl);

  final String webSocketDebuggerUrl;
}

final class CdpClient {
  CdpClient._(this._socket) {
    _subscription = _socket.listen(_handleMessage);
  }

  final WebSocket _socket;
  late final StreamSubscription<dynamic> _subscription;
  final Map<int, Completer<Map<String, Object?>>> _pending = {};
  var _nextId = 1;

  static Future<CdpClient> connect(String url) async {
    final socket = await WebSocket.connect(url);
    return CdpClient._(socket);
  }

  Future<Map<String, Object?>> send(
    String method, [
    Map<String, Object?> params = const {},
  ]) {
    final id = _nextId++;
    final completer = Completer<Map<String, Object?>>();
    _pending[id] = completer;
    _socket.add(
      jsonEncode({
        'id': id,
        'method': method,
        if (params.isNotEmpty) 'params': params,
      }),
    );
    return completer.future.timeout(
      const Duration(seconds: 10),
      onTimeout: () {
        _pending.remove(id);
        throw TimeoutException('Timed out waiting for $method');
      },
    );
  }

  Future<void> close() async {
    await _subscription.cancel();
    await _socket.close();
  }

  void _handleMessage(dynamic message) {
    if (message is! String) return;
    final decoded = jsonDecode(message);
    if (decoded is! Map<String, Object?>) return;
    final id = decoded['id'];
    if (id is! int) return;
    final completer = _pending.remove(id);
    if (completer == null) return;
    final error = decoded['error'];
    if (error != null) {
      completer.completeError(StateError('$error'));
      return;
    }
    final result = decoded['result'];
    if (result is Map<String, Object?>) {
      completer.complete(result);
    } else {
      completer.complete(const {});
    }
  }
}
