import 'dart:convert';
import 'dart:io';

final class FlutterMachineTestReceipt {
  const FlutterMachineTestReceipt({
    required this.name,
    required this.result,
    required this.skipped,
    required this.runnerSucceeded,
  });

  final String name;
  final String result;
  final bool skipped;
  final bool runnerSucceeded;

  bool get passed => result == 'success' && !skipped && runnerSucceeded;
}

FlutterMachineTestReceipt verifyFlutterMachineTest(
  Iterable<String> lines, {
  required String expectedName,
}) {
  final testNames = <int, String>{};
  final completions = <int, ({String result, bool skipped})>{};
  bool? runnerSucceeded;

  for (final line in lines) {
    Object? decoded;
    try {
      decoded = jsonDecode(line);
    } on FormatException {
      // Flutter can interleave non-protocol dependency/build output with its
      // machine event stream. Only decoded JSON objects are protocol events.
      continue;
    }
    if (decoded is! Map<String, Object?>) continue;
    switch (decoded['type']) {
      case 'testStart':
        final test = decoded['test'];
        if (test is Map<String, Object?> &&
            test['id'] is int &&
            test['name'] is String) {
          testNames[test['id']! as int] = test['name']! as String;
        }
      case 'testDone':
        final id = decoded['testID'];
        final result = decoded['result'];
        final skipped = decoded['skipped'];
        if (id is int && result is String && skipped is bool) {
          completions[id] = (result: result, skipped: skipped);
        }
      case 'done':
        final success = decoded['success'];
        if (success is bool) runnerSucceeded = success;
    }
  }

  final matching = testNames.entries
      .where((entry) => entry.value == expectedName)
      .toList(growable: false);
  if (matching.length != 1) {
    throw FormatException(
      'Expected exactly one machine test named "$expectedName"; '
      'found ${matching.length}.',
    );
  }
  final completion = completions[matching.single.key];
  if (completion == null) {
    throw FormatException(
      'Machine test "$expectedName" did not emit testDone.',
    );
  }
  if (runnerSucceeded == null) {
    throw const FormatException('Flutter machine stream did not emit done.');
  }
  final receipt = FlutterMachineTestReceipt(
    name: expectedName,
    result: completion.result,
    skipped: completion.skipped,
    runnerSucceeded: runnerSucceeded,
  );
  if (!receipt.passed) {
    throw StateError(
      'Required Flutter test "$expectedName" did not execute successfully: '
      'result=${receipt.result}, skipped=${receipt.skipped}, '
      'runnerSucceeded=${receipt.runnerSucceeded}.',
    );
  }
  return receipt;
}

Future<void> main(List<String> arguments) async {
  if (arguments.length != 2) {
    stderr.writeln(
      'usage: dart run scripts/verify_flutter_machine_test.dart '
      '<machine-log.jsonl> <exact-test-name>',
    );
    exitCode = 64;
    return;
  }
  try {
    final receipt = verifyFlutterMachineTest(
      await File(arguments[0]).readAsLines(),
      expectedName: arguments[1],
    );
    stdout.writeln(
      'flutter-machine-test: PASS name="${receipt.name}" skipped=false',
    );
  } on Object catch (error) {
    stderr.writeln('flutter-machine-test: FAIL $error');
    exitCode = 1;
  }
}
