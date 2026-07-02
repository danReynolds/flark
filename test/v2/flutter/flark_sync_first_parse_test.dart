import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

/// The synchronous first-parse path: a sync-capable backend lets a surface
/// hold an authoritative render plan before its first frame paints, so a
/// read-only preview never flashes raw markdown source while an async parse
/// round-trips.
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

  test('attachParsingSurface parses synchronously before the first frame', () {
    final backend = _SyncParseBackend();
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title',
      parseBackend: backend,
    );
    addTearDown(controller.dispose);

    // The widget path: initState attaches, build runs later in the same
    // frame. The plan must already be authoritative here — no async gap.
    controller.attachParsingSurface();
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
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
