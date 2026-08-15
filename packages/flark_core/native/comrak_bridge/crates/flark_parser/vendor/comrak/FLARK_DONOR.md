# Flark lexical donor

This directory is the crates.io source for Comrak `0.54.0`, whose published
crate checksum is
`0d5910408554659ed848ff469e67ec83b30f179e72cec286cfdae64d1616f466`.
The corresponding upstream commit is
`172c2ee7d2c5c262a28be3e407aadf705daea2b7`.

Flark's production patch is deliberately limited to:

- a packaging-only manifest that retains the exact no-default core dependency
  closure and declares empty optional-feature names, avoiding unrelated
  CLI/renderer/dev dependencies in Flark's lockfile;
- registering and re-exporting `parser::block_spine_facade`; and
- the lexical facade itself, which exposes exact scanner/table/reference
  helpers to a caller-owned block controller.

The facade contains no full-parser oracle and no Flark document/controller
state. Comrak remains the lexical grammar donor; Flark owns resumability,
opener order, source authority, output, and publication. Intake of a new
Comrak version must replay this patch, refresh the pin/checksum, and rerun the
stock differential and Flark controller suites before changing the pin.
