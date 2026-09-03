import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'backend.dart';
import 'native.dart' as native;
import 'render_model.dart';

typedef _ParseC = Int32 Function(Pointer<Uint8>, Uint32, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef _ParseD = int Function(Pointer<Uint8>, int, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef _AllocC = Pointer<Uint8> Function(Uint32);
typedef _AllocD = Pointer<Uint8> Function(int);
typedef _FreeC = Void Function(Pointer<Uint8>, Uint32);
typedef _FreeD = void Function(Pointer<Uint8>, int);
typedef _VersionC = Uint32 Function();
typedef _VersionD = int Function();

/// The native transport: one synchronous FFI call per parse.
///
/// Symbols come from the code asset the build hook produces. Set
/// `FLARK_PARSE_LIBRARY` to a library path to bypass the hook, for tooling.
final class FfiParseBackend implements FlarkParseBackend {
  FfiParseBackend._(this._parse, this._alloc, this._free, this._version) {
    _outCell = _alloc(16);
    _outPtr = _outCell.cast<Pointer<Uint8>>();
    _outLen = Pointer<Uint32>.fromAddress(_outCell.address + 8);
  }

  factory FfiParseBackend() {
    final override = Platform.environment['FLARK_PARSE_LIBRARY'];
    if (override != null && override.isNotEmpty) {
      final lib = DynamicLibrary.open(override);
      return FfiParseBackend._(
        lib.lookupFunction<_ParseC, _ParseD>('flark_parse'),
        lib.lookupFunction<_AllocC, _AllocD>('flark_parse_alloc'),
        lib.lookupFunction<_FreeC, _FreeD>('flark_parse_free'),
        lib.lookupFunction<_VersionC, _VersionD>('flark_parse_schema_version'),
      );
    }
    return FfiParseBackend._(native.flarkParse, native.flarkParseAlloc, native.flarkParseFree, native.flarkParseSchemaVersion);
  }

  final _ParseD _parse;
  final _AllocD _alloc;
  final _FreeD _free;
  final _VersionD _version;
  late final Pointer<Uint8> _outCell;
  late final Pointer<Pointer<Uint8>> _outPtr;
  late final Pointer<Uint32> _outLen;
  Pointer<Uint8> _input = nullptr;
  int _inputCapacity = 0;

  @override
  int get schemaVersion => _version();

  @override
  RenderModel parse(String source) {
    final bytes = utf8.encode(source);
    if (bytes.length > _inputCapacity) {
      if (_input != nullptr) _free(_input, _inputCapacity);
      _inputCapacity = bytes.length * 2 + 1024;
      _input = _alloc(_inputCapacity);
    }
    _input.asTypedList(_inputCapacity).setRange(0, bytes.length, bytes);
    final rc = _parse(_input, bytes.length, _outPtr, _outLen);
    if (rc != 0) {
      throw FlarkParseException(rc, switch (rc) { 1 => 'null argument', 2 => 'invalid UTF-8', 3 => 'contained native fault', _ => 'unknown' });
    }
    final len = _outLen.value;
    final ptr = _outPtr.value;
    // Copy out so the model outlives the native buffer; the copy is a memcpy
    // of a few hundred KB at most inside the tier.
    final copy = Uint8List.fromList(ptr.asTypedList(len));
    _free(ptr, len);
    return RenderModel(copy);
  }
}

FlarkParseBackend createParseBackend() => FfiParseBackend();
