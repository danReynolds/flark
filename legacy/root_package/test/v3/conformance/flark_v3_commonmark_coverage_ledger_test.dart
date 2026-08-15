import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

const _allowedStatuses = <String>{
  'authoritative_supported_probe',
  'intentional_fail_closed',
  'intentional_extension_divergence',
  'unclassified',
};

const _expectedSelectorsByStatus = <String, List<String>>{
  'authoritative_supported_probe': <String>[
    '12-17',
    '20',
    '43-45',
    '47-59',
    '107',
    '110-118',
    '231',
    '234',
    '242',
    '480-482',
    '485',
    '489',
    '496',
    '510',
    '516-518',
    '572',
    '574-575',
    '594-601',
    '603-605',
  ],
  'intentional_fail_closed': <String>[
    '46',
    '60-61',
    '108-109',
    '230',
    '235',
    '237',
    '239',
    '244',
    '250-252',
    '602',
    '606-610',
  ],
  'intentional_extension_divergence': <String>['611-612'],
  'unclassified': <String>[
    '1-11',
    '18-19',
    '21-42',
    '62-106',
    '119-229',
    '232-233',
    '236',
    '238',
    '240-241',
    '243',
    '245-249',
    '253-479',
    '483-484',
    '486-488',
    '490-495',
    '497-509',
    '511-515',
    '519-571',
    '573',
    '576-593',
    '613-652',
  ],
};

void main() {
  final ledgerFile = File('test/fixtures/commonmark/v3_coverage_ledger.json');
  final ledger = _jsonObject(ledgerFile);
  final corpus = _object(ledger, 'corpus');
  final fixtureFile = File(_string(corpus, 'fixture'));
  final fixtures = _jsonList(fixtureFile);

  test('pins the complete CommonMark 0.31.2 fixture inventory', () {
    expect(ledger['schemaVersion'], 1);
    expect(corpus['name'], 'CommonMark');
    expect(corpus['specVersion'], '0.31.2');
    expect(fixtures, hasLength(_integer(corpus, 'expectedExamples')));
    expect(
      sha256.convert(fixtureFile.readAsBytesSync()).toString(),
      corpus['sha256'],
    );

    final examples = fixtures
        .map((fixture) => _integer(_asObject(fixture, 'fixture'), 'example'))
        .toList(growable: false);
    expect(examples, List<int>.generate(652, (index) => index + 1));
    for (final fixture in fixtures) {
      final object = _asObject(fixture, 'fixture');
      expect(_string(object, 'section'), isNotEmpty);
      expect(_string(object, 'markdown'), isNotEmpty);
      expect(object['html'], isA<String>());
    }
  });

  test(
    'classifies every fixture exactly once without inflating pass coverage',
    () {
      final definitions = _object(ledger, 'statusDefinitions');
      expect(definitions.keys.toSet(), _allowedStatuses);
      for (final status in _allowedStatuses) {
        expect(_string(definitions, status), isNotEmpty);
      }

      final classifications = _list(ledger, 'classifications');
      final statusCounts = <String, int>{};
      final classifiedByExample = <int, String>{};

      for (final value in classifications) {
        final classification = _asObject(value, 'classification');
        final status = _string(classification, 'status');
        expect(_allowedStatuses, contains(status));
        expect(
          statusCounts,
          isNot(contains(status)),
          reason: 'duplicate status $status',
        );

        final selectors = _list(
          classification,
          'selectors',
        ).map(_asString).toList(growable: false);
        expect(
          selectors,
          _expectedSelectorsByStatus[status],
          reason: '$status changed without updating its executable baseline',
        );
        final examples = _expandSelectors(selectors);
        expect(examples, hasLength(_integer(classification, 'expectedCount')));
        statusCounts[status] = examples.length;
        for (final example in examples) {
          final previous = classifiedByExample[example];
          expect(
            previous,
            isNull,
            reason:
                'example $example is classified as both $previous and $status',
          );
          classifiedByExample[example] = status;
        }

        final evidence = _list(classification, 'evidence');
        if (status == 'unclassified') {
          expect(evidence, isEmpty);
        } else {
          expect(evidence, isNotEmpty);
        }
      }

      expect(statusCounts.keys.toSet(), _allowedStatuses);
      expect(statusCounts, <String, int>{
        'authoritative_supported_probe': 60,
        'intentional_fail_closed': 19,
        'intentional_extension_divergence': 2,
        'unclassified': 571,
      });

      final fixtureExamples = fixtures
          .map((fixture) => _integer(_asObject(fixture, 'fixture'), 'example'))
          .toSet();
      expect(classifiedByExample.keys.toSet(), fixtureExamples);
      expect(
        statusCounts['authoritative_supported_probe'],
        lessThan(fixtureExamples.length),
        reason:
            'the ledger must not turn inventory accounting into a pass claim',
      );
    },
  );

  test('keeps every asserted classification tied to live test evidence', () {
    final sources = _object(ledger, 'evidenceSources');
    final referencedSources = <String>{};
    for (final value in _list(ledger, 'classifications')) {
      final classification = _asObject(value, 'classification');
      referencedSources.addAll(
        _list(classification, 'evidence').map(_asString),
      );
    }
    expect(referencedSources, sources.keys.toSet());

    for (final entry in sources.entries) {
      final source = _asObject(entry.value, 'evidence source ${entry.key}');
      final file = File(_string(source, 'path'));
      expect(file.existsSync(), isTrue, reason: '${file.path} is missing');
      final contents = file.readAsStringSync();
      final anchors = _list(source, 'anchors').map(_asString).toList();
      expect(anchors, isNotEmpty);
      for (final anchor in anchors) {
        expect(
          contents,
          contains(anchor),
          reason: '${file.path} no longer contains $anchor',
        );
      }
    }
  });
}

Map<String, Object?> _jsonObject(File file) {
  return _asObject(jsonDecode(file.readAsStringSync()), file.path);
}

List<Object?> _jsonList(File file) {
  final value = jsonDecode(file.readAsStringSync());
  if (value is! List<Object?>) {
    throw FormatException('${file.path} must contain a JSON list');
  }
  return value;
}

Map<String, Object?> _object(Map<String, Object?> object, String key) {
  return _asObject(object[key], key);
}

Map<String, Object?> _asObject(Object? value, String label) {
  if (value is! Map<String, Object?>) {
    throw FormatException('$label must be a JSON object');
  }
  return value;
}

List<Object?> _list(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value is! List<Object?>) {
    throw FormatException('$key must be a JSON list');
  }
  return value;
}

String _string(Map<String, Object?> object, String key) {
  return _asString(object[key]);
}

String _asString(Object? value) {
  if (value is! String) {
    throw FormatException('expected a JSON string, got $value');
  }
  return value;
}

int _integer(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value is! int) {
    throw FormatException('$key must be a JSON integer');
  }
  return value;
}

Set<int> _expandSelectors(Iterable<String> selectors) {
  final examples = <int>{};
  final pattern = RegExp(r'^(\d+)(?:-(\d+))?$');
  for (final selector in selectors) {
    final match = pattern.firstMatch(selector);
    if (match == null) {
      throw FormatException('invalid example selector $selector');
    }
    final start = int.parse(match.group(1)!);
    final end = int.parse(match.group(2) ?? match.group(1)!);
    if (start < 1 || end < start) {
      throw FormatException('invalid example selector $selector');
    }
    for (var example = start; example <= end; example++) {
      if (!examples.add(example)) {
        throw FormatException('duplicate example $example within one status');
      }
    }
  }
  return examples;
}
