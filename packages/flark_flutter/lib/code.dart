/// Code regions: highlighting, indentation and language detection for fenced
/// code, from `flark_codemirror`, CodeMirror's language modes ported to Dart.
/// Pass a [FlarkCodeMirror] as an editor's `codeEditing`. It runs
/// synchronously on the UI thread and needs no loading or disposal;
/// [FlarkCodeMirror.only] keeps other languages out of the build.
library;

export 'package:flark_codemirror/flark_codemirror.dart'
    show
        FlarkCodeMirror,
        CodeMirrorLanguage,
        CodeMirrorLanguages,
        codeMirrorLanguageName;
