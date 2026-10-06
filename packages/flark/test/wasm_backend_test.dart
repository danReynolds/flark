/// The Wasm transport's fault handling, under node or in a browser with
/// dart2js and dart2wasm. On the Dart VM a parser fault is a contained panic
/// that FFI reports as [FlarkParseException.faultCode]; wasm32 aborts on
/// panic instead, so the same fault traps out of the module. A module whose
/// exports trap on demand checks that the trap reaches Dart as that same
/// exception under both compilers, and that the parser rebuilds its instance
/// and parses on.
@TestOn('node || browser')
library;

import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:flark/rendering.dart';
import 'package:flark/session.dart';
import 'package:flark/wasm.dart';
import 'package:test/test.dart';

/// flark_parse's four exports and memory. `flark_parse` traps when its input
/// starts with `T` and otherwise refuses with code 4, as for an extraction
/// deviation; `flark_parse_alloc` traps when asked for more than 8 KiB, as
/// when memory cannot grow, and otherwise bump-allocates whole words.
Uint8List _faultingModule() {
  List<int> name(String s) => [s.length, ...s.codeUnits];
  List<int> section(int id, List<int> body) => [id, body.length, ...body];
  List<int> function(List<int> code) => [code.length + 1, 0, ...code];
  const i32 = 0x7f, end = 0x0b, unreachable = 0x00;
  final version = RenderModelSchema.version;
  assert(version < 64, 'one-byte signed LEB128');
  return Uint8List.fromList([
    ...[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    // Types: parse (i32 x4) -> i32, alloc i32 -> i32, free (i32 x2), version.
    ...section(1, [
      4,
      ...[0x60, 4, i32, i32, i32, i32, 1, i32],
      ...[0x60, 1, i32, 1, i32],
      ...[0x60, 2, i32, i32, 0],
      ...[0x60, 0, 1, i32],
    ]),
    ...section(3, [4, 0, 1, 2, 3]),
    // One page of memory, and a bump pointer starting at 1 KiB.
    ...section(5, [1, 0, 1]),
    ...section(6, [1, i32, 1, 0x41, 0x80, 0x08, end]),
    ...section(7, [
      5,
      ...name('memory'),
      2,
      0,
      ...name('flark_parse'),
      0,
      0,
      ...name('flark_parse_alloc'),
      0,
      1,
      ...name('flark_parse_free'),
      0,
      2,
      ...name('flark_parse_schema_version'),
      0,
      3,
    ]),
    ...section(10, [
      4,
      // if (load8_u(src) == 'T') unreachable; return 4;
      ...function([
        ...[0x20, 0, 0x2d, 0, 0, 0x41, 0xd4, 0x00, 0x46],
        ...[0x04, 0x40, unreachable, end, 0x41, 4, end],
      ]),
      // if (len > 8192) unreachable; top += (len + 3) & ~3; return old top;
      ...function([
        ...[0x20, 0, 0x41, 0x80, 0xc0, 0x00, 0x4b],
        ...[0x04, 0x40, unreachable, end],
        ...[0x23, 0, 0x23, 0, 0x20, 0, 0x41, 3, 0x6a, 0x41, 0x7c, 0x71, 0x6a],
        ...[0x24, 0, end],
      ]),
      ...function([end]),
      ...function([0x41, version, end]),
    ]),
  ]);
}

Matcher _throwsParse(int code) =>
    throwsA(isA<FlarkParseException>().having((e) => e.code, 'code', code));

void main() {
  late FlarkParseBackend backend;
  setUp(
    () async => backend = await WasmParseBackend.fromBytes(_faultingModule()),
  );
  tearDown(() => backend.dispose());

  test('a trap in a parse is a fault, and the parser parses on', () {
    expect(() => backend.parse('x'), _throwsParse(4));
    expect(
      () => backend.parse('T'),
      _throwsParse(FlarkParseException.faultCode),
    );
    // The rebuilt instance answers.
    expect(() => backend.parse('x'), _throwsParse(4));
    expect(
      () => backend.parse('T again'),
      _throwsParse(FlarkParseException.faultCode),
    );
    expect(() => backend.parse('x'), _throwsParse(4));
  });

  test('a trap while the input grows is a fault, and the parser parses on', () {
    // 2,000 code units need a larger input buffer than the initial 4 KiB.
    expect(
      () => backend.parse('x' * 2000),
      _throwsParse(FlarkParseException.faultCode),
    );
    expect(() => backend.parse('x'), _throwsParse(4));
  });

  test('documents and editors meet a trap as they meet a native fault', () {
    // A read-only document shows text the parser cannot take as source.
    final document = FlarkReadDocument(backend, 'T');
    expect(document.sourceMode, isTrue);
    expect(document.source, 'T');
    expect(document.update('x'), isTrue);
    expect(document.sourceMode, isTrue);
    // An editor refuses to open it.
    expect(
      () => FlarkEditor(backend, text: 'T'),
      _throwsParse(FlarkParseException.faultCode),
    );
    // Disposing after a trap frees the rebuilt instance's buffers.
    backend.dispose();
    expect(() => backend.parse('x'), throwsStateError);
  });

  test('a session whose first parse traps fails rather than loads', () async {
    final session = FlarkSession(
      markdown: 'T',
      backendLoader: () async => FlarkBackendLease(backend, () {}),
    );
    addTearDown(session.dispose);
    await expectLater(
      session.ready,
      _throwsParse(FlarkParseException.faultCode),
    );
    expect(session.state.status, FlarkStatus.failed);
  });
}
