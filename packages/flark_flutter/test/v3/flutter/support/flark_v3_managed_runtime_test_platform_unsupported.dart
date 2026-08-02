import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter_test/flutter_test.dart';

Future<FlarkV3DocumentRuntime> openManagedRuntimeForTest(String markdown) =>
    FlarkV3DocumentRuntime.open(markdown);

Future<T> runManagedRuntimeAsyncForTest<T>(
  WidgetTester tester,
  Future<T> Function() work,
) => work();
