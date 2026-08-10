# M1 package identity rename manifest

**Status:** M0 execution contract. **Inventory commit:**
`47692297661489bcbc2a2af4574a6a422cf68ef7`. **Authority:**
[RFC 026](../../rfc/rfc_026_flark_v4_product_architecture.md) and the
[v4 build plan](../build_plan.md).

This manifest makes M1A and M1B mechanical identity changes. It is not a
license to rename every occurrence of `flark`, move package directories,
change exports, rebuild native artifacts, or alter runtime behavior. Each stage
must be one clean green commit, and M1B starts only after the M1A commit and its
archive receipts are accepted.

## 0. Package-name preflight

Immediately before M1 work, the exact pub.dev package API endpoints were
rechecked from this Mac at `2026-08-08T23:38:02Z`:

| Candidate | Endpoint | HTTP status |
| --- | --- | ---: |
| `flark` | `https://pub.dev/api/packages/flark` | 404 |
| `flark_core` | `https://pub.dev/api/packages/flark_core` | 404 |
| `flark_flutter` | `https://pub.dev/api/packages/flark_flutter` | 404 |

The command was `curl -sS -o /dev/null -w ...` against each exact endpoint.
This is evidence that pub.dev did not expose those package records at that
instant. It is not a reservation, ownership proof, or permission to skip the
publication-time recheck.

The inventory at the commit above contains 476 `package:flark/` occurrences in
221 active Dart files and 116 `package:flark_flutter/` occurrences in 100
active Dart files. Counts are navigation aids, not acceptance criteria: the
selectors and postconditions below are authoritative because files may be
added during a mechanical stage.

Reproduce the active-code inventory with these exact selectors. They exclude
historical Markdown and ignored/generated build state on purpose.

```sh
rg -n --glob '*.dart' -F 'package:flark/' \
  lib test tool hook example packages/flark_flutter
rg -n --glob '*.dart' -F 'package:flark_flutter/' \
  lib test tool hook example packages/flark_flutter
rg -n -F 'packages/flark/assets/' \
  lib test tool example packages/flark_flutter
rg -n \
  -e '/packages/flark_flutter/assets/' \
  -e 'assets/packages/flark_flutter/lib/assets/' \
  packages/flark_flutter/lib packages/flark_flutter/test \
  example/lib example/test tool/archive_consumer \
  scripts/verify_publish_archives.sh
rg -n '^name: (flark|flark_flutter)$|^  (flark|flark_flutter):' \
  pubspec.yaml packages/flark_flutter/pubspec.yaml \
  packages/flark_flutter/pubspec_overrides.yaml \
  example/pubspec.yaml example/pubspec_overrides.yaml \
  tool/archive_consumer/dart_consumer/pubspec.yaml \
  tool/archive_consumer/flutter_consumer/pubspec.yaml
```

## 1. Name classes

The same word currently names several different things. Apply a mapping only
to the matching class.

| Class | M1 treatment |
| --- | --- |
| Dart package identity and package URI | Rename in M1A: `flark` -> `flark_core` |
| Flutter package identity and package URI | Rename in M1B: `flark_flutter` -> `flark` |
| Public product name and `Flark*` Dart symbols | Keep |
| Rust crates `flark-engine` and `flark-parser` | Keep |
| Legacy Rust package, library, and ABI stem `flark_comrak_bridge` | Keep until its separately reviewed M2-M5 replacement |
| Repository, website, and Pages slug `flark` | Keep; the GitHub repository is not being renamed |
| Physical repository paths | Keep throughout M1; see the allowlist below |
| Historical evidence and captured commands | Keep verbatim |
| Package-resolution and generated asset namespaces | Regenerate from the new logical identities; never hand-edit generated output |

## 2. M1A - headless Dart identity

M1A changes the root package and every consumer of that package while the
Flutter package is still named `flark_flutter`.

| Surface | Before M1A | After M1A |
| --- | --- | --- |
| Root `pubspec.yaml` name | `flark` | `flark_core` |
| Root package URI prefix | `package:flark/` | `package:flark_core/` |
| Existing narrow core barrel | `package:flark/flark_core.dart` | `package:flark_core/flark_core.dart` |
| Existing supported barrel | `package:flark/flark.dart` | `package:flark_core/flark.dart` |
| Other root public libraries | `package:flark/<file>.dart` | `package:flark_core/<file>.dart` |
| Flutter package dependency | `flark: ^0.1.1` | `flark_core: ^0.1.1` |
| Flutter local override key | `flark:` | `flark_core:`; path remains `../..` |
| Example dependency/override | `flark:` | `flark_core:`; path remains `..` |
| Headless archive-consumer dependency/import | `flark` / `package:flark/` | `flark_core` / `package:flark_core/` |
| Root hosted archive | `flark.tar.gz` | `flark_core.tar.gz` |
| Hosted server/cache key | `flark` / `flark-$VERSION` | `flark_core` / `flark_core-$VERSION` |
| Root Web asset namespace | `packages/flark/assets/` and `/packages/flark/assets/` | `packages/flark_core/assets/` and `/packages/flark_core/assets/` |

The physical files `lib/flark.dart` and `lib/flark_core.dart` stay in place.
Their export targets, `show` lists, and `hide` lists must be identical before
and after M1A. Documentation and package-URI references may change; export-set
consolidation belongs to M3. The same rule applies to `lib/flark_adapter.dart`,
`lib/flark_advanced.dart`, and `lib/flark_v3.dart`.

M1A updates active imports/exports in root source and tests, tools, build hooks,
the example, the archive fixtures, and the still-physical
`packages/flark_flutter/` tree. It also updates current package documentation
and metadata where `flark` denotes the headless pub package. It does not rewrite
repository URLs such as `github.com/danReynolds/flark`, Pages base paths such as
`/flark/`, product copy, widget keys, temporary-file prefixes, or Rust names.

### M1A postconditions

Run these from the repository root after dependency resolution. Every negative
scan must print nothing.

```sh
test "$(sed -n 's/^name: //p' pubspec.yaml | head -n 1)" = flark_core
test "$(sed -n 's/^name: //p' packages/flark_flutter/pubspec.yaml | head -n 1)" = flark_flutter

rg -n '^  flark_core: \^0\.1\.1$' packages/flark_flutter/pubspec.yaml example/pubspec.yaml
rg -n '^  flark_core:$' packages/flark_flutter/pubspec_overrides.yaml example/pubspec_overrides.yaml

if rg -n --glob '*.dart' -F 'package:flark/' \
  lib test tool hook example packages/flark_flutter; then
  echo 'stale core package URI after M1A' >&2
  exit 1
fi

if rg -n -F 'packages/flark/assets/' \
  lib test tool/archive_consumer example packages/flark_flutter; then
  echo 'stale core asset namespace after M1A' >&2
  exit 1
fi

if rg -n '^  flark:' \
  pubspec.yaml packages/flark_flutter/pubspec.yaml \
  packages/flark_flutter/pubspec_overrides.yaml \
  example/pubspec.yaml example/pubspec_overrides.yaml \
  tool/archive_consumer/dart_consumer/pubspec.yaml \
  tool/archive_consumer/flutter_consumer/pubspec.yaml; then
  echo 'stale core dependency key after M1A' >&2
  exit 1
fi

if rg -n --glob '*.dart' "package:flutter/|dart:ui" lib hook; then
  echo 'Flutter leaked into flark_core' >&2
  exit 1
fi

if rg -n '^  flutter:' pubspec.yaml; then
  echo 'Flutter SDK dependency leaked into flark_core' >&2
  exit 1
fi
```

The `package:flark_flutter/` namespace remains valid during M1A. Do not combine
its removal with this commit.

## 3. M1B - Flutter product identity

M1B changes the nested package's logical identity while leaving its directory
in place.

| Surface | Before M1B | After M1B |
| --- | --- | --- |
| `packages/flark_flutter/pubspec.yaml` name | `flark_flutter` | `flark` |
| Product package URI prefix | `package:flark_flutter/` | `package:flark/` |
| Product entrypoint | `package:flark_flutter/flark_flutter.dart` | new `package:flark/flark.dart` |
| Retained migration barrel | `package:flark_flutter/flark_flutter.dart` | `package:flark/flark_flutter.dart` |
| Retained advanced barrel | `package:flark_flutter/flark_flutter_advanced.dart` | `package:flark/flark_flutter_advanced.dart` |
| Product engine dependency/imports | old root `flark` identity | explicit `flark_core` identity |
| Example product dependency/override | `flark_flutter:` | `flark:`; override path remains `../packages/flark_flutter` |
| Product archive | `flark_flutter.tar.gz` | `flark.tar.gz` |
| Hosted server/cache key | `flark_flutter` / `flark_flutter-$VERSION` | `flark` / `flark-$VERSION` |
| Product Web asset namespace | `/packages/flark_flutter/assets/` | `/packages/flark/assets/` |
| Flutter build asset path | `assets/packages/flark_flutter/lib/assets/` | `assets/packages/flark/lib/assets/` |

Create `packages/flark_flutter/lib/flark.dart` as a behavior-free relative
forwarder to `flark_flutter.dart`. Retain `flark_flutter.dart` and
`flark_flutter_advanced.dart` at their physical paths for migration
verification. The existing export sets remain unchanged except that engine
exports use `package:flark_core/`. Do not add a product self-import merely
because `flark` now names the product.

The repository example may retain an explicit `flark_core` dependency only for
files that directly import core-only APIs. The immutable product archive
consumer is stricter: its `pubspec.yaml` contains only a direct `flark`
dependency, its source imports only `package:flark/flark.dart`, and
`flark_core` appears only as a transitive package-config entry.

### M1B postconditions

Run these from the repository root. Every negative scan must print nothing.

```sh
test "$(sed -n 's/^name: //p' pubspec.yaml | head -n 1)" = flark_core
test "$(sed -n 's/^name: //p' packages/flark_flutter/pubspec.yaml | head -n 1)" = flark
test -f packages/flark_flutter/lib/flark.dart
rg -n '^  flark_core: \^0\.1\.1$' packages/flark_flutter/pubspec.yaml

if rg -n --glob '*.dart' -F 'package:flark_flutter/' \
  lib test tool hook example packages/flark_flutter; then
  echo 'stale Flutter product package URI after M1B' >&2
  exit 1
fi

if rg -n --glob '*.dart' "^(import|export) 'package:flark/" \
  packages/flark_flutter/lib; then
  echo 'product self-import or core import through the product identity' >&2
  exit 1
fi

rg -n --glob '*.dart' "^(import|export) 'package:flark_core/" \
  packages/flark_flutter/lib

if rg -n --glob '*.dart' -F 'package:flark/' \
  lib test hook tool/archive_consumer/dart_consumer; then
  echo 'core source or headless consumer imports the Flutter product' >&2
  exit 1
fi

if rg -n \
  -e '/packages/flark_flutter/assets/' \
  -e 'assets/packages/flark_flutter/lib/assets/' \
  packages/flark_flutter/lib packages/flark_flutter/test \
  example/lib example/test tool/archive_consumer \
  scripts/verify_publish_archives.sh; then
  echo 'stale logical Flutter asset namespace after M1B' >&2
  exit 1
fi

if rg -n -F 'packages/flark_flutter/lib/assets/' \
  packages/flark_flutter/lib/src/v2/flutter/flark_default_parse_backend_web.dart; then
  echo 'stale product package asset key after M1B' >&2
  exit 1
fi

if rg -n '^name: flark_flutter$|^  flark_flutter:' \
  packages/flark_flutter/pubspec.yaml \
  packages/flark_flutter/pubspec_overrides.yaml \
  example/pubspec.yaml example/pubspec_overrides.yaml \
  tool/archive_consumer/dart_consumer/pubspec.yaml \
  tool/archive_consumer/flutter_consumer/pubspec.yaml; then
  echo 'stale logical flark_flutter pub identity after M1B' >&2
  exit 1
fi

if rg -n 'flark_core|package:flark_core/' \
  tool/archive_consumer/flutter_consumer/pubspec.yaml \
  tool/archive_consumer/flutter_consumer/lib \
  tool/archive_consumer/flutter_consumer/test; then
  echo 'product-only consumer has a direct flark_core dependency or import' >&2
  exit 1
fi
```

`package:flark/` is expected in product consumers after M1B. Its presence is
not itself stale; the forbidden case is a product self-import or its use by
root/headless code.

## 4. Physical-path and historical-evidence allowlist

These names are intentionally not rewritten in M1. A match is allowed only
when it denotes the listed physical or historical thing, not when it is used as
a package URI, hosted-package key, or generated asset namespace.

| Preserved name/path | Reason |
| --- | --- |
| Repository root and repository slug `flark` | Product/repository brand, not the root pub identity |
| `packages/flark_flutter/` | Filesystem move is explicitly outside M1 |
| `packages/flark_flutter/lib/flark_flutter.dart` | Retained migration barrel |
| `packages/flark_flutter/lib/flark_flutter_advanced.dart` | Retained advanced barrel; only its package URI changes |
| `native/comrak_bridge/` | Legacy bridge replacement is later work |
| Rust names `flark-engine`, `flark-parser`, `flark_comrak_bridge` and `libflark_comrak_bridge.*` | No Rust or ABI identity change in M1 |
| `lib/flark.dart`, `lib/flark_core.dart`, `lib/flark_adapter.dart`, `lib/flark_advanced.dart`, `lib/flark_v3.dart` | Existing root library filenames stay; package URI prefix changes |
| Physical `packages/flark_flutter` roots in `build_comrak_wasm.sh`, `verify_benchmark_lane.sh`, `verify_native_editor_ci.sh`, `verify_package_confidence.sh`, `verify_publish_archives.sh`, `verify_release.sh`, and `verify_web_adapter_ci.sh` | Script navigation follows the unchanged directory; logical archive names and asset URLs in `verify_publish_archives.sh` still change |
| `tool/gen_wasm_buildinfo.dart` staging destination under `packages/flark_flutter/lib/assets/` | Generated files are still copied to the unchanged physical directory |
| Physical asset assertions in `test/v2/packaging/flark_wasm_freshness_test.dart` and `flark_v2_native_packaging_contract_test.dart` | They verify checkout staging paths; logical Web URL assertions in the latter still change |
| Test and golden filenames, `Flark*` symbols, widget keys, fixture IDs, and product prose | Product identity, not package-resolution metadata |
| GitHub URLs, Pages `/flark/` base path, and `flark.dev` fixture URLs | Repository/site identity or fixture data |
| `/tmp/flark-*` names and benchmark labels | Diagnostic labels, not package identities |

Do not bulk-rewrite captured evidence. In particular, preserve old commands,
paths, artifact names, and package identities in:

- `docs/architecture/v2/**` and `docs/architecture/v3/**`;
- RFCs 016-025 and their referenced evidence;
- `docs/production_readiness/execution_log.md` and dated readiness records;
- `docs/architecture/v4/g2_*.log`, `g2_jank_results.md`,
  `g3_inframe_results.md`, `m0_certification_findings.md`, and M0 baseline
  receipts.

Current instructions and consumer-facing documentation are different: root and
package READMEs, API doc comments, example installation/import snippets,
archive fixture documentation, RFC 026, this manifest, and the v4 build plan
must describe the final identities while explicitly labeling historical names.

## 5. Generated-artifact invariants

M1 changes logical metadata, not generated runtime bytes.

1. The root and nested copies of each of the following remain byte-identical:
   `flark_comrak_bridge.wasm`, its `.wasm.buildinfo`, and
   `flark_v3_parser_worker.js`.
2. Their SHA-256 values are identical before and after each M1 commit. M1 must
   not rebuild Rust, WASM, or the Worker. A required rebuild means the commit is
   no longer rename-only and must stop.
3. The source manifest inside `.wasm.buildinfo` keeps physical
   `native/comrak_bridge/**` paths. It must still match current Rust sources.
4. `example/lib/v3_engine_lab_web_asset_version.dart` is generated by
   `tool/gen_wasm_buildinfo.dart` and remains byte-identical in M1.
5. Ignored `pubspec.lock` files at the root, nested package, and example are
   updated/regenerated from the edited pubspecs with `pub get`; they are
   verification inputs, not committed rename edits.
6. Ignored `.dart_tool/package_config.json`, `package_graph.json`,
   `.flutter-plugins-dependencies`, and platform ephemeral files are regenerated
   after each stage. Never text-replace or commit them.
7. The tracked Linux and macOS generated plugin registrants remain
   byte-identical because M1 changes no plugin set. If Flutter regeneration
   changes them, inspect and separate that change rather than folding it into
   M1.
8. Publish archives, hosted-cache directories, Flutter build assets, and their
   package configs are regenerated and therefore use the new logical names,
   even though source-package directories in the checkout do not move.

Record pre- and post-commit hashes with:

```sh
shasum -a 256 \
  lib/assets/wasm/flark_comrak_bridge.wasm \
  lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo \
  lib/assets/worker/flark_v3_parser_worker.js \
  packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm \
  packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo \
  packages/flark_flutter/lib/assets/worker/flark_v3_parser_worker.js \
  example/lib/v3_engine_lab_web_asset_version.dart \
  example/linux/flutter/generated_plugin_registrant.cc \
  example/linux/flutter/generated_plugin_registrant.h \
  example/macos/Flutter/GeneratedPluginRegistrant.swift
```

## 6. Package-config and archive expectations

After `dart pub get` or `flutter pub get`, each generated package config must
contain exactly one entry for each expected identity and none for the displaced
identity.

| Stage/config | Required entries | Forbidden entries |
| --- | --- | --- |
| M1A root | one `flark_core` rooted at the repository root | `flark` |
| M1A nested Flutter | one `flark_flutter` rooted at `packages/flark_flutter`, one `flark_core` rooted at repository root | `flark` |
| M1A example | one `flark_flutter`, one `flark_core`, each rooted at its override path | `flark` |
| M1A external headless consumer | one hosted `flark_core` rooted in the isolated cache | `flark`; any checkout path |
| M1B root | one `flark_core` rooted at repository root | root `flark` |
| M1B nested Flutter | one `flark` rooted at `packages/flark_flutter`, one `flark_core` rooted at repository root | `flark_flutter` |
| M1B example | one `flark` plus `flark_core` direct or transitive as its imports require | `flark_flutter` |
| M1B external product consumer | direct hosted `flark` and transitive hosted `flark_core`, each exactly once | `flark_flutter`; any checkout path |

Use the existing exact-root verifier rather than accepting name presence alone:

```sh
repo_root="$(pwd -P)"
dart run tool/archive_consumer/verify_package_config.dart \
  flark_core "$repo_root"

(
  cd packages/flark_flutter
  dart run ../../tool/archive_consumer/verify_package_config.dart \
    flark "$PWD" \
    flark_core "$repo_root"
)
```

The M1A archive gate must create and consume `flark_core.tar.gz` independently,
including its browser-runtime migration receipt. Its default Worker/WASM URLs
must resolve through `/packages/flark_core/`; the receipt must reject any
fallback to `/packages/flark/` or to a checkout copy.

The final M1B archive gate must create `flark_core.tar.gz` and `flark.tar.gz`,
serve both through the loopback hosted-package protocol, and prove:

- root archive entries include `lib/flark_core.dart`, `lib/flark.dart`,
  `lib/flark_v3.dart`, `hook/build.dart`, native Cargo sources, and root
  Worker/WASM assets;
- product archive entries include new `lib/flark.dart`, retained
  `lib/flark_flutter.dart` and `lib/flark_flutter_advanced.dart`, and the
  product Worker/WASM copies;
- neither archive contains `.dart_tool`, `pubspec_overrides.yaml`, native
  `target/`, absolute/parent-traversing paths, symlinks, or checkout references;
- neither published pubspec contains a path dependency or override;
- extracted archives equal their immutable hosted-cache copies;
- the product consumer declares/imports only `flark`, while package config
  resolves `flark_core` transitively from its archive;
- Flutter release output contains the assets at
  `assets/packages/flark/lib/assets/{worker,wasm}/...`, and a real Chrome test
  boots those exact archive-backed bytes;
- the root and product archive copies of the Worker, WASM, and buildinfo compare
  byte-for-byte equal.

Run the clean-checkout receipt with no warning suppression:

```sh
FLARK_KEEP_ARCHIVE_WORKSPACE=1 ./scripts/verify_publish_archives.sh
```

Retain the workspace path printed by the script, archive listings, logs,
archive SHA-256 values, package configs, toolchain versions, and commit SHA as
the immutable M1A or M1B receipt. Do not reuse one stage's workspace for the
other.

## 7. Registry rechecks

The 2026-08-08 not-found responses are evidence only; they reserve nothing.
Recheck all three names immediately before the first M1A commit and again
immediately before the first publication attempt:

```sh
set -eu
registry_receipt_dir="${REGISTRY_RECEIPT_DIR:?set a new receipt directory}"
mkdir "$registry_receipt_dir"

for package_name in flark flark_core flark_flutter; do
  checked_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  endpoint="https://pub.dev/api/packages/$package_name"
  headers_file="$registry_receipt_dir/$package_name.headers"
  response_file="$registry_receipt_dir/$package_name.json"
  http_status="$(curl --silent --show-error \
    --dump-header "$headers_file" \
    --output "$response_file" \
    --write-out '%{http_code}' \
    "$endpoint")"
  response_sha256="$(shasum -a 256 "$response_file" | awk '{print $1}')"
  headers_sha256="$(shasum -a 256 "$headers_file" | awk '{print $1}')"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$checked_at" "$package_name" "$endpoint" "$http_status" \
    "$response_sha256" "$headers_sha256" \
    >>"$registry_receipt_dir/manifest.tsv"
done
```

The checked-in receipt records, for each name, the exact endpoint, UTC
timestamp, HTTP status, complete response body (or a linked immutable artifact),
and response SHA-256. A network error is not a not-found result. If `flark` or
`flark_core` becomes owned or otherwise unavailable, stop M1/publication and
return to architecture review; do not choose a substitute name inside a
mechanical rename commit.

## 8. Stage acceptance

For each stage, preserve a before/after export inventory and generated-artifact
hash manifest, run the selectors above, regenerate dependency state, and run
the stage's complete analyzer/test/build/archive gates at the clean committed
SHA. The diff may contain identity metadata, package URIs, current docs,
archive machinery, tests of those identities, and the new M1B forwarding
barrel. It may not contain runtime, parser, ABI, export-set, generated-binary,
filesystem-layout, formatting-cleanup, or unrelated dependency changes.

Any such extra change rejects the stage even if tests pass.
