# Consumer API diagnostic receipts

Implementation: `c25d3fb1cbca6b5b8e4c14794bb2b10d12471bba`.
Machine: Apple M1 Pro, macOS 26.2, arm64; Dart 3.12.2 / Flutter 3.44.4.
Local AOT executables produced with `dart build cli`, not CI or a device lab.

- `reader-aot.json`: ten alternating-order construction rounds after warmup,
  4,731-byte document, one and 100 retained instances. No layout, paint, or
  retained-memory measurement. The reader did not establish a speed advantage.
- `session-aot.json`: alternating engine/session insertion timings after 50
  warmups, 200 samples per path and size. Each insertion is undone outside its
  timed interval. Includes public state publication, excludes host rendering.

Reproduce from `packages/flark` with `dart build cli --target
 tool/bench_reader.dart --output <dir>` and likewise `tool/bench_session.dart`;
run the executable from the generated bundle. Copy the entire bundle when moving it.
Do not interpret these small synthetic workloads as production frame budgets.
