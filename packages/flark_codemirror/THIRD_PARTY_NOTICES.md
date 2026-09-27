# Third-party notices

The language modes under `lib/src/modes/` and the stream in
`lib/src/stream.dart` are ports of CodeMirror. Each ported file names its
upstream source:

- CodeMirror 5.65.21 (https://codemirror.net/5/, npm `codemirror@5.65.21`,
  tarball SHA-256
  `b26d9d7ae0f42e34b44a8f7667d29a3ab0d3df88d0e79c0dad39dc03ba43f6dc`): the
  stream and the `javascript`, `htmlmixed` and `php` modes.
- `@codemirror/legacy-modes` 6.5.4 (tarball SHA-256
  `0f5a0e82d4926ae42c420251c0d554b51534285db41cea35d93d5a014db57aa2`): the
  `python`, `clike`, `xml`, `css`, `sql`, `shell`, `yaml`, `go`, `ruby`,
  `rust`, `simple-mode` and `powershell` modes.
- `@codemirror/language` 6.12.4 (tarball SHA-256
  `5e49acf55fde65ce9848068f0f06478be9ec71f818e91de6eca00824e9152226`): the
  `StringStream` behaviour the legacy modes run on, which
  `tool/reference/generate_legacy.mjs` reads from an unpacked copy.

The reference fixtures under `test/fixtures/` include excerpts of
CodeMirror's own source as inputs.

## CodeMirror 5.65.21

Upstream: https://github.com/codemirror/codemirror5

```text
MIT License

Copyright (C) 2017 by Marijn Haverbeke <marijn@haverbeke.berlin> and others

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## @codemirror/legacy-modes 6.5.4 and @codemirror/language 6.12.4

Upstream: https://github.com/codemirror/legacy-modes and
https://github.com/codemirror/language, each under this license:

```text
MIT License

Copyright (C) 2018-2021 by Marijn Haverbeke <marijn@haverbeke.berlin> and others

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```
