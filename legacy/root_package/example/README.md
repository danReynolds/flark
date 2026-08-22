# Flark Example And Web Site

This Flutter app is both the package integration harness and the GitHub Pages
site. It depends on the local `flark_flutter` package, which in turn depends on
the root `flark` engine, imports only
`package:flark_flutter/flark_flutter.dart`, and exercises the editable editor,
read-only preview, toolbar commands, docs examples, and package feature
breakdown.

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

Verify iOS packaging:

```bash
../scripts/verify_example_packaging.sh --ios
```

iOS ships through native assets like every other platform: flark's build hook
(`hook/build.dart`) compiles the bridge to a `.dylib` during `flutter build` /
`flutter run`, and Flutter bundles it as `flark_comrak_bridge.framework`. There
is no XCFramework to prebuild and no `FlarkComrakAnchor.c` to link — the check
asserts that manual wiring is absent and that the hook declares the iOS build.

> Requires Flutter ≥ 3.44 for `flutter run -d ios`
> ([flutter/flutter#180603](https://github.com/flutter/flutter/issues/180603));
> `flutter build ios` works on older Flutter too.
