import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';

/// The synchronous first-parse path: `tryParseSync` lets a caller hold an
/// authoritative render plan before the first frame paints, so a standalone
/// read-only preview never flashes raw markdown source while an async parse
/// round-trips. The sync parse is deliberately NOT part of the scheduler's
/// `start()`/attach path — a shared controller may already have built widgets
/// listening, and a synchronous notify there would mark them dirty mid-build.
void main() {
  test('tryParseSync adopts an authoritative plan synchronously', () {
    final backend = _SyncParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title\n\nBody.',
      parseBackend: backend,
    );
    addTearDown(controller.dispose);

    expect(controller.hasAuthoritativeRenderPlan, isFalse);
    expect(controller.tryParseSync(), isTrue);
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
    expect(backend.syncRequests, hasLength(1));
    expect(backend.asyncRequests, isEmpty);
  });

  test('tryParseSync returns false for an async-only backend', () {
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: _AsyncOnlyParseBackend(),
    );
    addTearDown(controller.dispose);

    expect(controller.tryParseSync(), isFalse);
    expect(controller.hasAuthoritativeRenderPlan, isFalse);
  });

  test('attachParsingSurface never notifies synchronously', () {
    // A shared controller may already have built widgets listening when a
    // surface attaches mid-build; the first parse must stay deferred even
    // when the backend could parse synchronously.
    final backend = _SyncParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: backend,
    );
    addTearDown(controller.dispose);

    var notified = false;
    controller.addListener(() => notified = true);
    controller.attachParsingSurface();
    expect(notified, isFalse);
    expect(controller.hasAuthoritativeRenderPlan, isFalse);
    expect(backend.syncRequests, isEmpty);
  });

  test('a successful tryParseSync drops the queued parse for the revision', () {
    final backend = _SyncParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: backend,
    );
    addTearDown(controller.dispose);

    // Arm the scheduled path first (attach queues an immediate async parse),
    // then parse synchronously; the queued parse must not re-run.
    controller.attachParsingSurface();
    expect(controller.tryParseSync(), isTrue);
    return Future<void>.delayed(Duration.zero, () async {
      await controller.parseNow();
      expect(backend.asyncRequests, isEmpty);
      expect(backend.syncRequests, hasLength(1));
    });
  });

  test(
    'a declined sync parse falls back to the scheduled async parse',
    () async {
      final backend = _SyncParseBackend(declineSync: true);
      final controller = FlarkFlutterController.fromMarkdown(
        '# Title',
        parseBackend: backend,
      );
      addTearDown(controller.dispose);

      controller.attachParsingSurface();
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      await controller.parseNow();
      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(backend.asyncRequests, isNotEmpty);
    },
  );

  test('parseSync declines large documents before encoding', () {
    final bridge = _RecordingSyncBridge();
    final backend = FlarkNativeComrakParseBackend(bridge: bridge);
    final result = backend.parseSync(
      FlarkMarkdownParseRequest(
        revision: 1,
        markdown: 'a' * flarkNativeParseIsolateThresholdBytes,
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );
    expect(result, isNull);
    expect(
      bridge.syncCalls,
      0,
      reason: 'the size gate must decline before reaching the bridge',
    );
  });

  testWidgets(
    'a standalone preview renders from a sync parse on its first frame',
    (tester) async {
      final backend = _SyncParseBackend();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdown(markdown: '# Title', parseBackend: backend),
        ),
      );
      // The parse happened during initState — before this first frame — so
      // the preview never painted raw source.
      expect(backend.syncRequests, hasLength(1));
    },
  );
}

FlarkMarkdownParseResult _headingResult(FlarkMarkdownParseRequest request) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: request.revision,
    sourceTextLength: request.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.heading,
        type: 'heading',
        sourceRange: FlarkSourceRange(0, request.markdown.length),
        attributes: const {'level': 1},
      ),
    ],
    inlineTokens: const [],
  );
}

final class _SyncParseBackend implements FlarkSyncCapableParseBackend {
  _SyncParseBackend({this.declineSync = false});

  /// Simulates a document too large for the calling isolate.
  final bool declineSync;
  final syncRequests = <FlarkMarkdownParseRequest>[];
  final asyncRequests = <FlarkMarkdownParseRequest>[];

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'sync-test',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  FlarkMarkdownParseResult? parseSync(FlarkMarkdownParseRequest request) {
    if (declineSync) return null;
    syncRequests.add(request);
    return _headingResult(request);
  }

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    asyncRequests.add(request);
    return _headingResult(request);
  }
}

final class _AsyncOnlyParseBackend implements FlarkMarkdownParseBackend {
  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'async-only-test',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    return _headingResult(request);
  }
}

/// A bridge that records sync attempts, to prove the backend's size gate
/// declines before ever reaching the bridge.
final class _RecordingSyncBridge implements SyncCapableNativeComrakBridge {
  var syncCalls = 0;

  @override
  NativeComrakParseResult? parseSyncBelowThreshold(
    NativeComrakParseInput input,
  ) {
    syncCalls += 1;
    return null;
  }

  @override
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input) async {
    return NativeComrakParseResult(revision: input.revision);
  }
}
