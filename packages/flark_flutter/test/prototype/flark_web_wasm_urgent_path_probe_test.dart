@Tags(<String>['benchmark'])
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter.dart';

import '../v2/support/flark_test_backend.dart';

void main() {
  test('profile the warmed web WASM path for bounded active shards', () async {
    final backend = flarkTestNativeBackend();
    await backend.parse(
      const FlarkMarkdownParseRequest(
        revision: 0,
        markdown: '**warm**',
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );

    Future<void> measure({
      required String workload,
      required String markdown,
      required int warmups,
      required int measured,
    }) async {
      final totalSamples = <int>[];
      final encodeSamples = <int>[];
      final bridgeSamples = <int>[];
      final mappingSamples = <int>[];
      var inlineTokens = 0;
      for (var iteration = 0; iteration < warmups + measured; iteration++) {
        final profiled = await backend.parseWithProfile(
          FlarkMarkdownParseRequest(
            revision: iteration + 1,
            markdown: markdown,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        );
        final result = profiled.result;
        final profile = profiled.profile;
        expect(result.revision, iteration + 1);
        expect(
          result.diagnostics.where(
            (diagnostic) => diagnostic.extensions['isError'] == true,
          ),
          isEmpty,
        );
        if (iteration >= warmups) {
          totalSamples.add(profile.total.inMicroseconds);
          encodeSamples.add(profile.utf8Encode.inMicroseconds);
          bridgeSamples.add(profile.bridgeTotal.inMicroseconds);
          mappingSamples.add(profile.resultMapping.inMicroseconds);
          inlineTokens = profile.nativeInlineTokenCount;
        }
      }
      for (final samples in [
        totalSamples,
        encodeSamples,
        bridgeSamples,
        mappingSamples,
      ]) {
        samples.sort();
      }
      debugPrint(
        'flark_web_wasm_active_shard workload=$workload '
        'bytes=${markdown.length} inline_tokens=$inlineTokens '
        'total_p50_us=${_percentile(totalSamples, 50)} '
        'total_p95_us=${_percentile(totalSamples, 95)} '
        'total_max_us=${totalSamples.last} '
        'encode_p95_us=${_percentile(encodeSamples, 95)} '
        'bridge_p95_us=${_percentile(bridgeSamples, 95)} '
        'mapping_p95_us=${_percentile(mappingSamples, 95)}',
      );
    }

    for (final config in const [
      (bytes: 64, warmups: 8, measured: 40),
      (bytes: 1024, warmups: 8, measured: 40),
      (bytes: 4096, warmups: 6, measured: 30),
      (bytes: 16384, warmups: 4, measured: 15),
      (bytes: 65536, warmups: 2, measured: 8),
    ]) {
      await measure(
        workload: 'token_dense',
        markdown: _activeShardOfSize(config.bytes),
        warmups: config.warmups,
        measured: config.measured,
      );
    }
    for (final config in const [
      (bytes: 4096, warmups: 6, measured: 30),
      (bytes: 65536, warmups: 2, measured: 8),
    ]) {
      await measure(
        workload: 'plain',
        markdown: _plainShardOfSize(config.bytes),
        warmups: config.warmups,
        measured: config.measured,
      );
    }
  }, skip: !kIsWeb);

  testWidgets('a warmed async WASM parse can publish before the next paint', (
    tester,
  ) async {
    final backend = flarkTestNativeBackend();
    await tester.runAsync(() async {
      await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 0,
          markdown: '**warm**',
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );
    });
    final paints = <_PaintReceipt>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: SizedBox(
            width: 400,
            child: _AsyncWasmEditor(backend: backend, paints: paints),
          ),
        ),
      ),
    );
    final finder = find.byKey(_editorKey);
    final inputBefore = tester.state<EditableTextState>(finder);
    inputBefore.widget.focusNode.requestFocus();
    await tester.pump();
    final paintsBeforeEdit = paints.length;

    inputBefore.updateEditingValue(
      const TextEditingValue(
        text: '**bold**',
        selection: TextSelection.collapsed(offset: 8),
      ),
    );
    final host = tester.state<_AsyncWasmEditorState>(
      find.byType(_AsyncWasmEditor),
    );
    debugPrint(
      'flark_web_wasm_checkpoint after_delta source=${host.sourceRevision} '
      'authoritative=${host.authoritativeRevision} '
      'inline=${host.parseCompletedInline} paints=${paints.length}',
    );
    expect(host.sourceRevision, 1);
    expect(host.authoritativeRevision, 0);
    expect(host.parseCompletedInline, isFalse);

    await tester.runAsync(host.waitForLatestParse);

    debugPrint(
      'flark_web_wasm_checkpoint after_parse source=${host.sourceRevision} '
      'authoritative=${host.authoritativeRevision} strong=${host.hasStrong} '
      'paints=${paints.length} parse_us=${host.lastParseMicros}',
    );

    // The parse/projection state is ready without allowing a new frame to
    // paint. In a browser, a warmed Promise continuation runs in the
    // microtask checkpoint before the next rendering opportunity.
    expect(host.authoritativeRevision, host.sourceRevision);
    expect(host.hasStrong, isTrue);
    expect(paints.length, paintsBeforeEdit);
    expect(host.lastParseMicros, greaterThan(0));

    await tester.pump();

    debugPrint(
      'flark_web_wasm_checkpoint after_pump paints=${paints.length} '
      'last_source=${paints.isEmpty ? -1 : paints.last.sourceRevision} '
      'last_authoritative='
      '${paints.isEmpty ? -1 : paints.last.authoritativeRevision}',
    );

    final inputAfter = tester.state<EditableTextState>(finder);
    expect(identical(inputBefore, inputAfter), isTrue);
    expect(tester.testTextInput.hasAnyClients, isTrue);
    expect(paints.last.sourceRevision, 1);
    expect(paints.last.authoritativeRevision, 1);
    expect(paints.last.hasStrong, isTrue);
    expect(_containsBold(_textSpan(tester, inputAfter.widget)), isTrue);
    debugPrint(
      'flark_web_wasm_before_paint parse_us=${host.lastParseMicros} '
      'same_input_host=true authoritative_revision=1',
    );
  }, skip: !kIsWeb);
}

const _editorKey = Key('web-wasm-urgent-editor');

final class _AsyncWasmEditor extends StatefulWidget {
  const _AsyncWasmEditor({required this.backend, required this.paints});

  final FlarkNativeComrakParseBackend backend;
  final List<_PaintReceipt> paints;

  @override
  State<_AsyncWasmEditor> createState() => _AsyncWasmEditorState();
}

final class _AsyncWasmEditorState extends State<_AsyncWasmEditor> {
  late final _ProbeTextController _controller;
  late final FocusNode _focusNode;
  Future<void>? _pendingParse;
  var _sourceRevision = 0;
  var _authoritativeRevision = 0;
  var _lastParseMicros = 0;
  var _parseCompletedInline = false;
  var _synchronizing = false;
  late String _lastText;

  int get sourceRevision => _sourceRevision;
  int get authoritativeRevision => _authoritativeRevision;
  int get lastParseMicros => _lastParseMicros;
  bool get parseCompletedInline => _parseCompletedInline;
  bool get hasStrong => _controller.hasStrong;

  @override
  void initState() {
    super.initState();
    _controller = _ProbeTextController(text: '**bold*', hasStrong: false)
      ..addListener(_handleEdit);
    _lastText = _controller.text;
    _focusNode = FocusNode();
  }

  void _handleEdit() {
    if (_synchronizing) return;
    final markdown = _controller.text;
    if (markdown == _lastText) return;
    _lastText = markdown;
    _sourceRevision += 1;
    final requestedRevision = _sourceRevision;
    _parseCompletedInline = false;
    final stopwatch = Stopwatch()..start();
    var completed = false;
    final parse = widget.backend.parse(
      FlarkMarkdownParseRequest(
        revision: requestedRevision,
        markdown: markdown,
        profile: FlarkMarkdownProfile.commonMarkGfm,
      ),
    );
    _pendingParse = parse.then((result) {
      completed = true;
      stopwatch.stop();
      _lastParseMicros = stopwatch.elapsedMicroseconds;
      if (!mounted || requestedRevision != _sourceRevision) return;
      _synchronizing = true;
      _controller.hasStrong = result.inlineTokens.any(
        (token) => token.type == 'strong',
      );
      _synchronizing = false;
      setState(() {
        _authoritativeRevision = requestedRevision;
      });
    });
    // Even with a warmed module, the current bridge deliberately exposes a
    // Future. This flag distinguishes a true inline return from completion in
    // the browser microtask checkpoint.
    _parseCompletedInline = completed;
  }

  Future<void> waitForLatestParse() async {
    await _pendingParse;
  }

  @override
  void dispose() {
    _controller
      ..removeListener(_handleEdit)
      ..dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      foregroundPainter: _PaintReceiptPainter(
        sourceRevision: _sourceRevision,
        authoritativeRevision: _authoritativeRevision,
        hasStrong: _controller.hasStrong,
        receipts: widget.paints,
      ),
      child: EditableText(
        key: _editorKey,
        controller: _controller,
        focusNode: _focusNode,
        style: const TextStyle(fontSize: 16),
        cursorColor: const Color(0xFF006ADC),
        backgroundCursorColor: const Color(0x00000000),
        maxLines: null,
      ),
    );
  }
}

final class _ProbeTextController extends TextEditingController {
  _ProbeTextController({required String text, required this.hasStrong})
    : super(text: text);

  bool hasStrong;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    return TextSpan(
      text: text,
      style: (style ?? const TextStyle()).merge(
        hasStrong ? const TextStyle(fontWeight: FontWeight.w700) : null,
      ),
    );
  }
}

final class _PaintReceiptPainter extends CustomPainter {
  const _PaintReceiptPainter({
    required this.sourceRevision,
    required this.authoritativeRevision,
    required this.hasStrong,
    required this.receipts,
  });

  final int sourceRevision;
  final int authoritativeRevision;
  final bool hasStrong;
  final List<_PaintReceipt> receipts;

  @override
  void paint(Canvas canvas, Size size) {
    receipts.add(
      _PaintReceipt(
        sourceRevision: sourceRevision,
        authoritativeRevision: authoritativeRevision,
        hasStrong: hasStrong,
      ),
    );
  }

  @override
  bool shouldRepaint(_PaintReceiptPainter oldDelegate) {
    return sourceRevision != oldDelegate.sourceRevision ||
        authoritativeRevision != oldDelegate.authoritativeRevision ||
        hasStrong != oldDelegate.hasStrong;
  }
}

final class _PaintReceipt {
  const _PaintReceipt({
    required this.sourceRevision,
    required this.authoritativeRevision,
    required this.hasStrong,
  });

  final int sourceRevision;
  final int authoritativeRevision;
  final bool hasStrong;
}

String _activeShardOfSize(int targetLength) {
  final output = StringBuffer();
  var index = 0;
  while (output.length < targetLength) {
    output.write('word$index **bold** *em* `code` ');
    index += 1;
  }
  return output.toString().substring(0, targetLength);
}

String _plainShardOfSize(int targetLength) {
  const unit = 'ordinary words without markdown delimiters ';
  final output = StringBuffer();
  while (output.length < targetLength) {
    output.write(unit);
  }
  return output.toString().substring(0, targetLength);
}

TextSpan _textSpan(WidgetTester tester, EditableText editable) {
  return editable.controller.buildTextSpan(
    context: tester.element(find.byKey(_editorKey)),
    style: editable.style,
    withComposing: true,
  );
}

bool _containsBold(InlineSpan span) {
  if (span.style?.fontWeight == FontWeight.w700) return true;
  if (span is! TextSpan) return false;
  return span.children?.any(_containsBold) ?? false;
}

int _percentile(List<int> sortedValues, int percentile) {
  return sortedValues[((sortedValues.length - 1) * percentile) ~/ 100];
}
