# Releasing

The four packages are released with [rk](https://github.com/danReynolds/release-kit)
from a clean checkout of `main`. `release.toml` lists them in dependency
order: `flark`, `flark_codemirror`, `flark_flutter`, `flark_fleury`. rk
publishes git tags and pub.dev versions only after it has staged and checked
everything private, and asks for one yes per package before the first
permanent step. pub.dev never deletes a version; it can only retract one.

## 1. Prepare the release commit

- Bump each package's `version`, the constraints its dependents declare
  (`flark: ^0.5.0`), and add a `CHANGELOG.md` entry per package.
- `flark_fleury` needs the fleury and fleury_widgets versions it declares live
  on pub.dev. Remove the root pubspec's `dependency_overrides` once they are.
- Merge to `main` and let CI pass. The `windows-kernel` job of that run uploads
  the Windows parser libraries as the `flark_parse-windows` artifact.

`rk status` lists everything that still blocks a package.

## 2. Publish the parser libraries

The published `flark` package downloads its native parser at build time, from
a GitHub release, and checks it against SHA-256s pinned in
`packages/flark/hook/prebuilt.json`. Publish new libraries whenever the parse
crate changed since the last pinned release.

From a clean checkout of the release commit, on a Mac with Xcode, the Android
NDK and Docker:

```sh
ANDROID_HOME=~/Library/Android/sdk WINDOWS_RUN=<CI run id> \
  native/flark_parse/tool/build_release_libraries.sh dist/flark_parse
gh release create flark_parse-v0.5.0 dist/flark_parse/assets/* \
  --title "flark_parse 0.5.0" --notes "Parser libraries for flark 0.5.0."
native/flark_parse/tool/write_prebuilt_manifest.py dist/flark_parse/assets flark_parse-v0.5.0
```

Apple and Android libraries build locally; Linux builds in Docker on an older
glibc; Windows comes from the CI artifact of the same commit. Never replace an
asset of a published parser release: a new build gets a new tag, because
published `flark` versions pin their hashes.

Commit `hook/prebuilt.json` to `main`. Only that file changes, so the pinned
libraries still match the crate source they were built from.

## 3. Release the packages

```sh
rk status
rk release
```

rk releases unfinished packages in order: tag, then pub.dev. Run it from a
terminal so it can ask; `rk release <package>` releases one.
