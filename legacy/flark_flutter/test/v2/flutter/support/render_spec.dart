import 'package:flark_flutter/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';
import 'package:flutter_test/flutter_test.dart';

import 'inline_sequence_harness.dart';

/// One-string render assertions: a markdown [source] on the left, its
/// rendered form on the right as the display text with styled spans wrapped
/// in tags —
///
/// ```dart
/// await expectRendered(
///   'hello *world* how **are** you',
///   'hello <em>world</em> how <strong>are</strong> you',
/// );
/// ```
///
/// Tags: `<em>`, `<strong>`, `<del>` (strikethrough), `<code>`, `<a>` (links
/// and autolinks, destination hidden). Text the parser leaves literal appears
/// literally — `'**foo **bar'` renders as `'**foo **bar'`. Other inline kinds
/// (images, raw HTML) are outside this notation.
///
/// On failure the actual annotated render is printed, so authoring a spec is
/// "run once, paste the verified actual".
Future<void> expectRendered(String source, String rendered) async {
  final controller = FlarkFlutterController.fromMarkdown(source);
  try {
    expect(
      controller.tryParseSync(),
      isTrue,
      reason: 'render spec needs a sync-capable parse backend',
    );
    expect(
      annotatedDisplay(controller),
      rendered,
      reason: 'render spec for source "$source"',
    );
  } finally {
    controller.dispose();
  }
}

/// [expectRendered], but the [source] is *typed into a live editor one
/// character at a time* instead of loaded — the "user speaking markdown"
/// flow. Every keystroke passes the export round-trip gate, the final source
/// must equal the literal keystrokes (the editor never rewrites hand-typed
/// text), and the final render must match [rendered] — so a typed document
/// and a loaded document are provably equivalent.
Future<void> expectTypedRendered(String source, String rendered) async {
  final sequence = await InlineSequence.start('');
  try {
    await sequence.typeSource(source);
    expect(
      sequence.controller.markdown,
      source,
      reason: 'typed source must equal the literal keystrokes',
    );
    expect(
      annotatedDisplay(sequence.controller),
      rendered,
      reason: 'typed render spec for source "$source"',
    );
  } finally {
    sequence.dispose();
  }
}

/// The controller's display text with each styled inline run wrapped in its
/// tag, e.g. `hello <em>world</em>`.
String annotatedDisplay(FlarkFlutterController controller) {
  final display = controller.projection.projectText(controller.markdown);
  final runs = <FlarkRenderInlineRun>[];
  void visit(FlarkRenderBlock block) {
    for (final run in block.inlineRuns) {
      if (_tagFor(run.kind) != null &&
          run.displayRange.start < run.displayRange.end) {
        runs.add(run);
      }
    }
    block.children.forEach(visit);
  }

  controller.renderPlan.blocks.forEach(visit);

  // Tag events, ordered so the annotation nests the way the parse tree does.
  // Display offsets alone cannot break ties — `***x***` is two runs with the
  // *same* display range (their markers are hidden) — so ties order by source
  // range: the outer run opens first and closes last.
  final events = <_TagEvent>[];
  for (final run in runs) {
    final tag = _tagFor(run.kind)!;
    events.add(
      _TagEvent(
        position: run.displayRange.start,
        isOpen: true,
        sourceStart: run.sourceRange.start,
        sourceEnd: run.sourceRange.end,
        text: '<$tag>',
      ),
    );
    events.add(
      _TagEvent(
        position: run.displayRange.end,
        isOpen: false,
        sourceStart: run.sourceRange.start,
        sourceEnd: run.sourceRange.end,
        text: '</$tag>',
      ),
    );
  }
  events.sort(_TagEvent.compare);

  final buffer = StringBuffer();
  var cursor = 0;
  for (final event in events) {
    buffer.write(display.substring(cursor, event.position));
    buffer.write(event.text);
    cursor = event.position;
  }
  buffer.write(display.substring(cursor));
  return buffer.toString();
}

String? _tagFor(FlarkMarkdownInlineKind kind) {
  return switch (kind) {
    FlarkMarkdownInlineKind.emphasis => 'em',
    FlarkMarkdownInlineKind.strong => 'strong',
    FlarkMarkdownInlineKind.strikethrough => 'del',
    FlarkMarkdownInlineKind.inlineCode => 'code',
    FlarkMarkdownInlineKind.link => 'a',
    FlarkMarkdownInlineKind.autolink => 'a',
    _ => null,
  };
}

final class _TagEvent {
  const _TagEvent({
    required this.position,
    required this.isOpen,
    required this.sourceStart,
    required this.sourceEnd,
    required this.text,
  });

  final int position;
  final bool isOpen;
  final int sourceStart;
  final int sourceEnd;
  final String text;

  static int compare(_TagEvent left, _TagEvent right) {
    if (left.position != right.position) {
      return left.position - right.position;
    }
    // Closes before opens at the same position (`<em>a</em><strong>b</strong>`).
    if (left.isOpen != right.isOpen) {
      return left.isOpen ? 1 : -1;
    }
    if (left.isOpen) {
      // Outer (earlier source start) opens first.
      if (left.sourceStart != right.sourceStart) {
        return left.sourceStart - right.sourceStart;
      }
      return right.sourceEnd - left.sourceEnd;
    }
    // Inner (later source start) closes first.
    if (left.sourceEnd != right.sourceEnd) {
      return left.sourceEnd - right.sourceEnd;
    }
    return right.sourceStart - left.sourceStart;
  }
}
