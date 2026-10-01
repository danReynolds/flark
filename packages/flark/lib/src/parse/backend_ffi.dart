import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'backend.dart';
import 'native.dart' as native;
import 'parser_memory.dart';
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
///
/// The parser owns native memory: [dispose] frees it, and a finalizer frees
/// it for a parser that is dropped instead. Being [Finalizable] also keeps
/// the parser in its isolate: a copy sent to another would share that memory
/// without owning it, so the VM refuses the copy. Create a parser in each
/// isolate that parses.
final class FfiParseBackend implements FlarkParseBackend, Finalizable {
  FfiParseBackend._(this._parse, this._free, this._version) {
    final version = _version();
    if (version != RenderModelSchema.version) {
      throw FlarkParseException(
        FlarkParseException.schemaMismatchCode,
        'native flark_parse writes schema $version, this package reads ${RenderModelSchema.version}',
      );
    }
    // Allocated after the schema check, so a mismatched library owns nothing.
    _memory = ParserMemory(_initialInputCapacity);
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
        lib.lookupFunction<_FreeC, _FreeD>('flark_parse_free'),
        lib.lookupFunction<_VersionC, _VersionD>('flark_parse_schema_version'),
      );
    }
    return FfiParseBackend._(
      native.flarkParse,
      native.flarkParseFree,
      native.flarkParseSchemaVersion,
    );
  }

  final _ParseD _parse;
  final _FreeD _free;
  final _VersionD _version;
  late final ParserMemory _memory;
  bool _disposed = false;

  @override
  int get schemaVersion => _version();

  @override
  RenderModel parse(String source) {
    if (_disposed) throw StateError('FfiParseBackend used after dispose');
    final memory = _memory;
    // One UTF-16 code unit never needs more than three UTF-8 bytes.
    if (source.length * 3 > memory.inputCapacity) {
      final capacity = ParserMemory.inputCapacityFor(source.length);
      if (capacity == null) {
        throw FlarkParseException(
          FlarkParseException.invalidHostTextCode,
          'a source of ${source.length} UTF-16 code units can exceed the '
          'parser input limit of ${ParserMemory.maxInputBytes} bytes',
        );
      }
      memory.grow(capacity);
    }
    final length = _encodeUtf8(
      source,
      memory.input.asTypedList(memory.inputCapacity),
    );
    final rc = _parse(
      memory.input,
      length,
      memory.modelAddress,
      memory.modelLength,
    );
    if (rc != 0) throw FlarkParseException.fromCode(rc);
    final len = memory.modelLength.value;
    final ptr = memory.modelAddress.value;
    // Copy out so the model outlives the native buffer: one memcpy of a few
    // hundred KB at most inside the tier, into a word-aligned Dart buffer.
    final copy = Uint8List.fromList(ptr.asTypedList(len));
    _free(ptr, len);
    return RenderModel(copy);
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _memory.release();
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

/// A new FFI parser. The caller owns it and calls
/// [FlarkParseBackend.dispose] when done.
FlarkParseBackend createParseBackend() => FfiParseBackend();
