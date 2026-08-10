import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:super_editor/super_editor.dart';

/// The frozen `ordinary-prose` cycle from benchmark/v4/workloads_v1.json.
const ordinaryProseCycle =
    'Ordinary prose opens with a clear sentence and a small **bold** run.\n'
    'It continues with _emphasis_, `code`, and a direct '
    '[link](https://example.invalid/).\n\n';

/// Generates the frozen ASCII recipe at exactly [targetBytes] UTF-8 bytes.
String generateOrdinaryProseFixture(int targetBytes) {
  if (targetBytes < 1) {
    throw ArgumentError.value(targetBytes, 'targetBytes', 'must be positive');
  }
  final cycle = ascii.encode(ordinaryProseCycle);
  final bytes = Uint8List(targetBytes);
  for (var offset = 0; offset < targetBytes; offset += 1) {
    bytes[offset] = cycle[offset % cycle.length];
  }
  return ascii.decode(bytes);
}

String sha256Text(String value) =>
    sha256.convert(utf8.encode(value)).toString();

/// Maps plain source into default SuperEditor paragraph nodes without losing
/// physical newlines. Empty lines become empty paragraphs. Joining exported
/// nodes with `\n` therefore reproduces the exact source, including a trailing
/// newline.
List<String> sourceLinesPreservingNewlines(String source) => source.split('\n');

MutableDocument documentFromExactSource(String source) {
  final lines = sourceLinesPreservingNewlines(source);
  return MutableDocument(
    nodes: [
      for (var index = 0; index < lines.length; index += 1)
        ParagraphNode(
          id: 'source-line-$index',
          text: AttributedText(lines[index]),
        ),
    ],
  );
}

String exportExactSource(Document document) {
  return document
      .map((node) {
        if (node is! TextNode) {
          throw StateError(
            'Unexpected non-text node ${node.runtimeType} in plain-source '
            'competitor fixture',
          );
        }
        return node.text.toPlainText();
      })
      .join('\n');
}

final class SourceCaret {
  const SourceCaret({required this.nodeId, required this.nodeOffset});

  final String nodeId;
  final int nodeOffset;

  DocumentPosition get position => DocumentPosition(
    nodeId: nodeId,
    nodePosition: TextNodePosition(offset: nodeOffset),
  );
}

/// Converts an exact-source offset into the equivalent paragraph position.
/// A source offset on a newline maps to the beginning of the following node.
SourceCaret sourceCaretAt(Document document, int sourceOffset) {
  var sourceLength = 0;
  for (final node in document) {
    if (node is! TextNode) {
      throw StateError('Unexpected non-text node ${node.runtimeType}');
    }
    sourceLength += node.text.length;
    if (node != document.last) {
      sourceLength += 1;
    }
  }
  if (sourceOffset < 0 || sourceOffset > sourceLength) {
    throw RangeError.range(sourceOffset, 0, sourceLength, 'sourceOffset');
  }

  var consumed = 0;
  for (final node in document) {
    if (node is! TextNode) {
      throw StateError('Unexpected non-text node ${node.runtimeType}');
    }
    final length = node.text.length;
    final end = consumed + length;
    if (sourceOffset <= end) {
      return SourceCaret(nodeId: node.id, nodeOffset: sourceOffset - consumed);
    }
    consumed = end + 1;
  }

  final last = document.last;
  if (last is! TextNode) {
    throw StateError('Unexpected non-text node ${last.runtimeType}');
  }
  return SourceCaret(nodeId: last.id, nodeOffset: last.text.length);
}
