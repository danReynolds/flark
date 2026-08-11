import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

void main() {
  final profilePath = 'test/fixtures/v4/markdown_profile_v2.json';
  final profile = _object(profilePath);
  final ledger = _object('test/fixtures/v4/markdown_ledgers_v2.json');

  test('pins GFM as normative and CommonMark 0.31.2 as compatibility only', () {
    expect(profile['schemaVersion'], 2);
    expect(profile['profileId'], 'flark-gfm-0.29-v2');
    expect(ledger['profile'], profilePath);

    final gfm = _map(profile['normativeSemanticProfile']);
    expect(gfm['specVersion'], '0.29-gfm');
    expect(gfm['precedence'], contains('sole semantic product authority'));
    final gfmCases = _listFromFile(gfm['fixture'] as String);
    expect(gfmCases, hasLength(gfm['fixtureCaseCount'] as int));
    _expectHash(gfm['fixture'] as String, gfm['fixtureSha256'] as String);

    final supplementPath = gfm['supplement'] as String;
    final supplement = _object(supplementPath);
    _expectHash(supplementPath, gfm['supplementSha256'] as String);
    final supplementedCases = _list(supplement['cases']);
    expect(
      supplementedCases.map((entry) => _map(entry)['example']),
      gfm['fixtureMissingExamples'],
    );

    final ids = <int>{
      for (final entry in gfmCases) _map(entry)['example'] as int,
      for (final entry in supplementedCases) _map(entry)['example'] as int,
    };
    expect(ids, hasLength(gfm['profileCaseCount'] as int));
    expect(ids, {for (var id = 1; id <= 672; id += 1) id});
    expect(gfm['extensions'], [
      'tables',
      'task_list_items',
      'strikethrough',
      'extended_autolinks',
      'disallowed_raw_html',
    ]);

    final compatibility = _list(profile['compatibilityProfiles']).map(_map);
    final commonmark = compatibility.single;
    expect(commonmark['id'], 'commonmark-0.31.2-compatibility');
    expect(commonmark['normativeForProduct'], isFalse);
    expect(commonmark['rule'], contains('never changes GFM pass or fail'));
    final commonmarkCases = _listFromFile(commonmark['fixture'] as String);
    expect(commonmarkCases, hasLength(commonmark['caseCount'] as int));
    expect(
      commonmarkCases.map((entry) => _map(entry)['example']),
      List<int>.generate(652, (index) => index + 1),
    );
    _expectHash(
      commonmark['fixture'] as String,
      commonmark['fixtureSha256'] as String,
    );
  });

  test('keeps flark-live-v1 independent from semantic conformance', () {
    final selected = _map(profile['liveProjectionProfile']);
    expect(selected['profileId'], 'flark-live-v1');
    expect(selected['rule'], contains('never counted as semantic GFM'));
    final manifestPath = selected['manifest'] as String;
    _expectHash(manifestPath, selected['manifestSha256'] as String);
    final manifest = _object(manifestPath);
    expect(manifest['profileId'], 'flark-live-v1');
    expect(manifest['semanticProfileId'], profile['profileId']);
    expect(manifest['separationRule'], contains('never changes GFM'));
    _expectHash(
      manifest['matrix'] as String,
      manifest['matrixSha256'] as String,
    );
  });

  test('keeps four denominator-owning ledgers separate', () {
    final statuses = _map(ledger['statusDefinitions']).keys.toSet();
    expect(statuses, {'exact', 'missing', 'divergent', 'approved_deviation'});

    final ledgers = {
      for (final entry in _list(ledger['ledgers']))
        _map(entry)['id'] as String: _map(entry),
    };
    expect(ledgers.keys, {
      'commonmark_semantic',
      'gfm_semantic',
      'commonmark_incremental',
      'gfm_incremental',
    });

    for (final entry in ledgers.values) {
      final counts = _map(entry['counts']);
      expect(counts.keys.toSet(), statuses);
      expect(
        counts.values.cast<int>().fold<int>(0, (sum, value) => sum + value),
        entry['denominator'],
        reason: '${entry['id']} must account for its whole denominator',
      );
      if (entry['receiptSha256'] == null) {
        expect(
          counts['exact'],
          0,
          reason: '${entry['id']} cannot claim exact cases without a receipt',
        );
      }
    }

    expect(ledgers['gfm_semantic']!['claimRole'], 'normative');
    expect(ledgers['gfm_incremental']!['claimRole'], 'normative');
    expect(ledgers['commonmark_semantic']!['claimRole'], 'compatibility');
    expect(ledgers['commonmark_incremental']!['claimRole'], 'compatibility');
    expect(ledgers['gfm_semantic']!['counts'], {
      'exact': 572,
      'missing': 81,
      'divergent': 19,
      'approved_deviation': 0,
    });
    expect(
      ledgers['gfm_semantic']!['receiptSha256'],
      '138b04d9a4afbc85073425311d69512809a57ba73ea94bc06f3d7f9be557f19a',
    );
    expect(ledgers['commonmark_semantic']!['counts'], {
      'exact': 563,
      'missing': 77,
      'divergent': 12,
      'approved_deviation': 0,
    });
    expect(ledgers['commonmark_semantic']!['denominator'], 652);
    expect(ledgers['gfm_semantic']!['denominator'], 672);
  });

  test('enforces the reviewed deviation register against exact ledgers', () {
    final policy = _map(profile['deviationPolicy']);
    final registerPath = policy['register']! as String;
    expect(ledger['deviationRegister'], registerPath);
    _expectHash(registerPath, policy['registerSha256']! as String);

    final register = _object(registerPath);
    expect(register['schemaVersion'], 2);
    expect(register['profileId'], profile['profileId']);
    final deviations = _list(register['deviations']).map(_map).toList();
    final allowedStates = _list(
      policy['allowedStatuses'],
    ).cast<String>().toSet();
    expect(allowedStates, {'proposed', 'approved', 'retired'});
    final requiredFields = _list(
      policy['requiredFields'],
    ).cast<String>().toSet();
    expect(requiredFields, {
      'ledger',
      'corpus',
      'example',
      'owner',
      'reason',
      'targetMilestone',
      'reviewState',
    });
    expect(policy['rule'], contains('approved in-denominator register row'));

    final ledgers = {
      for (final entry in _list(ledger['ledgers']))
        _map(entry)['id'] as String: _map(entry),
    };
    final approvedByLedger = _validatedApprovedDeviationCounts(
      deviations: deviations,
      requiredFields: requiredFields,
      allowedStates: allowedStates,
      ledgers: ledgers,
    );

    for (final entry in ledgers.entries) {
      expect(
        _map(entry.value['counts'])['approved_deviation'],
        approvedByLedger[entry.key],
        reason:
            '${entry.key} approved_deviation must equal approved register rows',
      );
    }
    expect(deviations, isEmpty, reason: 'the active profile has no deviations');
  });

  test('rejects malformed, duplicate, and out-of-denominator deviations', () {
    final policy = _map(profile['deviationPolicy']);
    final requiredFields = _list(
      policy['requiredFields'],
    ).cast<String>().toSet();
    final allowedStates = _list(
      policy['allowedStatuses'],
    ).cast<String>().toSet();
    final ledgers = {
      for (final entry in _list(ledger['ledgers']))
        _map(entry)['id'] as String: _map(entry),
    };
    final valid = <String, Object?>{
      'ledger': 'commonmark_semantic',
      'corpus': 'commonmark',
      'example': 1,
      'owner': 'parser-team',
      'reason': 'synthetic validator exercise',
      'targetMilestone': 'M2',
      'reviewState': 'proposed',
    };
    Map<String, int> validate(List<Map<String, Object?>> rows) =>
        _validatedApprovedDeviationCounts(
          deviations: rows,
          requiredFields: requiredFields,
          allowedStates: allowedStates,
          ledgers: ledgers,
        );

    expect(validate([valid])['commonmark_semantic'], 0);
    expect(
      validate([
        {...valid, 'reviewState': 'approved'},
      ])['commonmark_semantic'],
      1,
    );

    final missingOwner = Map<String, Object?>.from(valid)..remove('owner');
    for (final rows in <List<Map<String, Object?>>>[
      [missingOwner],
      [
        {...valid, 'ledger': 'unknown'},
      ],
      [
        {...valid, 'corpus': 'gfm'},
      ],
      [
        {...valid, 'example': 0},
      ],
      [
        {...valid, 'example': 653},
      ],
      [
        {...valid, 'reviewState': 'rubber_stamped'},
      ],
      [
        {...valid, 'reason': ''},
      ],
      [valid, Map<String, Object?>.from(valid)],
    ]) {
      expect(() => validate(rows), throwsFormatException);
    }
  });

  test('does not launder the v3 probe inventory into semantic conformance', () {
    final v3 = _object('test/fixtures/commonmark/v3_coverage_ledger.json');
    final classifications = _list(v3['classifications']);
    final counted = classifications.fold<int>(
      0,
      (sum, entry) => sum + (_map(entry)['expectedCount'] as int),
    );
    expect(counted, 652);
    expect(
      classifications
          .map((entry) => _map(entry)['status'])
          .contains('unclassified'),
      isTrue,
    );
    expect(
      _map(
        _list(
          ledger['ledgers'],
        ).singleWhere((entry) => _map(entry)['id'] == 'commonmark_semantic'),
      )['counts'],
      isNot(equals({'exact': 60, 'missing': 592})),
    );
  });
}

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

List<Object?> _listFromFile(String path) =>
    _list(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();

void _expectHash(String path, String expected) {
  expect(sha256.convert(File(path).readAsBytesSync()).toString(), expected);
}

Map<String, int> _validatedApprovedDeviationCounts({
  required List<Map<String, Object?>> deviations,
  required Set<String> requiredFields,
  required Set<String> allowedStates,
  required Map<String, Map<String, Object?>> ledgers,
}) {
  final approvedByLedger = {for (final id in ledgers.keys) id: 0};
  final registeredCases = <String>{};
  for (final deviation in deviations) {
    final missing = requiredFields.difference(deviation.keys.toSet());
    if (missing.isNotEmpty) {
      throw FormatException('deviation is missing ${missing.join(', ')}');
    }
    final ledgerId = deviation['ledger'];
    if (ledgerId is! String || !ledgers.containsKey(ledgerId)) {
      throw const FormatException('unknown deviation ledger');
    }
    final namedLedger = ledgers[ledgerId]!;
    final expectedCorpus = ledgerId.startsWith('commonmark_')
        ? 'commonmark'
        : 'gfm';
    final corpus = deviation['corpus'];
    if (corpus != expectedCorpus) {
      throw const FormatException('deviation corpus does not match ledger');
    }
    final example = deviation['example'];
    if (example is! int ||
        example < 1 ||
        example > (namedLedger['denominator']! as int)) {
      throw const FormatException('deviation is outside ledger denominator');
    }
    final key = '$ledgerId:$corpus:$example';
    if (!registeredCases.add(key)) {
      throw const FormatException('duplicate deviation ledger case');
    }
    final reviewState = deviation['reviewState'];
    if (reviewState is! String || !allowedStates.contains(reviewState)) {
      throw const FormatException('invalid deviation review state');
    }
    for (final field in ['owner', 'reason', 'targetMilestone']) {
      final value = deviation[field];
      if (value is! String || value.trim().isEmpty) {
        throw FormatException('deviation $field must be nonempty');
      }
    }
    if (reviewState == 'approved') {
      approvedByLedger[ledgerId] = approvedByLedger[ledgerId]! + 1;
    }
  }
  return approvedByLedger;
}
