# Publish-archive consumer fixtures

These are copied into a temporary directory by
`scripts/verify_publish_archives.sh`. A loopback hosted-package server serves
pub's generated `.tar.gz` files, so the consumers resolve ordinary hosted
dependencies without path dependencies or `dependency_overrides`.

Keep the fixtures outside the archives themselves. The verifier compares the
hosted cache with the archives, checks resolved package roots, and scans
generated output for accidental references back to the Flark checkout.

