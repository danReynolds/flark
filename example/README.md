# Flark Example

This Flutter app is the package's mobile integration harness. It depends on the
local package through `path: ..`, imports only
`package:flark/flark.dart`, and exercises both the
editable editor and read-only preview surfaces.

Run it locally:

```bash
cd example
flutter run
```

Verify Android native packaging:

```bash
../scripts/verify_example_packaging.sh --android
```

That command builds the debug APK through Gradle and fails unless the packaged
APK contains `lib/**/libsovereign_comrak_bridge.so`.

Verify iOS project wiring:

```bash
../scripts/verify_example_packaging.sh --ios
```

The iOS harness links the package XCFramework from
`../native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework` and
builds `Runner/FlarkComrakAnchor.c` into the app target so the static bridge
symbols remain visible to `DynamicLibrary.process()`.

Build the iOS XCFramework before a real device or simulator build:

```bash
../scripts/build_comrak_ios.sh
```
