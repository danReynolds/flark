# Releasing

The four packages are released with [rk](https://github.com/danReynolds/release-kit)
from a clean checkout of `main`. `release.toml` lists them in dependency
order: `flark`, `flark_codemirror`, `flark_flutter`, `flark_fleury`. rk
publishes git tags and pub.dev versions only after it has staged and checked
everything private. It asks for one yes per package before the first permanent
step. pub.dev never deletes a version; it can only retract one.

Two requirements for rk:

- **A version with the graph-aware override check.** The workspace root pins
  Fleury by Git for `flark_fleury` alone. Earlier rk versions refuse every
  package because of that pin. The check is on release-kit's
  `danreynolds/flark-dogfood` branch until it ships in an rk release.
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
- `flark_fleury` needs the fleury and fleury_widgets versions it declares live
  on pub.dev. Until then, rk refuses to stage it, because the root's Git
  overrides reach its dependencies. Remove the root pubspec's
  `dependency_overrides` once those versions are live.
- Merge to `main` and let CI pass. That run's `windows-kernel` job does the
  following for the Windows parser libraries:
  - builds them with the C runtime linked in;
  - checks that they need no Visual C++ runtime;
  - runs the kernel tests against the x64 library;
  - uploads them as the `flark_parse-windows` artifact.

## 2. Build and publish the parser libraries

The published `flark` package downloads its native parser at build time from a
GitHub release, and checks it against the SHA-256s pinned in
`packages/flark/hook/prebuilt.json`.

- Publish new libraries whenever the parse crate has changed since the last
  pinned release.
- Never replace an asset of a published parser release: published `flark`
  versions pin its hashes. Turn on immutable releases for the repository so
  that GitHub enforces this.

From a clean checkout of the release commit, on a Mac with Xcode, the Android
NDK and Docker:

```sh
ANDROID_HOME=~/Library/Android/sdk WINDOWS_RUN=<CI run id for this commit> \
  native/flark_parse/tool/build_release_libraries.sh dist/flark_parse
gh release create flark_parse-v0.5.0 dist/flark_parse/assets/* \
  --target "$(git rev-parse HEAD)" \
  --title "flark_parse 0.5.0" --notes "Parser libraries for flark 0.5.0."
```

Where each library comes from:

- Apple and Android libraries build locally.
- Linux libraries build in Docker, on an older glibc.
- Windows libraries come from the CI run. The script refuses a run that is not
  a successful run of this commit.

`--target` tags the commit the libraries were built from. `dist/` is ignored by
Git.

## 3. Pin them

```sh
git fetch origin tag flark_parse-v0.5.0
native/flark_parse/tool/write_prebuilt_manifest.py flark_parse-v0.5.0 dist/flark_parse/assets
packages/flark/tool/verify_download_consumer.sh --pinned
```

The manifest writer downloads what the release serves. It checks that those
bytes are what was built, and that the parse crate is unchanged since the tag.
Then it writes `hook/prebuilt.json`. The consumer check builds an app with no
Rust on `PATH`, which downloads this machine's library from the release.

Retitle `flark`'s "Unreleased" changelog entry to "0.5.0". Then commit the
manifest and the changelog to `main`, through CI.

## 4. Release the packages

```sh
rk status
rk release
```

rk releases the unfinished packages in order: tag, then pub.dev. Run it from a
terminal so that it can ask; `rk release <package>` releases one package.

`rk status` reports what is published, what each package waits for, and
problems such as a missing changelog entry. Pub's own validation and the
override check run only when a package is staged. `rk release <package>
--stage` runs them without publishing.
