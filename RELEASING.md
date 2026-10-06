# Releasing

Everything is released with [rk](https://github.com/danReynolds/release-kit)
from a clean checkout of `main`, on this machine. `release.toml` lists the
units in release order: `parser`, the native parser libraries, then the four
packages, `flark`, `flark_codemirror`, `flark_flutter` and `flark_fleury`. rk
publishes tags, the libraries' GitHub release and pub.dev versions only after
it has staged and checked everything private. A run of several units shows
what each will publish and asks once, before any of them acts. pub.dev never
deletes a version; it can only retract one.

Two requirements for rk:

- **A version with what this release relies on.** The `parser` unit publishes
  what its own build script writes, and one `rk release` has to pass over
  `parser`, released a commit before the packages. Both are on rk's `main`
  after 0.1.13, until they ship in an rk release.
- **The Flutter SDK's `dart`.** Put Flutter's `bin` first on `PATH`. The
  workspace has Flutter packages, which only a Flutter SDK's pub resolves, so rk
  refuses a standalone Dart SDK.

## 1. Prepare the release commit

- Bump each package's `version` and the constraints its dependents declare
  (`flark: ^0.5.0`). Add a `CHANGELOG.md` entry per package.
- Keep `flark`'s entry titled "Unreleased" until its parser libraries are
  pinned (step 3). CI fails a `flark` changelog that names the version while
  `hook/prebuilt.json` pins nothing. rk refuses to release `flark` without that
  entry. Together they keep an unpinned `flark` off pub.dev.
- `flark_fleury` depends on fleury 0.1 from pub.dev, where it is published.
  The root pubspec overrides nothing; keep it that way, since rk refuses to
  stage a package that an override reaches.
- Merge to `main` and let CI pass. That run's `windows-kernel` job does the
  following for the Windows parser libraries:
  - builds them with the C runtime linked in;
  - checks that they need no Visual C++ runtime;
  - runs the kernel tests against the x64 library;
  - uploads them as the `flark_parse-windows` artifact.

## 2. Release the parser libraries

The published `flark` package downloads its native parser at build time from a
GitHub release, and checks it against the SHA-256s pinned in
`packages/flark/hook/prebuilt.json`.

- Release new libraries whenever the parse crate has changed since the last
  pinned release. The crate's version in `native/flark_parse/Cargo.toml` names
  the release, so bump it and add an entry to `native/flark_parse/CHANGELOG.md`.
- Never replace an asset of a published parser release: published `flark`
  versions pin its hashes. Turn on immutable releases for the repository so
  that GitHub enforces this.

On a Mac with Xcode, the Android NDK and Docker running:

```sh
rk release parser
```

rk runs `native/flark_parse/tool/build_release_libraries.sh` from a clean copy
of the release commit, and publishes the files it writes as the GitHub release
`flark_parse-v<crate version>`, tagged at that commit:

- Apple and Android libraries build locally. The script finds Android Studio's
  SDK unless `ANDROID_HOME` names another.
- Linux libraries build in Docker, on an older glibc.
- Windows libraries come from the `windows-kernel` job of this commit's CI
  run on `main`, which the script finds, so let CI finish first. It refuses a
  pull request's run, which builds the pull request merged into `main` rather
  than the commit itself.

`rk stage parser` builds and checks the libraries without publishing them.

## 3. Pin them

```sh
git fetch origin tag flark_parse-v0.1.0
native/flark_parse/tool/write_prebuilt_manifest.py flark_parse-v0.1.0
packages/flark/tool/verify_download_consumer.sh --pinned
```

The manifest writer downloads what the release serves. It checks that the
parse crate is unchanged since the tag, then writes `hook/prebuilt.json`. That
file records each library's SHA-256, the tag's commit, and a digest of the
crate and toolchain. The consumer check builds an app with no Rust on `PATH`,
which downloads this machine's library from the release.

Retitle `flark`'s "Unreleased" changelog entry to "0.5.0". Then commit the
manifest and the changelog to `main`, through CI. CI fails a commit whose
changelog has an entry for the version when the parse crate or
`rust-toolchain.toml` differs from what the pinned release was built from. A
later `flark` release can reuse the pin only while the parser is unchanged.
After a release, bump `flark`'s version when you reopen its "Unreleased"
entry. rk does not look at CI, so on the release commit run the same check
before step 4:

```sh
cd packages/flark && dart test test/prebuilt_manifest_test.dart
```

## 4. Release the packages

```sh
rk status
rk release
```

rk passes over `parser`: its tag is on the commit before the pin, and the
crate has not changed since. It shows what the four packages will publish and
asks once. Then it tags and publishes each package in order, checking each
again before it acts. Run it from a terminal so that rk can ask. A package
asks again, and says why, if those checks find something the question did
not show, or if rk warns about it, as it does for Pub's validation warnings.
If a package stops, the ones before it stay published. Fix the problem and
run `rk release` again to carry on.

`rk status` reports what is published, what each package waits for, and
problems such as a missing changelog entry. Pub's own validation and the
override check run only when a package is staged. `rk stage <package>` runs
them without publishing.
