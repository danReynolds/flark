import 'package:flark/flark_advanced.dart';
import 'package:flark/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart'
    show debugRequiredDefaultBackendResolver;
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('FlarkFlutterController routes default-backend load failures', () {
    const loadError = NativeComrakBridgeLoadException(
      kind: NativeComrakBridgeLoadFailureKind.libraryNotFound,
      message: 'test: native comrak bridge unavailable',
      platform: 'test',
    );

    setUp(() {
      // Make the lazily resolved default backend fail as if the native bridge
      // could not load — a failure the host cannot otherwise reproduce.
      debugRequiredDefaultBackendResolver = () => throw loadError;
    });
    tearDown(() {
      debugRequiredDefaultBackendResolver = null;
    });

    FlarkFlutterController controllerReporting(List<Object> errors) {
      // The default backend is resolved lazily (not at construction), so this
      // builds fine even with the failing resolver installed.
      return FlarkFlutterController(
        runtime: FlarkEditorRuntime.fromMarkdown('# hi'),
        onParseError: (error, _) => errors.add(error),
      );
    }

    test('parseNow reports the error instead of throwing', () async {
      final errors = <Object>[];
      final controller = controllerReporting(errors);
      addTearDown(controller.dispose);

      await controller.parseNow(); // completes, does not throw

      expect(errors, [same(loadError)]);
    });

    test('ensureParsing reports the error instead of throwing', () {
      final errors = <Object>[];
      final controller = controllerReporting(errors);
      addTearDown(controller.dispose);

      expect(controller.ensureParsing, returnsNormally);

      expect(errors, [same(loadError)]);
    });

    test('tryParseSync reports the error and returns false', () {
      final errors = <Object>[];
      final controller = controllerReporting(errors);
      addTearDown(controller.dispose);

      expect(controller.tryParseSync(), isFalse);

      expect(errors, [same(loadError)]);
    });
  });
}
