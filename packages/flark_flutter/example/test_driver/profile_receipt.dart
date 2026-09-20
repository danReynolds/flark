/// Validate full qualification runs, not deliberately filtered diagnostics.
/// The integration driver can report success when setUpAll failed before any
/// widget test ran, so its success flag alone is insufficient.
void validateProfileReceipt(Map<String, dynamic>? data) {
  void require(bool condition, String reason) {
    if (!condition) throw StateError('Invalid native profile receipt: $reason');
  }

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
      cases.add('${receipt['shape']}:${receipt['site']}');
      require(
        receipt['samples'] == 200 &&
            receipt['raw'] is List &&
            (receipt['raw'] as List).length == 200,
        'expected 200 linked samples per frame case',
      );
      for (final field in ['insertP99Us', 'deleteP99Us']) {
        final value = receipt[field];
        require(value is num && value > 0 && value < 16667, '$field failed');
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
  for (final sample in samples as List) {
    require(sample is Map, 'malformed workbench sample');
    final operation = (sample as Map)['operation'];
    if (operation == 'open') opens++;
    if (operation == 'sustained insert' || operation == 'sustained delete') {
      sustained++;
    }
  }
  require(opens == 100 && sustained == 3000, 'incomplete workbench workload');
}
