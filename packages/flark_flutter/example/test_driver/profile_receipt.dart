import 'frame_gate.dart';

/// Operations each frame-profile site measures, 100 linked samples apiece.
/// The start site also inserts rows (Enter, a multi-line paste) near the
/// document start and removes them with Undo.
const profileSiteOperations = {
  'start': ['insert', 'delete', 'enter', 'undo enter', 'paste', 'undo paste'],
  'largest block': ['insert', 'delete'],
  'end': ['insert', 'delete'],
};

/// Validate full qualification runs, not deliberately filtered diagnostics.
/// The integration driver can report success when setUpAll failed before any
/// widget test ran, so its success flag alone is insufficient. Gates are
/// recomputed from the linked samples rather than trusted from summaries.
void validateProfileReceipt(Map<String, dynamic>? data) {
  void require(bool condition, String reason) {
    if (!condition) throw StateError('Invalid native profile receipt: $reason');
  }

  bool linked(Object? sample, String command) =>
      sample is Map &&
      [
        'startUs',
        command,
        'buildUs',
        'rasterUs',
        'vsyncStartUs',
        'latencyUs',
      ].every((field) => sample[field] is int);

  require(data != null && data.isNotEmpty, 'no measurements');
  final report = data!;
  if (report['receipts'] case final List receipts) {
    require(receipts.length == 24, 'expected all 24 frame cases');
    final cases = <String>{};
    for (final value in receipts) {
      require(value is Map, 'malformed frame case');
      final receipt = value as Map;
      require(
        receipt['nativeSemantics'] == true &&
            receipt['lifecycle'] == 'resumed' &&
            receipt['bounded'] == true &&
            receipt['productionWorkbench'] == true,
        'frame case was not a native production workload',
      );
      final site = receipt['site'];
      final operations = profileSiteOperations[site];
      require(operations != null, 'unknown frame site $site');
      cases.add('${receipt['shape']}:$site');
      final raw = receipt['raw'];
      require(
        raw is List &&
            raw.length == 100 * operations!.length &&
            receipt['samples'] == raw.length,
        'expected 100 linked samples per operation at $site',
      );
      require(
        (raw as List).every((sample) => linked(sample, 'commandUs')),
        'frame samples must link command, frame and vsync times',
      );
      final displayHz = receipt['displayHz'];
      require(displayHz is num && displayHz > 0, 'missing display rate');
      for (final operation in operations!) {
        final group = [
          for (final sample in raw.cast<Map>())
            if (sample['operation'] == operation) sample,
        ];
        require(group.length == 100, 'expected 100 $operation samples');
        final failures = FrameGateResult(
          group,
          displayHz as num,
          budgetUs: frameBudgetUs,
        ).failures('${receipt['shape']} $site $operation', nextFrame: true);
        require(failures.isEmpty, failures.join('; '));
      }
    }
    require(cases.length == 24, 'duplicate frame cases');
    return;
  }

  require(
    report['productionWorkbench'] == true && report['nativeSemantics'] == true,
    'not a native production workbench',
  );
  require(
    report['failures'] is List && (report['failures'] as List).isEmpty,
    'workbench reported failures',
  );
  final summaries = report['summaries'];
  final samples = report['samples'];
  require(summaries is List && summaries.length == 90, 'expected 90 groups');
  require(
    samples is List && samples.length == 3732,
    'expected all 3732 linked workbench samples',
  );
  var opens = 0, sustained = 0;
  final groups = <String, List<Map>>{};
  for (final sample in samples as List) {
    require(linked(sample, 'actionUs'), 'malformed workbench sample');
    final map = sample as Map;
    final operation = map['operation'];
    if (operation == 'open') opens++;
    if (operation == 'sustained insert' || operation == 'sustained delete') {
      sustained++;
    }
    (groups['${map['shape']}: $operation'] ??= []).add(map);
  }
  require(opens == 100 && sustained == 3000, 'incomplete workbench workload');
  final displayHz = report['displayHz'];
  require(displayHz is num && displayHz > 0, 'missing display rate');
  for (final MapEntry(key: group, value: members) in groups.entries) {
    final budget = members.first['budgetUs'];
    require(budget is int, 'missing budget for $group');
    final failures = FrameGateResult(
      members,
      displayHz as num,
      budgetUs: budget as int,
    ).failures(group, nextFrame: budget == frameBudgetUs);
    require(failures.isEmpty, failures.join('; '));
  }
}
