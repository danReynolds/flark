#!/usr/bin/env python3
"""Pins published parser libraries in packages/flark/hook/prebuilt.json.

  native/flark_parse/tool/write_prebuilt_manifest.py <assets-dir> <release-tag>

<assets-dir> is the `assets` directory build_release_libraries.sh wrote and
that was uploaded, unchanged, to the GitHub release <release-tag>. Every
asset's SHA-256 is recomputed and checked against SHA256SUMS; the manifest then
names each target's download URL with that hash, which the build hook
verifies before using a download.
"""
import hashlib
import json
import os
import sys

REPOSITORY = 'danReynolds/flark'
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
MANIFEST = os.path.join(ROOT, 'packages', 'flark', 'hook', 'prebuilt.json')
LIBRARIES = ('libflark_parse.dylib', 'libflark_parse.so', 'flark_parse.dll')


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    assets, tag = sys.argv[1], sys.argv[2]
    sums = {}
    for line in open(os.path.join(assets, 'SHA256SUMS')):
        digest, name = line.split(maxsplit=1)
        sums[name.strip()] = digest
    files = {}
    for name in sorted(sums):
        path = os.path.join(assets, name)
        digest = hashlib.sha256(open(path, 'rb').read()).hexdigest()
        if digest != sums[name]:
            sys.exit(f'{name}: SHA-256 {digest} does not match SHA256SUMS ({sums[name]})')
        library = next((l for l in LIBRARIES if name.endswith('-' + l)), None)
        if library is None:
            sys.exit(f'{name}: not a parser library asset')
        triple = name[:-len('-' + library)]
        files[triple] = {
            'url': f'https://github.com/{REPOSITORY}/releases/download/{tag}/{name}',
            'sha256': digest,
        }
    with open(MANIFEST, 'w') as f:
        json.dump({'release': tag, 'files': files}, f, indent=2)
        f.write('\n')
    print(f'pinned {len(files)} libraries from {tag} in {os.path.relpath(MANIFEST, ROOT)}')


if __name__ == '__main__':
    main()
