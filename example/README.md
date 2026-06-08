# Flark Example And Web Site

This Flutter app is both the package integration harness and the GitHub Pages
site. It depends on the local package through `path: ..`, imports only
`package:flark/flark.dart`, and exercises the editable editor, read-only
preview, toolbar commands, docs examples, and package feature breakdown.

Run it locally:

```bash
cd example
flutter run
```

Build the web site locally:

```bash
cd example
flutter build web --release --base-href /flark/
```

Verify Android native packaging:

```bash
../scripts/verify_example_packaging.sh --android
```

That command builds the debug APK through Gradle and fails unless the packaged
APK contains `lib/**/libflark_comrak_bridge.so`.

Verify iOS project wiring:

```bash
../scripts/verify_example_packaging.sh --ios
```

The iOS harness links the package XCFramework from
`../native/comrak_bridge/dist/ios/flark_comrak_bridge.xcframework` and
builds `Runner/FlarkComrakAnchor.c` into the app target so the static bridge
symbols remain visible to `DynamicLibrary.process()`.

Build the iOS XCFramework before a real device or simulator build:

```bash
../scripts/build_comrak_ios.sh
```
