import 'dart:ffi';

typedef ParseC = Int32 Function(Pointer<Uint8>, Uint32, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef ParseD = int Function(Pointer<Uint8>, int, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef AllocC = Pointer<Uint8> Function(Uint32);
typedef AllocD = Pointer<Uint8> Function(int);
typedef FreeC = Void Function(Pointer<Uint8>, Uint32);
typedef FreeD = void Function(Pointer<Uint8>, int);

final DynamicLibrary _lib = DynamicLibrary.process();
final ParseD flarkParse = _lib.lookupFunction<ParseC, ParseD>('flark_spike_parse');
final AllocD flarkAlloc = _lib.lookupFunction<AllocC, AllocD>('flark_spike_alloc');
final FreeD flarkFree = _lib.lookupFunction<FreeC, FreeD>('flark_spike_free');
