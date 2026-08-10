# SuperEditor Mac competitor profile runner

This isolated package measures the pinned, unmodified SuperEditor dependency in
a real Flutter profile application. It is evidence tooling, not part of Flark.

The runner generates the frozen `ordinary-prose` fixture at an exact UTF-8 byte
length, maps physical lines to default `ParagraphNode`s, and exports the model by
joining nodes with the same line separator. Initial and final SHA-256 values make
any source-fidelity change explicit.

Measured edits are not direct `Editor.execute` calls:

- text and backspace enter as native macOS key events;
- paste writes an exact 32,768-byte payload to `NSPasteboard`, first asks AppKit
  to send `paste:` to Flutter's active responder, and falls back to
  `NSTextInputClient.insertText` on that same active platform input client; and
- sustained typing uses a native background 10 Hz source so OS events continue
  to queue even when Flutter's UI/platform thread is occupied.

Paste samples do not accumulate. After every warmup and measured paste, the
harness records the exact canonical pre/post byte and SHA-256 states, selects
the pasted source range, sends a native platform backspace outside the measured
interval, waits for acceptance and a containing raster frame, and proves the
original fixture is restored before the next sample. The paired Quill harness
uses the same evolving-state contract. The hashed timeline links unique paste
and reset sequences for all 22 transitions and retains comparable
request/ingress/accept/build/raster/callback timestamps.

Every accepted model change is correlated with the first containing engine
`FrameTiming.rasterFinish`. Raw input, frame, RSS, fidelity export, dependency,
machine, power, display, invocation, and hashed artifact provenance are retained.

## One fresh-process run

Run the driver from this directory. The full protocol defaults are selected when
count overrides are omitted:

```sh
dart run tool/run_macos_profile.dart \
  --flutter=/absolute/path/to/flutter \
  --workload=cold-open \
  --bytes=1048576 \
  --location=middle \
  --run-id=supereditor-cold-1mib-01
```

Workloads are `cold-open`, `sustained-typing`, `local-insert-delete`, and
`paste-32kib`. Locations are `start`, `middle`, and `end`. Each invocation
launches a fresh process and writes retained artifacts below ignored `results/`.
Use `--no-build` only after the exact profile bundle has already been built.

Count overrides are for runner validation and produce a mechanically
non-conformant receipt, for example:

```sh
dart run tool/run_macos_profile.dart \
  --flutter=/absolute/path/to/flutter \
  --no-build \
  --workload=sustained-typing \
  --bytes=1048576 \
  --location=end \
  --warmups=2 \
  --samples=3 \
  --run-id=smoke-typing
```

`--paste-bytes=N` likewise exists only to validate the Command-V plumbing at a
small size. The default and only protocol-conformant value is exactly 32,768.

The runner preserves Flutter's active `NSTextInputClient`; it never replaces it
with `FlutterView` for paste. The original clipboard contents are restored after
every attempt. Some AppKit responders report that `paste:` was handled without
emitting a model edit; the first warmup allows one frame-scale grace period, then
uses and remembers the native text-input fallback for the process. If neither
route delivers the edit, the receipt fails closed.

The launch driver has an external no-progress watchdog (60 seconds by default),
so a blocked Flutter isolate still yields a machine-readable timeout receipt.
`--app-timeout-seconds` may be set below `--timeout-seconds` for diagnostic
controls that distinguish a responsive rejected input from a blocked UI; any
such override is not protocol evidence.
The overall M0 coordinator remains responsible for five-minute idle periods,
exclusive machine use, and the recorded Latin-square rotation across peers and
sizes; this peer-local runner cannot honestly attest those cross-run controls.
Consequently, peer-local receipts always set `claimEligible` to false. They also
record whether all local checks passed, but only the external suite coordinator
may promote a complete run group after verifying the cross-peer controls.

## Dependency reconstruction

`pubspec.lock` and `evidence/resolved_dependencies_compact.txt` are tracked.
The pinned SuperEditor revision currently compiles cleanly on Flutter 3.44.4;
the historical Flutter-3.41 cache patch is neither present nor needed. The
driver checks the dependency git tree before every run. If it is dirty, it
retains the exact binary diff and hash and makes the result claim-ineligible.
