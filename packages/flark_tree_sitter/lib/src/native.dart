import 'dart:ffi';

@Native<Uint32 Function()>(symbol: 'flark_tree_sitter_version')
external int codeVersion();

@Native<Pointer<Uint8> Function(Uint32)>(symbol: 'flark_tree_sitter_alloc')
external Pointer<Uint8> codeAlloc(int length);

@Native<Void Function(Pointer<Uint8>, Uint32)>(symbol: 'flark_tree_sitter_free')
external void codeFree(Pointer<Uint8> ptr, int length);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Uint32,
    Uint32,
    Pointer<Pointer<Uint8>>,
    Pointer<Uint32>,
  )
>(symbol: 'flark_tree_sitter_analyze')
external int codeAnalyze(
  Pointer<Uint8> source,
  int length,
  int language,
  Pointer<Pointer<Uint8>> output,
  Pointer<Uint32> outputLength,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Uint32,
    Uint32,
    Pointer<Pointer<Uint8>>,
    Pointer<Uint32>,
  )
>(symbol: 'flark_tree_sitter_edit')
external int codeEdit(
  Pointer<Uint8> request,
  int length,
  int language,
  Pointer<Pointer<Uint8>> output,
  Pointer<Uint32> outputLength,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Uint32,
    Uint32,
    Pointer<Pointer<Uint8>>,
    Pointer<Uint32>,
  )
>(symbol: 'flark_tree_sitter_detect')
external int codeDetect(
  Pointer<Uint8> source,
  int length,
  int language,
  Pointer<Pointer<Uint8>> output,
  Pointer<Uint32> outputLength,
);
