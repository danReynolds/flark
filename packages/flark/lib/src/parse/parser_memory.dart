import 'dart:ffi';
import 'dart:math' as math;

import 'package:ffi/ffi.dart';

/// One FFI parser's native memory: a cell that `flark_parse` writes the
/// model's address and byte length into, followed by the UTF-8 input.
///
/// The block comes from the C allocator rather than `flark_parse_alloc`: a
/// [NativeFinalizer] calls a free function with one argument, and
/// `flark_parse_free` also needs the length. The parser library only reads
/// the input and writes the cell, so it never frees this block itself.
final class ParserMemory implements Finalizable {
  /// A block whose input holds [inputCapacity] bytes.
  ParserMemory(int inputCapacity) {
    _replace(inputCapacity);
  }

  static final _finalizer = NativeFinalizer(malloc.nativeFree);

  /// An address and a u32 length, padded to 16 bytes as the Wasm transport's
  /// cell is.
  static const int _cellBytes = 16;

  /// The most input `flark_parse` accepts: it takes the length as a u32.
  static const int maxInputBytes = 0xFFFFFFFF;

  /// The input capacity for a source of [codeUnits] UTF-16 code units: three
  /// bytes per unit, the most UTF-8 can need, doubled so that typing does not
  /// reallocate on every keystroke, and capped at [maxInputBytes]. Null when
  /// even the worst case may not fit, so that the caller refuses the source
  /// instead of passing a truncated length.
  static int? inputCapacityFor(int codeUnits) {
    final worstCase = codeUnits * 3;
    if (worstCase > maxInputBytes) return null;
    return math.min(worstCase * 2, maxInputBytes);
  }

  /// Blocks attached to the finalizer and not yet freed by [release]: those
  /// of live parsers, and of dropped ones that the finalizer frees when their
  /// parser is collected. Tests read it to check that each parser keeps
  /// exactly one block attached until it is disposed.
  static int get attachedBlocks => _attachedBlocks;
  static int _attachedBlocks = 0;

  Pointer<Uint8> _block = nullptr;
  int _inputCapacity = 0;

  /// Where `flark_parse` writes the address of the model it allocates.
  Pointer<Pointer<Uint8>> get modelAddress => _block.cast();

  /// Where `flark_parse` writes the model's length in bytes.
  Pointer<Uint32> get modelLength => (_block + 8).cast();

  /// The UTF-8 input buffer, [inputCapacity] bytes long.
  Pointer<Uint8> get input => _block + _cellBytes;

  int get inputCapacity => _inputCapacity;

  /// Replace the block with one whose input holds [inputCapacity] bytes. The
  /// new block is allocated before the old one is freed, so a failed
  /// allocation leaves this memory usable and still attached.
  void grow(int inputCapacity) => _replace(inputCapacity);

  /// Free the block now instead of when the finalizer runs. A second call
  /// does nothing.
  void release() {
    if (_block == nullptr) return;
    _free();
    _block = nullptr;
    _inputCapacity = 0;
  }

  void _replace(int inputCapacity) {
    final block = malloc.allocate<Uint8>(_cellBytes + inputCapacity);
    if (_block != nullptr) _free();
    _block = block;
    _inputCapacity = inputCapacity;
    _finalizer.attach(
      this,
      block.cast(),
      detach: this,
      externalSize: _cellBytes + inputCapacity,
    );
    _attachedBlocks++;
  }

  void _free() {
    _finalizer.detach(this);
    _attachedBlocks--;
    malloc.free(_block);
  }
}
