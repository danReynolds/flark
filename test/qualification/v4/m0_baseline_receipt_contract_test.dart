@Tags(<String>['historical-receipt'])
library;

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

const _receiptPath = 'benchmark/v4/m0_baseline_receipt_2026-08-08.json';

void main() {
  final rawReceipt = File(_receiptPath).readAsStringSync();
  final receipt = _map(jsonDecode(rawReceipt));

  test('is an incomplete non-claim receipt over the exact planning base', () {
    expect(receipt['schemaVersion'], 1);
    expect(receipt['receiptId'], 'flark-v4-m0-baseline-2026-08-08');
    expect(receipt['status'], 'INCOMPLETE');
    expect(receipt['m0Complete'], isFalse);
    expect(receipt['claimEligible'], isFalse);

    final planningBase = _object(receipt, 'planningBase');
    expect(planningBase['commit'], '47692297661489bcbc2a2af4574a6a422cf68ef7');
    final worktree = _object(receipt, 'observedWorktree');
    expect(worktree['headMatchesPlanningBase'], isTrue);
    expect(worktree['clean'], isFalse);
    expect(worktree['immutableRevisionReceipt'], isFalse);

    final conclusion = _object(receipt, 'conclusion');
    expect(conclusion['m0Complete'], isFalse);
    expect(conclusion['releaseReady'], isFalse);
    expect(conclusion['profileClaimReady'], isFalse);
    expect(conclusion['conformanceClaimReady'], isFalse);
  });

  test('keeps private device identifiers out of the machine receipt', () {
    final lower = rawReceipt.toLowerCase();
    for (final forbidden in const [
      'serial number',
      'hardware uuid',
      'provisioning udid',
    ]) {
      expect(lower, isNot(contains(forbidden)));
    }

    final host = _object(receipt, 'host');
    expect(host['privacy'], 'anonymized-no-device-identifiers');
    expect(host['chip'], 'Apple M1 Pro');
    expect(host['memoryGB'], 16);
    expect(_object(host, 'cpuCores'), {
      'total': 10,
      'performance': 8,
      'efficiency': 2,
    });
  });

  test('records repairs without hiding the reopened WASM blocker', () {
    final blockers = {
      for (final raw in _list(receipt, 'originalBaselineBlockers'))
        _map(raw)['id'] as String: _map(raw),
    };
    expect(blockers.keys, {
      'stale_wasm',
      'missing_expected_structural_ack',
      'rustfmt_drift',
      'flutter_avoid_print',
    });
    expect(blockers['stale_wasm']!['currentValidation'], 'FAIL_REOPENED');
    expect(
      blockers['missing_expected_structural_ack']!['currentValidation'],
      'PASS',
    );
    expect(blockers['rustfmt_drift']!['currentValidation'], 'PASS');
    expect(
      blockers['flutter_avoid_print']!['currentValidation'],
      'PASS_TARGETED_ONLY',
    );

    final checks = _checks(receipt);
    expect(checks['flark_parser_lib']!['status'], 'PASS');
    expect(checks['flark_parser_lib']!['passed'], 170);
    expect(checks['flark_parser_lib']!['failed'], 0);
    expect(checks['commonmark_structural_admission']!['passed'], 5);
    expect(checks['root_dart_analysis']!['status'], 'PASS');
    expect(checks['input_window_contract']!['passed'], 3);
    expect(checks['competitor_baseline_contract']!['passed'], 5);
    expect(checks['wasm_freshness']!['status'], 'FAIL');
  });

  test('keeps every heavyweight unrun gate explicitly pending', () {
    final pending = {
      for (final raw in _list(receipt, 'pendingChecks'))
        _map(raw)['id'] as String: _map(raw),
    };
    expect(pending.keys, {
      'full_rust_workspace',
      'native_editor_ci_and_packaging',
      'package_confidence',
      'publish_archives',
      'mac_profile_run',
      'competitor_profile_protocol',
      'full_flutter_analysis',
      'full_flutter_tests',
      'full_flutter_build',
    });
    expect(
      pending.values.map((entry) => entry['status']),
      everyElement('PENDING'),
    );
    expect(
      pending.values.map((entry) => entry['command']),
      everyElement(isNotEmpty),
    );
  });

  test('pins every listed input to its current SHA-256', () {
    final inputs = _list(receipt, 'immutableInputs').map(_map).toList();
    expect(inputs, hasLength(13));
    expect(
      inputs.map((entry) => entry['path']).toSet(),
      hasLength(inputs.length),
    );

    for (final input in inputs) {
      final path = input['path']! as String;
      final file = File(path);
      expect(file.existsSync(), isTrue, reason: 'missing receipt input $path');
      expect(
        sha256.convert(file.readAsBytesSync()).toString(),
        input['sha256'],
        reason: '$path drifted without a new baseline receipt',
      );
    }
  });

  test('records matching WASM copies without claiming source freshness', () {
    final wasm = _object(receipt, 'wasmAssetState');
    final root = File(wasm['rootPath']! as String);
    final flutter = File(wasm['flutterPath']! as String);
    final rootHash = sha256.convert(root.readAsBytesSync()).toString();
    final flutterHash = sha256.convert(flutter.readAsBytesSync()).toString();
    expect(rootHash, wasm['rootSha256']);
    expect(flutterHash, wasm['flutterSha256']);
    expect(rootHash, flutterHash);
    expect(wasm['assetsByteIdentical'], isTrue);
    expect(wasm['sourceFreshness'], 'FAIL');
  });

  test('keeps structural admission outside four exact ledgers', () {
    final claims = _object(receipt, 'markdownClaims');
    final structural = _object(claims, 'structuralAdmission');
    expect(structural['admitted'], 652);
    expect(structural['denominator'], 652);
    expect(structural['claimBoundary'], contains('neither semantic'));

    final ledgers = {
      for (final raw in _list(claims, 'denominatorOwningLedgers'))
        _map(raw)['id'] as String: _map(raw),
    };
    expect(ledgers.keys, {
      'commonmark_semantic',
      'gfm_semantic',
      'commonmark_incremental',
      'gfm_incremental',
    });
    expect(_object(ledgers['commonmark_semantic']!, 'counts'), const {
      'exact': 384,
      'missing': 262,
      'divergent': 6,
      'approved_deviation': 0,
    });
    expect(_object(ledgers['gfm_semantic']!, 'counts'), const {
      'exact': 0,
      'missing': 672,
      'divergent': 0,
      'approved_deviation': 0,
    });
    expect(_object(ledgers['commonmark_incremental']!, 'counts'), const {
      'exact': 0,
      'missing': 652,
      'divergent': 0,
      'approved_deviation': 0,
    });
    expect(_object(ledgers['gfm_incremental']!, 'counts'), const {
      'exact': 0,
      'missing': 672,
      'divergent': 0,
      'approved_deviation': 0,
    });

    for (final ledger in ledgers.values) {
      final counts = _object(ledger, 'counts').values.cast<int>();
      expect(
        counts.fold<int>(0, (sum, count) => sum + count),
        ledger['denominator'],
        reason: '${ledger['id']} must own its whole denominator',
      );
    }

    final v3 = _object(claims, 'v3ProbeInventory');
    expect(v3['authoritative_supported_probe'], 60);
    expect(v3['intentional_fail_closed'], 19);
    expect(v3['intentional_extension_divergence'], 2);
    expect(v3['unclassified'], 571);
  });

  test('retains G2 and G3 as historical non-pass evidence', () {
    final evidence = {
      for (final raw in _list(receipt, 'historicalNonPassEvidence'))
        _map(raw)['id'] as String: _map(raw),
    };
    expect(evidence.keys, {'G2', 'G3', 'M0_RANGE_CERTIFICATION'});
    expect(evidence['G2']!['status'], 'BLOCKED');
    expect(evidence['G2']!['completedConfigurations'], 0);
    expect(evidence['G2']!['frameTimingsProduced'], isFalse);

    expect(evidence['G3']!['status'], 'PARTIAL_NONPASS');
    final paste = _object(evidence['G3']!, 'pasteFailure');
    expect(paste['pumpAttempts'], 100000);
    expect(paste['exactCurrent'], isFalse);
    expect(paste['sourceIntact'], isTrue);
    expect(paste['terminalReasonReported'], isFalse);

    expect(evidence['M0_RANGE_CERTIFICATION']!['status'], 'PARTIAL_NONPASS');
  });
}

Map<String, Map<String, Object?>> _checks(Map<String, Object?> receipt) => {
  for (final raw in _list(receipt, 'targetedChecks'))
    _map(raw)['id'] as String: _map(raw),
};

Map<String, Object?> _object(Map<String, Object?> value, String key) =>
    _map(value[key]);

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Map<String, Object?> value, String key) =>
    (value[key] as List).cast<Object?>();
