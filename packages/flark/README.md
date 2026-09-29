# flark

The Markdown editing kernel behind
[flark_flutter](https://pub.dev/packages/flark_flutter) and
[flark_fleury](https://pub.dev/packages/flark_fleury). The Markdown source is
the document: nothing is converted to a rich-text model and back.

Every edit re-parses the whole document with unmodified
[comrak](https://github.com/kivikakk/comrak), CommonMark 0.31.2 and GitHub
Flavored Markdown. The kernel projects that parse into rows that hide syntax
outside the caret's context, keeps the caret on legal source offsets, and edits
through a closed set of commands, so what users type is exactly what is saved.

Most apps use a host package. Use `flark` directly to write a host, or to edit
and read Markdown without a UI:

```dart
import 'package:flark/flark.dart';
import 'package:flark/session.dart';

final session = FlarkSession(markdown: 'Plain ');
await session.ready;
session.command(const SetSelection(6, 6));
session.command(const ToggleStyle(Style.strong));
for (final character in 'bold words'.split('')) {
  session.command(InsertText(character));
}
print(session.state.markdown); // Plain **bold words**
```

## Status

0.5 is a preview. The parser's HTML output matches the CommonMark and GFM spec
suites apart from a short list of registered comrak deviations, and the kernel
is covered by generated command sequences with invariants on every step.
Platform qualification is uneven:

| Platform | State |
| --- | --- |
| macOS | Frame-time profiled (32 KiB documents within one frame on an M1 Pro) |
| Web | Frame-time profiled in desktop Chrome at 32 KiB; other browsers untested |
| Android | Tested by hand on a Pixel 6a; no frame-time profile yet |
| Windows | Kernel tests pass on x64 in CI; no host qualified yet |
| iOS, Linux | Parser libraries built for release; not yet run on devices |

Screen-reader support (VoiceOver, TalkBack) has not been qualified.

## The parser library

On Android, iOS, macOS, Linux and Windows the parser is a native library. The
package's build hook downloads the library for the target you build, from this
repository's GitHub release, checks it against the SHA-256 pinned in
`hook/prebuilt.json`, and caches it in the project's `.dart_tool`, so rebuilds
of that project reuse it. A fresh clone, a CI run or `flutter clean` downloads
it again. To build without network access, fetch the files once and point the
hook at them:

```yaml
hooks:
  user_defines:
    flark:
      prebuilt_dir: /path/to/flark_parse
```

The directory holds `<target triple>/<library>`, for example
`aarch64-apple-darwin/libflark_parse.dylib`. The release names its assets
`<target triple>-<library>` (`aarch64-apple-darwin-libflark_parse.dylib`), so
save each one under its triple's directory with the library's own name, and
check it against the SHA-256 in `hook/prebuilt.json`. In a checkout of this
repository the hook builds the parser from `native/flark_parse` with Rust
instead.

On the web the parser is the Wasm module bundled in the package; nothing is
downloaded.

## License

MIT. The parser links comrak and other Rust crates; their licenses are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
