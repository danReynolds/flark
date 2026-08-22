import 'package:flark/flark_v3.dart';

Future<FlarkV3DocumentRuntime> openFlarkV3PublicRuntimeForTest(
  String markdown,
) => FlarkV3DocumentRuntime.open(
  markdown,
  webAssets: FlarkV3WebRuntimeAssets.packageDefaults(),
);
