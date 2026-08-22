// G3 headless probe — the same in-frame pump, no Flutter, no display.
//
// Run from example/:
//   dart run lib/g3_headless_probe.dart --lib <abs path to dylib>
//
// This measures the parser-side numbers exactly (pumps per edit, microseconds
// blocked per pump, fuel-abort behaviour). `g3_sync_spike.dart` runs the same
// engine from a real Flutter persistent frame callback for FrameTiming.

import 'dart:io';

import 'g3_inframe_engine.dart';

const int _defaultBudgetMicros = 4000;

void main(List<String> args) {
  String? libraryPath;
  for (var i = 0; i < args.length; i += 1) {
    if (args[i] == '--lib' && i + 1 < args.length) libraryPath = args[i + 1];
  }

  final sizes = <({String label, int targetBytes})>[
    (label: '1KB', targetBytes: 1024),
    (label: '100KB', targetBytes: 100 * 1024),
    (label: '1MB', targetBytes: 1024 * 1024),
  ];

  for (final size in sizes) {
    _runSize(size.label, size.targetBytes, libraryPath);
  }
  exit(0);
}

void _runSize(String label, int targetBytes, String? libraryPath) {
  final markdown = buildParagraphFixture(targetBytes);
  stdout.writeln(
    'g3 fixture size=$label bytes=${markdown.length} '
    'paragraphs=${'\n\n'.allMatches(markdown).length + 1}',
  );

  final document = G3InFrameDocument.open(markdown, libraryPath: libraryPath);

  // ---- cold open --------------------------------------------------------
  final coldFrames = <int>[];
  var coldFrameCount = 0;
  final coldWatch = Stopwatch()..start();
  while (!document.isExactCurrent && coldFrameCount < 20000) {
    final receipt = document.pump(budgetMicros: _defaultBudgetMicros);
    coldFrames.add(receipt.elapsedMicros);
    coldFrameCount += 1;
    if (!receipt.budgetExhausted && receipt.quiescent && !receipt.exactCurrent) {
      stdout.writeln('g3 cold size=$label STALLED quiescent-but-not-current');
      break;
    }
    if (document.failure != null) break;
  }
  coldWatch.stop();
  if (document.failure != null) {
    stdout.writeln('g3 cold size=$label FAILURE ${document.failure}');
    return;
  }
  stdout.writeln(
    'g3 cold size=$label frames=$coldFrameCount '
    'wall=${coldWatch.elapsedMilliseconds}ms '
    'maxframe=${_max(coldFrames)}us p50frame=${_pct(coldFrames, 50)}us '
    'exact=${document.isExactCurrent}',
  );

  // ---- isolated single-character edits, unbounded pump ------------------
  // Unbounded budget answers: what does one edit actually cost synchronously?
  const editCount = 120;
  final editMicros = <int>[];
  final editIterations = <int>[];
  final editPumps = <int>[];
  var oneShot = 0;
  for (var i = 0; i < editCount; i += 1) {
    final offset = _midParagraphOffset(document.sourceLengthUtf16, i);
    document.applyInsert(offset, 'x');
    final receipt = document.pump(budgetMicros: 1 << 30);
    var pumps = 1;
    while (!document.isExactCurrent && pumps < 10000) {
      document.pump(budgetMicros: 1 << 30);
      pumps += 1;
    }
    if (pumps == 1 && receipt.exactCurrent) oneShot += 1;
    editMicros.add(receipt.elapsedMicros);
    editIterations.add(receipt.iterations);
    editPumps.add(pumps);
    if (document.failure != null) {
      stdout.writeln('g3 edit size=$label FAILURE ${document.failure}');
      return;
    }
  }
  stdout.writeln(
    'g3 edit-unbounded size=$label n=$editCount '
    'oneshot=$oneShot/$editCount '
    'p50=${_pct(editMicros, 50)}us p90=${_pct(editMicros, 90)}us '
    'p99=${_pct(editMicros, 99)}us max=${_max(editMicros)}us '
    'iters_p50=${_pct(editIterations, 50)} iters_max=${_max(editIterations)} '
    'under2ms=${_fractionUnder(editMicros, 2000)} '
    'under4ms=${_fractionUnder(editMicros, 4000)} '
    'under8ms=${_fractionUnder(editMicros, 8000)}',
  );

  // ---- isolated single-character edits, budgeted pump -------------------
  final budgetedFrames = <int>[];
  final budgetedFirstPump = <int>[];
  var budgetedOneFrame = 0;
  for (var i = 0; i < editCount; i += 1) {
    final offset = _midParagraphOffset(document.sourceLengthUtf16, i + 7);
    document.applyInsert(offset, 'x');
    var frames = 0;
    var first = 0;
    while (!document.isExactCurrent && frames < 10000) {
      final receipt = document.pump(budgetMicros: _defaultBudgetMicros);
      if (frames == 0) first = receipt.elapsedMicros;
      frames += 1;
    }
    if (frames <= 1) budgetedOneFrame += 1;
    budgetedFrames.add(frames);
    budgetedFirstPump.add(first);
    if (document.failure != null) {
      stdout.writeln('g3 budgeted size=$label FAILURE ${document.failure}');
      return;
    }
  }
  stdout.writeln(
    'g3 edit-budgeted size=$label budget=${_defaultBudgetMicros}us '
    'n=$editCount oneframe=$budgetedOneFrame/$editCount '
    'frames_p50=${_pct(budgetedFrames, 50)} frames_p99=${_pct(budgetedFrames, 99)} '
    'frames_max=${_max(budgetedFrames)} '
    'firstpump_p50=${_pct(budgetedFirstPump, 50)}us '
    'firstpump_max=${_max(budgetedFirstPump)}us',
  );

  // ---- sustained typing: one edit per pump ------------------------------
  final sustainedMicros = <int>[];
  var sustainedExactAtPumpEnd = 0;
  const sustainedPumps = 240;
  var sustainedCompleted = 0;
  for (var i = 0; i < sustainedPumps; i += 1) {
    if (document.failure != null) break;
    final offset = _midParagraphOffset(document.sourceLengthUtf16, i + 31);
    document.applyInsert(offset, 'y');
    final receipt = document.pump(budgetMicros: _defaultBudgetMicros);
    sustainedMicros.add(receipt.elapsedMicros);
    if (receipt.exactCurrent) sustainedExactAtPumpEnd += 1;
    sustainedCompleted += 1;
    if (document.failure != null) {
      stdout.writeln(
        'g3 sustained size=$label FAILURE_AT_PUMP=$i ${document.failure} '
        '| ${document.failureDiagnosis}',
      );
      stdout.writeln(document.failureStack ?? StackTrace.empty);
      return;
    }
  }
  if (sustainedCompleted != sustainedPumps) {
    stdout.writeln(
      'g3 sustained size=$label INCOMPLETE completed=$sustainedCompleted '
      '${document.failure} | ${document.failureDiagnosis}',
    );
    return;
  }
  stdout.writeln(
    'g3 sustained size=$label pumps=$sustainedPumps '
    'budget=${_defaultBudgetMicros}us '
    'exact_at_pump_end=$sustainedExactAtPumpEnd/$sustainedPumps '
    'p50=${_pct(sustainedMicros, 50)}us p90=${_pct(sustainedMicros, 90)}us '
    'p99=${_pct(sustainedMicros, 99)}us max=${_max(sustainedMicros)}us',
  );
  // Drain to current before the abort probe.
  var drain = 0;
  while (!document.isExactCurrent && drain < 100000) {
    document.pump(budgetMicros: _defaultBudgetMicros);
    drain += 1;
  }

  // ---- fuel abort: a large paste under a deliberately small budget ------
  final pasteBody = buildParagraphFixture(32 * 1024);
  final pasteOffset = _midParagraphOffset(document.sourceLengthUtf16, 3);
  final expectedLength = document.sourceLengthUtf16 + pasteBody.length;
  const abortBudget = 1000;
  document.applyInsert(pasteOffset, pasteBody);
  final abortFrames = <int>[];
  var exhaustedFrames = 0;
  var frames = 0;
  while (!document.isExactCurrent && frames < 100000) {
    final receipt = document.pump(budgetMicros: abortBudget);
    abortFrames.add(receipt.elapsedMicros);
    if (receipt.budgetExhausted) exhaustedFrames += 1;
    frames += 1;
    if (document.failure != null) break;
  }
  if (document.failure != null) {
    stdout.writeln('g3 abort size=$label FAILURE ${document.failure}');
    return;
  }
  final actualLength = document.sourceLengthUtf16;
  final exported = document.session.source.toString();
  final intact =
      actualLength == expectedLength &&
      exported.length == expectedLength &&
      exported.substring(pasteOffset, pasteOffset + pasteBody.length) ==
          pasteBody;
  stdout.writeln(
    'g3 abort size=$label paste=${pasteBody.length} budget=${abortBudget}us '
    'frames=$frames exhausted=$exhaustedFrames '
    'maxframe=${_max(abortFrames)}us p99frame=${_pct(abortFrames, 99)}us '
    'exact=${document.isExactCurrent} source_intact=$intact',
  );

  stdout.writeln(
    'g3 native size=$label dispatch=${document.endpoint.dispatchCalls} '
    'poll=${document.endpoint.pollCalls} '
    'candidate=${document.endpoint.candidatePollCalls} '
    'encode=${document.endpoint.encodeCalls} '
    'encoded_bytes=${document.endpoint.encodedBytes} '
    'native_us=${document.endpoint.nativeMicros}',
  );

  document.dispose();
  stdout.writeln('g3 done size=$label');
}

/// Blank-line separated plain paragraphs with ordinary inline markup.
///
/// Deliberately avoids every known engine fault shape: no reference
/// definitions, no line longer than ~90 bytes, no line whose first
/// non-space byte is a block-start marker.
String buildParagraphFixture(int targetBytes) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetBytes) {
    buffer.writeln(
      'Paragraph $index opens with ordinary prose and a **bold** run here.',
    );
    buffer.writeln(
      'It continues with _emphasis_, some `inline code`, and plain words.',
    );
    buffer.writeln(
      'Then a third physical line closes the paragraph with more text.',
    );
    buffer.writeln();
    index += 1;
  }
  return buffer.toString();
}

/// A deterministic offset well inside a paragraph line, never at a boundary.
int _midParagraphOffset(int length, int salt) {
  final base = length ~/ 2;
  return (base + (salt * 37) % 40 + 4).clamp(1, length - 1);
}

int _max(List<int> values) =>
    values.isEmpty ? -1 : values.reduce((a, b) => a > b ? a : b);

int _pct(List<int> values, int percentile) {
  if (values.isEmpty) return -1;
  final sorted = [...values]..sort();
  final index = ((sorted.length - 1) * percentile / 100).round();
  return sorted[index];
}

String _fractionUnder(List<int> values, int micros) {
  if (values.isEmpty) return 'n/a';
  final count = values.where((value) => value <= micros).length;
  return '$count/${values.length}';
}
