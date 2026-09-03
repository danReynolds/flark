import 'dart:io';

Future<Map<String, Object>> dogfoodMeasurementHostIdentity() async {
  final physicalMemory = int.tryParse(
    await _command('sysctl', const ['-n', 'hw.memsize']),
  );
  return {
    'hostname': Platform.localHostname,
    'operatingSystem': Platform.operatingSystemVersion,
    'architecture': await _command('uname', const ['-m']),
    'logicalCores': Platform.numberOfProcessors,
    'physicalMemoryBytes': physicalMemory ?? 1,
  };
}

Future<Map<String, Object>> dogfoodHostIdentity() async => {
  ...await dogfoodMeasurementHostIdentity(),
  'cpu': await _command('sysctl', const ['-n', 'machdep.cpu.brand_string']),
  'flutterVersion': await _command('flutter', const ['--version']),
  'dartVersion': await _command('dart', const ['--version']),
  'rustcVersion': await _command('rustc', const ['--version']),
  'cargoVersion': await _command('cargo', const ['--version']),
  'xcodeVersion': await _command('xcodebuild', const ['-version']),
};

Future<String> _command(String executable, List<String> arguments) async {
  final result = await Process.run(executable, arguments);
  if (result.exitCode != 0) {
    throw StateError(
      '$executable ${arguments.join(' ')} failed: '
      '${(result.stderr as String).trim()}',
    );
  }
  return (result.stdout as String).trim();
}
