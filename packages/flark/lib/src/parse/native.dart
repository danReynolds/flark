// FFI symbols for the flark_parse code asset built by hook/build.dart.
import 'dart:ffi';

@Native<Int32 Function(Pointer<Uint8>, Uint32, Pointer<Pointer<Uint8>>, Pointer<Uint32>)>(symbol: 'flark_parse')
external int flarkParse(Pointer<Uint8> src, int len, Pointer<Pointer<Uint8>> out, Pointer<Uint32> outLen);

@Native<Pointer<Uint8> Function(Uint32)>(symbol: 'flark_parse_alloc')
external Pointer<Uint8> flarkParseAlloc(int len);

@Native<Void Function(Pointer<Uint8>, Uint32)>(symbol: 'flark_parse_free')
external void flarkParseFree(Pointer<Uint8> ptr, int len);

@Native<Uint32 Function()>(symbol: 'flark_parse_schema_version')
external int flarkParseSchemaVersion();
