import 'package:flark/flark_adapter.dart';

import 'flark_default_parse_backend_stub.dart'
    if (dart.library.js_interop) 'flark_default_parse_backend_web.dart'
    as implementation;

/// Resolves the parser backend supplied by the Flutter adapter.
///
/// Native platforms use Flark's normal native-asset resolution. Web loads the
/// packaged module through Flutter's asset bundle and passes bytes into the
/// Dart engine's runtime-neutral bridge contract.
FlarkNativeComrakParseBackend flarkDefaultParseBackend() {
  return implementation.flarkDefaultParseBackend();
}
