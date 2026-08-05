# Oversized physical-line gate

Disposable proof for the oversized-line seam of the Comrak-correspondent block
architecture. It does not replace the clean block core and **must not become a
second production parser**.

The gate mechanically restates only whole-line-sensitive regular classifiers
from the pinned Comrak 0.54 scanner lineage as resumable, byte-metered jobs.
Ordinary bounded inputs are differentially checked against the existing narrow
facade. Large inputs are checked against pristine full-parser projections and
post-line sentinel state where a facade call cannot be made.

The shipping maintenance direction is to generate or expose these resumable
states from the same pinned scanner source. The handwritten forms here are
adjudicated feasibility witnesses only. See [RESULTS.md](RESULTS.md) for the
verdict, receipts, uncovered surfaces, and the next acceptance gate.

Run:

```sh
cargo test
cargo run --release --bin receipt
```

The receipt binary deliberately runs the resumable jobs on ordinary inputs as
well as 1 MiB and 10 MiB inputs. It does not select a different grammar at the
8 KiB facade boundary.
