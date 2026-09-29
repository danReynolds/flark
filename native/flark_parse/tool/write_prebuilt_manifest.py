#!/usr/bin/env python3
"""Pins a published parser release in packages/flark/hook/prebuilt.json.

  native/flark_parse/tool/write_prebuilt_manifest.py <release-tag> [<assets-dir>]

Pins the GitHub release <release-tag> as it is served, not as it was built:

- the release must be public, not a draft, and have a library for every target
  build_release_libraries.sh builds, and nothing else that looks like one;
- the tag's commit must hold the same parse crate and toolchain as HEAD, so the
  pinned libraries were built from the source this package releases with;
- with <assets-dir> (the `assets` directory build_release_libraries.sh wrote),
  every downloaded library must be byte-for-byte what was built.

Each SHA-256 is computed from the downloaded bytes. Needs `gh` and the tag in
the local repository (`git fetch origin tag <release-tag>`).
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile

REPOSITORY = 'danReynolds/flark'
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
MANIFEST = os.path.join(ROOT, 'packages', 'flark', 'hook', 'prebuilt.json')
SCRIPT = os.path.join(ROOT, 'native', 'flark_parse', 'tool', 'build_release_libraries.sh')


def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, cwd=ROOT, **kwargs).stdout


def library(triple):
    if 'apple' in triple:
        return 'libflark_parse.dylib'
    if 'windows' in triple:
        return 'flark_parse.dll'
    return 'libflark_parse.so'


def sha256(path):
    with open(path, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    tag = sys.argv[1]
    built = sys.argv[2] if len(sys.argv) == 3 else None
    targets = re.search(r'^ALL="([^"]+)"$', open(SCRIPT).read(), re.M).group(1).split()

    release = json.loads(run('gh', 'release', 'view', tag, '--repo', REPOSITORY,
                             '--json', 'isDraft,isImmutable,assets'))
    if release['isDraft']:
        sys.exit(f'{tag} is a draft; publish it before pinning it')
    if not release['isImmutable']:
        print(f'warning: {tag} is not immutable, so its assets can still be replaced. '
              'Published flark versions would then fail their hash check.', file=sys.stderr)

    try:
        commit = run('git', 'rev-parse', f'{tag}^{{commit}}').strip()
    except subprocess.CalledProcessError:
        sys.exit(f'{tag} is not in this repository; run `git fetch origin tag {tag}`')
    changed = subprocess.run(
        ['git', 'diff', '--quiet', commit, 'HEAD', '--', 'native/flark_parse',
         ':!native/flark_parse/tool', 'rust-toolchain.toml'], cwd=ROOT).returncode
    if changed:
        sys.exit(f'the parse crate or rust-toolchain.toml changed between {tag} ({commit[:12]}) '
                 'and HEAD; build and publish new libraries under a new tag')

    expected = {f'{triple}-{library(triple)}': triple for triple in targets}
    names = {asset['name'] for asset in release['assets']}
    missing = sorted(set(expected) - names)
    if missing:
        sys.exit(f'{tag} has no {", ".join(missing)}')
    unexpected = sorted(n for n in names - set(expected) if 'flark_parse' in n)
    if unexpected:
        sys.exit(f'{tag} has libraries this package cannot pin: {", ".join(unexpected)}')

    files = {}
    with tempfile.TemporaryDirectory() as downloads:
        for name, triple in sorted(expected.items()):
            run('gh', 'release', 'download', tag, '--repo', REPOSITORY,
                '--pattern', name, '--dir', downloads)
            digest = sha256(os.path.join(downloads, name))
            if built is not None:
                local = sha256(os.path.join(built, name))
                if local != digest:
                    sys.exit(f'{tag} serves {name} with SHA-256 {digest}, '
                             f'but {built} holds {local}')
            files[triple] = {
                'url': f'https://github.com/{REPOSITORY}/releases/download/{tag}/{name}',
                'sha256': digest,
            }
    with open(MANIFEST, 'w') as f:
        json.dump({'release': tag, 'commit': commit, 'files': files}, f, indent=2)
        f.write('\n')
    print(f'pinned {len(files)} libraries from {tag} ({commit[:12]}) in '
          f'{os.path.relpath(MANIFEST, ROOT)}')


if __name__ == '__main__':
    main()
