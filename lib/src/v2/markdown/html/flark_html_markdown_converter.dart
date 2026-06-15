import 'package:html/dom.dart' as dom;
import 'package:html/parser.dart' as html_parser;

/// Converts an HTML fragment — for example content copied from a web page — to
/// Markdown.
///
/// Handles the common rich-text elements: headings, paragraphs, bold, italic,
/// strikethrough, inline code, links, images, ordered/unordered (nested)
/// lists, block quotes, code blocks, horizontal rules, and line breaks.
/// Unknown elements degrade to their text content. HTML whitespace is collapsed
/// per the usual rules; explicit `<br>` becomes a hard line break.
abstract final class FlarkHtmlMarkdown {
  /// Converts [html] to Markdown, returning the empty string for empty input.
  static String convert(String html) {
    if (html.trim().isEmpty) return '';
    final document = html_parser.parse(html);
    final container = document.body ?? document.documentElement;
    if (container == null) return '';
    return _renderFlow(container.nodes, indent: '').join('\n\n').trim();
  }

  /// Renders a flow of [nodes] (mixed block and inline) into Markdown blocks.
  /// Consecutive inline nodes coalesce into one paragraph.
  static List<String> _renderFlow(
    List<dom.Node> nodes, {
    required String indent,
  }) {
    final blocks = <String>[];
    final inline = StringBuffer();
    void flushInline() {
      final text = _squeezeSpaces(inline.toString()).trim();
      inline.clear();
      if (text.isNotEmpty) blocks.add(text);
    }

    for (final node in nodes) {
      if (node is dom.Element && _blockTags.contains(node.localName)) {
        flushInline();
        final block = _renderBlock(node, indent: indent);
        if (block.trim().isNotEmpty) blocks.add(block);
      } else {
        inline.write(_renderInline(node));
      }
    }
    flushInline();
    return blocks;
  }

  static String _renderBlock(dom.Element element, {required String indent}) {
    switch (element.localName) {
      case 'h1':
      case 'h2':
      case 'h3':
      case 'h4':
      case 'h5':
      case 'h6':
        final level = int.parse(element.localName!.substring(1));
        return '${'#' * level} ${_inlineChildren(element)}';
      case 'hr':
        return '---';
      case 'pre':
        return '```\n${element.text.trimRight()}\n```';
      case 'blockquote':
        final inner = _renderFlow(element.nodes, indent: indent).join('\n\n');
        return inner
            .split('\n')
            .map((line) => line.isEmpty ? '>' : '> $line')
            .join('\n');
      case 'ul':
        return _renderList(element, indent: indent, ordered: false);
      case 'ol':
        return _renderList(element, indent: indent, ordered: true);
      default:
        // p, div, section, article, figure … render their inner flow.
        return _renderFlow(element.nodes, indent: indent).join('\n\n');
    }
  }

  static String _renderList(
    dom.Element list, {
    required String indent,
    required bool ordered,
  }) {
    final lines = <String>[];
    var number = 1;
    for (final item in list.children.where((e) => e.localName == 'li')) {
      final marker = ordered ? '$number. ' : '- ';
      final continuationIndent = indent + ' ' * marker.length;
      final rendered = _renderListItem(item, indent: continuationIndent);
      final buffer = StringBuffer('$indent$marker${rendered.inline}');
      for (final nested in rendered.nestedBlocks) {
        buffer.write('\n$nested');
      }
      lines.add(buffer.toString());
      number += 1;
    }
    return lines.join('\n');
  }

  static ({String inline, List<String> nestedBlocks}) _renderListItem(
    dom.Element item, {
    required String indent,
  }) {
    final inline = StringBuffer();
    final nested = <String>[];
    for (final node in item.nodes) {
      if (node is dom.Element &&
          (node.localName == 'ul' || node.localName == 'ol')) {
        nested.add(
          _renderList(node, indent: indent, ordered: node.localName == 'ol'),
        );
      } else if (node is dom.Element && _blockTags.contains(node.localName)) {
        final block = _renderBlock(node, indent: indent);
        if (block.trim().isNotEmpty) nested.add(block);
      } else {
        inline.write(_renderInline(node));
      }
    }
    return (
      inline: _squeezeSpaces(inline.toString()).trim(),
      nestedBlocks: nested,
    );
  }

  static String _inlineChildren(dom.Element element) {
    final buffer = StringBuffer();
    for (final node in element.nodes) {
      buffer.write(_renderInline(node));
    }
    return _squeezeSpaces(buffer.toString()).trim();
  }

  static String _renderInline(dom.Node node) {
    if (node is dom.Text) return node.text.replaceAll(RegExp(r'\s+'), ' ');
    if (node is! dom.Element) return '';

    switch (node.localName) {
      case 'strong':
      case 'b':
        final content = _inlineChildren(node);
        return content.isEmpty ? '' : '**$content**';
      case 'em':
      case 'i':
        final content = _inlineChildren(node);
        return content.isEmpty ? '' : '*$content*';
      case 's':
      case 'del':
      case 'strike':
        final content = _inlineChildren(node);
        return content.isEmpty ? '' : '~~$content~~';
      case 'code':
        return '`${node.text}`';
      case 'br':
        return '\n';
      case 'a':
        final href = node.attributes['href'] ?? '';
        final text = _inlineChildren(node);
        return href.isEmpty ? text : '[$text]($href)';
      case 'img':
        final src = node.attributes['src'] ?? '';
        return src.isEmpty ? '' : '![${node.attributes['alt'] ?? ''}]($src)';
      default:
        return _inlineChildren(node);
    }
  }

  /// Collapses runs of horizontal whitespace to a single space, leaving any
  /// `\n` (from `<br>`) intact.
  static String _squeezeSpaces(String text) {
    return text.replaceAll(RegExp(r'[ \t]+'), ' ');
  }

  static const Set<String> _blockTags = {
    'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
    'p', 'div', 'section', 'article', 'figure',
    'hr', 'blockquote', 'pre', 'ul', 'ol', 'li', 'table',
  };
}
