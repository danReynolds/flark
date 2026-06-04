import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/flutter/flark_live_block_signature.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/projection/projection.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

typedef _SignaturePlan = ({
  String markdown,
  String text,
  List<FlarkRenderBlock> blocks,
});

// Completeness contract for liveBlockContentSignature: it must be INVARIANT
// under offset shifts and must CHANGE on every content-meaningful edit. Tests
// parse real Markdown so the signature is exercised against true render-plan
// structure (descriptors, inline runs) rather than hand-built fixtures.
void main() {
  late FlarkNativeComrakParseBackend backend;

  setUpAll(() {
    final loaded = FlarkNativeComrakParseBackend.tryLoad();
    if (loaded == null) {
      fail('native comrak bridge required for signature tests');
    }
    backend = loaded;
  });

  test('signature is invariant under offset shifts', () async {
    final a = await _plan(backend, '- [ ] task item');
    final b = await _plan(
      backend,
      'A leading paragraph that shifts everything.\n\n- [ ] task item',
    );
    expect(_sig(b, _task(b)), _sig(a, _task(a)));
  });

  test('checkbox toggle changes the signature', () async {
    final a = await _plan(backend, '- [ ] task item');
    final b = await _plan(backend, '- [x] task item');
    expect(_sig(b, _task(b)), isNot(_sig(a, _task(a))));
  });

  test('code-fence language change changes the signature', () async {
    final a = await _plan(backend, '```dart\nfinal x = 1;\n```');
    final b = await _plan(backend, '```python\nfinal x = 1;\n```');
    expect(_sig(b, _code(b)), isNot(_sig(a, _code(a))));
  });

  test('code body edit changes the signature', () async {
    final a = await _plan(backend, '```dart\nfinal x = 1;\n```');
    final b = await _plan(backend, '```dart\nfinal y = 1;\n```');
    expect(_sig(b, _code(b)), isNot(_sig(a, _code(a))));
  });

  test('table cell edit changes the signature', () async {
    final a = await _plan(backend, '| a | b |\n| - | - |\n| c | d |');
    final b = await _plan(backend, '| a | b |\n| - | - |\n| cc | d |');
    expect(_sig(b, _table(b)), isNot(_sig(a, _table(a))));
  });

  test('inline styling change (same text) changes the signature', () async {
    // Bold markers are hidden, so the display slice is identical — the
    // difference is purely the inline run styling. This is the key
    // completeness case.
    final a = await _plan(backend, 'plain text here');
    final b = await _plan(backend, 'plain **text** here');
    final sa = _sig(a, _para(a));
    final sb = _sig(b, _para(b));
    expect(sb, isNot(sa));
  });

  test('link target change (same text) changes the signature', () async {
    final a = await _plan(backend, '[docs](https://a.example)');
    final b = await _plan(backend, '[docs](https://b.example)');
    expect(_sig(b, _para(b)), isNot(_sig(a, _para(a))));
  });

  test(
    'ordered list marker change (same text) changes the signature',
    () async {
      final a = await _plan(backend, '1. item');
      final b = await _plan(backend, '2. item');
      expect(_sig(b, _listItem(b)), isNot(_sig(a, _listItem(a))));
    },
  );

  test('heading level change changes the signature', () async {
    final a = await _plan(backend, '# Title');
    final b = await _plan(backend, '## Title');
    expect(_sig(b, b.blocks.first), isNot(_sig(a, a.blocks.first)));
  });

  test('typing in a block changes only that block signature', () async {
    final a = await _plan(backend, 'first paragraph\n\nsecond paragraph');
    final b = await _plan(backend, 'first paragraphX\n\nsecond paragraph');
    // First (edited) differs; second (unchanged, shifted) is stable.
    expect(_sig(b, b.blocks.first), isNot(_sig(a, a.blocks.first)));
    expect(_sig(b, b.blocks.last), _sig(a, a.blocks.last));
  });
}

_SignaturePlan _result(FlarkMarkdownParseResult result, String markdown) {
  final projection = FlarkProjection.fromParseResult(result);
  final plan = FlarkRenderPlan.fromParseResult(
    parseResult: result,
    projection: projection,
  );
  return (
    markdown: markdown,
    text: projection.projectText(markdown),
    blocks: plan.blocks.toList(),
  );
}

Future<_SignaturePlan> _plan(
  FlarkMarkdownParseBackend backend,
  String markdown,
) async {
  final result = await backend.parse(
    FlarkMarkdownParseRequest(
      revision: 1,
      markdown: markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
  return _result(result, markdown);
}

String _sig(_SignaturePlan plan, FlarkRenderBlock block) {
  return liveBlockContentSignature(block, plan.text, markdown: plan.markdown);
}

FlarkRenderBlock _task(_SignaturePlan plan) =>
    plan.blocks.firstWhere((b) => b.taskListItem != null);

FlarkRenderBlock _listItem(_SignaturePlan plan) =>
    plan.blocks.firstWhere((b) => b.listItem != null);

FlarkRenderBlock _code(_SignaturePlan plan) =>
    plan.blocks.firstWhere((b) => b.codeBlock != null);

FlarkRenderBlock _table(_SignaturePlan plan) =>
    plan.blocks.firstWhere((b) => b.table != null);

FlarkRenderBlock _para(_SignaturePlan plan) =>
    plan.blocks.firstWhere((b) => b.kind == FlarkMarkdownBlockKind.paragraph);
