import 'dart:convert';
import 'dart:io';

import 'package:flark_dogfood/qualification.dart';
import 'package:flutter_test/flutter_test.dart';

import '../test_driver/frame_gate.dart';
import '../test_driver/profile_receipt.dart';

void main() {
  Map<String, dynamic> receipt(String name) =>
      jsonDecode(
            utf8.decode(
              gzip.decode(
                File(
                  '../../../docs/architecture/v5/receipts/'
                  'native-attended-2026-09-20/$name-driver.json.gz',
                ).readAsBytesSync(),
              ),
            ),
          )
          as Map<String, dynamic>;

  // The 2026-09-20 frame receipts predate per-sample command times and the
  // start-site row edits. This synthetic upgrade exercises the validator only:
  // each sample gets its case's command p99 (an upper bound), and start-site
  // row edits reuse the typed samples. It is not qualification evidence.
  Map<String, dynamic> upgradedFrames() {
    final frames = receipt('frames');
    for (final value in frames['receipts'] as List) {
      final r = value as Map<String, dynamic>;
      final typed = [
        for (final s in (r['raw'] as List).cast<Map<String, dynamic>>())
          {
            ...s,
            'operation': s['insert'] == 1 ? 'insert' : 'delete',
            'commandUs': r['commandP99Us'],
          },
      ];
      final operations = profileSiteOperations[r['site']]!;
      final raw = [
        for (final operation in operations)
          for (final s in typed.where(
            (s) =>
                s['operation'] ==
                (operation == 'insert' ||
                        operation == 'enter' ||
                        operation == 'paste'
                    ? 'insert'
                    : 'delete'),
          ))
            {...s, 'operation': operation},
      ];
      r['raw'] = raw;
      r['samples'] = raw.length;
    }
    return frames;
  }

  test('start-site operations match the harness edits', () {
    for (final site in profileSiteOperations.keys) {
      expect(profileSiteOperations[site], [
        for (final (name, _) in profileOperations(site)) name,
      ]);
    }
  });
  test('the captured workbench run passes the work gate', () {
    validateProfileReceipt(receipt('workbench-corrected'));
  });
  test('frame receipts from before the work gate are rejected', () {
    expect(() => validateProfileReceipt(receipt('frames')), throwsStateError);
  });
  test('accepts a complete frame receipt', () {
    validateProfileReceipt(upgradedFrames());
  });
  test('rejects the missing data from failed suite setup', () {
    expect(() => validateProfileReceipt(null), throwsStateError);
    expect(() => validateProfileReceipt({}), throwsStateError);
  });
  test('rejects partial and repeated frame cases', () {
    final partial = upgradedFrames();
    (partial['receipts'] as List).removeLast();
    expect(() => validateProfileReceipt(partial), throwsStateError);
    final duplicate = upgradedFrames();
    final cases = duplicate['receipts'] as List;
    cases[23] = cases[0];
    expect(() => validateProfileReceipt(duplicate), throwsStateError);
  });
  test('rejects a start site without its row edits', () {
    final frames = upgradedFrames();
    final start = (frames['receipts'] as List).firstWhere(
      (r) => r['site'] == 'start',
    );
    (start['raw'] as List).removeWhere((s) => s['operation'] == 'paste');
    start['samples'] = (start['raw'] as List).length;
    expect(() => validateProfileReceipt(frames), throwsStateError);
  });
  test('rejects over-budget work and missed frames, not slow vsyncs', () {
    List<Map> samples(Map<String, dynamic> frames) =>
        ((frames['receipts'] as List)[0]['raw'] as List).cast<Map>();

    final raster = upgradedFrames();
    for (final s in samples(raster).take(2)) {
      s['rasterUs'] = frameBudgetUs;
    }
    expect(() => validateProfileReceipt(raster), throwsStateError);

    final work = upgradedFrames();
    for (final s in samples(work).take(2)) {
      s['buildUs'] = frameBudgetUs;
    }
    expect(() => validateProfileReceipt(work), throwsStateError);

    final late = upgradedFrames();
    for (final s in samples(late).take(2)) {
      s['vsyncStartUs'] = (s['vsyncStartUs'] as int) + 20000;
    }
    expect(() => validateProfileReceipt(late), throwsStateError);

    // Latency alone no longer fails a run: it is mostly vsync alignment.
    final latency = upgradedFrames();
    for (final s in samples(latency)) {
      s['latencyUs'] = 3 * frameBudgetUs;
    }
    validateProfileReceipt(latency);
  });
  test('rejects truncated sustained workload and failed limits', () {
    final partial = receipt('workbench-corrected');
    (partial['samples'] as List).removeLast();
    expect(() => validateProfileReceipt(partial), throwsStateError);
    final missingEdit = receipt('workbench-corrected');
    final samples = missingEdit['samples'] as List;
    samples.removeAt(samples.indexWhere((s) => s['operation'] == 'delete'));
    expect(() => validateProfileReceipt(missingEdit), throwsStateError);
    final failed = receipt('workbench-corrected');
    failed['failures'] = ['peak RSS'];
    expect(() => validateProfileReceipt(failed), throwsStateError);
    final slow = receipt('workbench-corrected');
    for (final s in (slow['samples'] as List).cast<Map>().where(
      (s) => s['operation'] == 'sustained insert',
    )) {
      s['actionUs'] = frameBudgetUs;
    }
    expect(() => validateProfileReceipt(slow), throwsStateError);
  });
  test('rejects framework-only frame receipts', () {
    final noNative = upgradedFrames();
    noNative['receipts'][0]['nativeSemantics'] = false;
    expect(() => validateProfileReceipt(noNative), throwsStateError);
  });
}
