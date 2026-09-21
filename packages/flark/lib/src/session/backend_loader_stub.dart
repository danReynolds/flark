import 'backend_loader.dart';

Future<FlarkBackendLease> load() => Future.error(
  UnsupportedError('Flark requires a native Dart or JavaScript host.'),
);
