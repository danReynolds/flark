import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'document.dart';

/// A parser-owned link, automatic link or image and its resolved destination.
/// Source coordinates are UTF-16. [text] is visible text, without Markdown.
final class InlineResource {
  InlineResource.fromRun(FlarkDocument document, RunView run)
    : run = run.index,
      block = run.block,
      isImage = run.kind == RunKind.image,
      start = run.startUtf16,
      end = run.endUtf16,
      contentStart = run.contentStartUtf16,
      contentEnd = run.contentEndUtf16,
      destination = run.destination,
      title = run.title,
      text = document.visibleText(run.contentStartUtf16, run.contentEndUtf16);

  final int run, block, start, end, contentStart, contentEnd;
  final bool isImage;
  final String text, destination, title;
}
