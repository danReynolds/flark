import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'backend.dart';
import 'native.dart' as native;
import 'render_model.dart';
import 'schema.g.dart';

typedef _ParseC =
    Int32 Function(
      Pointer<Uint8>,
      Uint32,
      Pointer<Pointer<Uint8>>,
      Pointer<Uint32>,
    );
typedef _ParseD =
    int Function(Pointer<Uint8>, int, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef _AllocC = Pointer<Uint8> Function(Uint32);
typedef _AllocD = Pointer<Uint8> Function(int);
typedef _FreeC = Void Function(Pointer<Uint8>, Uint32);
typedef _FreeD = void Function(Pointer<Uint8>, int);
typedef _VersionC = Uint32 Function();
typedef _VersionD = int Function();

const int _initialInputCapacity = 4096;

/// The native transport: one synchronous FFI call per parse.
///
/// Symbols come from the code asset the build hook produces. Outside product
/// builds, `FLARK_PARSE_LIBRARY` may name a library to load instead, for
/// tooling; release builds ignore it so an environment cannot redirect the
/// parser.
final class FfiParseBackend implements FlarkParseBackend {
  FfiParseBackend._(this._parse, this._alloc, this._free, this._version) {
    final version = _version();
    if (version != RenderModelSchema.version) {
      throw FlarkParseException(
        FlarkParseException.schemaMismatchCode,
        'native flark_parse writes schema $version, this package reads ${RenderModelSchema.version}',
      );
    }
    _outCell = _alloc(16);
    _outPtr = _outCell.cast<Pointer<Uint8>>();
    _outLen = (_outCell + 8).cast<Uint32>();
    _input = _alloc(_initialInputCapacity);
    _inputCapacity = _initialInputCapacity;
  }

  factory FfiParseBackend() {
    const product = bool.fromEnvironment('dart.vm.product');
    final override = product
        ? null
        : Platform.environment['FLARK_PARSE_LIBRARY'];
    if (override != null && override.isNotEmpty) {
      final lib = DynamicLibrary.open(override);
      return FfiParseBackend._(
        lib.lookupFunction<_ParseC, _ParseD>('flark_parse'),
        lib.lookupFunction<_AllocC, _AllocD>('flark_parse_alloc'),
        lib.lookupFunction<_FreeC, _FreeD>('flark_parse_free'),
        lib.lookupFunction<_VersionC, _VersionD>('flark_parse_schema_version'),
      );
    }
    return FfiParseBackend._(
      native.flarkParse,
      native.flarkParseAlloc,
      native.flarkParseFree,
      native.flarkParseSchemaVersion,
    );
  }

  final _ParseD _parse;
  final _AllocD _alloc;
  final _FreeD _free;
  final _VersionD _version;
  late final Pointer<Uint8> _outCell;
  late final Pointer<Pointer<Uint8>> _outPtr;
  late final Pointer<Uint32> _outLen;
  late Pointer<Uint8> _input;
  late int _inputCapacity;
  bool _disposed = false;

  @override
  int get schemaVersion => _version();

  @override
  RenderModel parse(String source) {
    if (_disposed) throw StateError('FfiParseBackend used after dispose');
    // One UTF-16 code unit never needs more than three UTF-8 bytes. Keep
    // headroom so typing does not reallocate on every keystroke.
    if (source.length * 3 > _inputCapacity) {
      _free(_input, _inputCapacity);
      _inputCapacity = source.length * 6;
      _input = _alloc(_inputCapacity);
    }
    final length = _encodeUtf8(source, _input.asTypedList(_inputCapacity));
    final rc = _parse(_input, length, _outPtr, _outLen);
    if (rc != 0) throw FlarkParseException.fromCode(rc);
    final len = _outLen.value;
    final ptr = _outPtr.value;
    // Copy out so the model outlives the native buffer: one memcpy of a few
    // hundred KB at most inside the tier, into a word-aligned Dart buffer.
    final copy = Uint8List.fromList(ptr.asTypedList(len));
    _free(ptr, len);
    return RenderModel(copy);
  }

  /// Release the native input buffer and out-cell. Parsing after this throws.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _free(_input, _inputCapacity);
    _free(_outCell, 16);
  }
}

/// Write [source] into [out] as UTF-8 and return the byte length, validating
/// as [validateFlarkSourceText] does in the same pass. Encoding straight into
/// native memory avoids a Dart byte list and a copy on every parse.
int _encodeUtf8(String source, Uint8List out) {
  var o = 0;
  final n = source.length;
  for (var i = 0; i < n; i++) {
    final unit = source.codeUnitAt(i);
    if (unit < 0x80) {
      out[o++] = unit;
    } else if (unit < 0x800) {
      out[o++] = 0xC0 | unit >> 6;
      out[o++] = 0x80 | unit & 0x3F;
    } else if (unit < 0xD800 || unit > 0xDFFF) {
      out[o++] = 0xE0 | unit >> 12;
      out[o++] = 0x80 | unit >> 6 & 0x3F;
      out[o++] = 0x80 | unit & 0x3F;
    } else {
      final low = unit <= 0xDBFF && i + 1 < n ? source.codeUnitAt(i + 1) : 0;
      if (low < 0xDC00 || low > 0xDFFF) {
        throw FlarkParseException(
          FlarkParseException.invalidHostTextCode,
          'unpaired UTF-16 surrogate at offset $i',
        );
      }
      final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
      out[o++] = 0xF0 | scalar >> 18;
      out[o++] = 0x80 | scalar >> 12 & 0x3F;
      out[o++] = 0x80 | scalar >> 6 & 0x3F;
      out[o++] = 0x80 | scalar & 0x3F;
      i++;
    }
  }
  return o;
}

FlarkParseBackend createParseBackend() => FfiParseBackend();
