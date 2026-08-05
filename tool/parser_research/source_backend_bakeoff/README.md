# Source backend bakeoff

Temporary falsification probe for RFC 023. It compares the integrated slice's
custom persistent source with `crop` 0.4.3 under the source contract Flark
actually needs: O(1) snapshot leases, byte-indexed edits, UTF-16 summaries,
bounded range reads, cooperative scans, short history retention, and observable
retirement latency.

This does not select `crop`. In particular, its private tree does not expose
stable subtree identities or fuelled destruction. The probe tests whether edit
provenance plus a snapshot lease can make subtree identity unnecessary and
whether ordinary `Arc` retirement is acceptably bounded.

Example:

```sh
cargo run --release -- crop 10 1000 4096
cargo run --release --features custom -- custom 10 1000 4096
cargo run --release -- crop-stream 100 1000 4096
```

Run each backend in a separate warmed process under `/usr/bin/time -l` for RSS.
`crop-stream` is a supplementary ingress probe that avoids allocating a whole
generator `String`; it is not a third source backend.
