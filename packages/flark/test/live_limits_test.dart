import 'dart:convert';
import 'package:flark/session.dart';
import 'package:flark/src/session/browser_limits.dart' show browserLiveBytes;
import 'package:test/test.dart';

/// About 20 KiB of prose: above the 16 KiB phone default, below the 32 KiB
/// desktop one.
final _prose = List.filled(
  40,
  'A paragraph of ordinary prose that wraps across several lines. ' * 8,
).join('\n\n');

void main() {
  test('the default fits the platform: 32 KiB on a desktop VM', () {
    expect(utf8.encode(_prose).length, inInclusiveRange(16 * 1024, 32 * 1024));
    expect(flarkDefaultLiveBytes, 32 * 1024);
  });

  test('browsers: 32 KiB on desktop, 16 KiB on phones and tablets', () {
    const desktop = {
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 '
              '(KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36':
          0,
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 '
              '(KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36 Edg/153.0.0.0':
          10,
      'Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 '
              '(KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36':
          10,
      'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 '
              'Firefox/140.0':
          0,
    };
    const mobile = {
      'Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) '
              'AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 '
              'Mobile/15E148 Safari/604.1':
          5,
      // iPadOS Safari requests the desktop site as a Mac with touch points.
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 '
              '(KHTML, like Gecko) Version/18.6 Safari/605.1.15':
          5,
      'Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like '
              'Gecko) Chrome/153.0.0.0 Mobile Safari/537.36':
          5,
      'Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 (KHTML, '
              'like Gecko) Chrome/153.0.0.0 Safari/537.36':
          10,
    };
    for (final MapEntry(key: agent, value: touch) in desktop.entries) {
      expect(browserLiveBytes(agent, touch), 32 * 1024, reason: agent);
    }
    for (final MapEntry(key: agent, value: touch) in mobile.entries) {
      expect(browserLiveBytes(agent, touch), 16 * 1024, reason: agent);
    }
  });

  test(
    'a session renders live by default and honors an app override',
    () async {
      final session = FlarkSession(markdown: _prose);
      addTearDown(session.dispose);
      await session.ready;
      expect(session.syncLimit, flarkDefaultLiveBytes);
      expect(session.state.mode, FlarkMode.rendered);

      final small = FlarkSession(markdown: _prose, syncLimit: 16 * 1024);
      addTearDown(small.dispose);
      await small.ready;
      expect(small.state.mode, FlarkMode.source);

      final shape = FlarkSession(
        markdown: _prose,
        liveLimits: const FlarkLiveLimits(blocks: 8),
      );
      addTearDown(shape.dispose);
      await shape.ready;
      expect(shape.state.mode, FlarkMode.source);
    },
  );

  test('the read-only path uses the same default and overrides', () async {
    final reader = FlarkReader(_prose);
    addTearDown(reader.dispose);
    await reader.ready;
    expect(reader.document!.sourceMode, isFalse);

    final small = FlarkReader(_prose, syncLimit: 16 * 1024);
    addTearDown(small.dispose);
    await small.ready;
    expect(small.document!.sourceMode, isTrue);
  });
}
