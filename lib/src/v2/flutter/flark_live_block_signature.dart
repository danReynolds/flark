import '../core/core.dart' show FlarkSourceRange;
import '../render_plan/render_plan.dart';

/// An offset-independent digest of everything a live-rendered block renders.
///
/// Two blocks with equal signatures render identically, so a block whose
/// signature is unchanged between parses can have its widget instance reused
/// (Stage 3) — even if earlier edits shifted its absolute offsets. Equivalently,
/// the signature MUST change whenever the block's rendered output would change
/// (text, inline styling, checkbox state, code language, table structure/cells,
/// heading level, list kind, …) and MUST NOT change merely because offsets
/// shifted. That completeness is the correctness contract; it is covered by
/// `test/v2/flutter/flark_live_block_signature_test.dart`.
///
/// All positions are emitted relative to the block's display start, so a uniform
/// shift of the whole block leaves the signature unchanged. Style/colours are
/// constant across blocks (per editor) and are intentionally NOT part of the
/// per-block signature — a style change rebuilds every block.
///
/// See `docs/architecture/live_rendered_rebuild_isolation.md`.
String liveBlockContentSignature(FlarkRenderBlock block, String displayText) {
  final buffer = StringBuffer();
  _writeBlock(buffer, block, displayText, block.displayRange.start);
  return buffer.toString();
}

void _writeBlock(
  StringBuffer buffer,
  FlarkRenderBlock block,
  String displayText,
  int base,
) {
  buffer
    ..write('B|')
    ..write(block.kind.name)
    ..write('|')
    ..write(block.type)
    ..write('|')
    ..write(block.styleToken.name)
    ..write('|')
    ..write(_attributes(block.attributes))
    ..write('|')
    ..write(_slice(displayText, block.displayRange))
    ..write('|');

  final table = block.table;
  if (table != null) {
    buffer
      ..write('T:')
      ..write(table.columnAlignments.map((a) => a.name).join(','))
      ..write(';');
    for (final row in table.rows) {
      buffer
        ..write('r')
        ..write(row.header ? 'H' : '-')
        ..write(':');
      for (final cell in row.cells) {
        buffer
          ..write(_slice(displayText, cell.displayRange))
          ..write('~');
      }
      buffer.write(';');
    }
  }

  final listItem = block.listItem;
  if (listItem != null) buffer.write('L:${listItem.kind.name};');

  final task = block.taskListItem;
  if (task != null) buffer.write('K:${task.checked};');

  final code = block.codeBlock;
  if (code != null) buffer.write('C:${code.language};');

  for (final run in block.inlineRuns) {
    buffer
      ..write('i:')
      ..write(run.kind.name)
      ..write(':')
      ..write(run.type)
      ..write(':')
      ..write(run.styleToken.name)
      ..write(':');
    final action = run.action;
    if (action != null) {
      buffer
        ..write(action.kind.name)
        ..write('@')
        ..write(action.destination)
        ..write('@')
        ..write(action.title ?? '')
        ..write('@')
        ..write(action.label ?? '');
    }
    buffer
      ..write(':')
      ..write(run.displayRange.start - base)
      ..write('+')
      ..write(run.displayRange.end - run.displayRange.start)
      ..write(';');
  }

  buffer.write('{');
  for (final child in block.children) {
    _writeBlock(buffer, child, displayText, base);
    buffer.write('|');
  }
  buffer.write('}');
}

String _slice(String displayText, FlarkSourceRange range) {
  if (range.start < 0 ||
      range.end > displayText.length ||
      range.start > range.end) {
    return '';
  }
  return displayText.substring(range.start, range.end);
}

String _attributes(Map<String, Object?> attributes) {
  if (attributes.isEmpty) return '';
  final keys = attributes.keys
      .where((key) => key != 'stableId' && key != 'synthetic')
      .toList()
    ..sort();
  return keys.map((key) => '$key=${attributes[key]}').join(',');
}
