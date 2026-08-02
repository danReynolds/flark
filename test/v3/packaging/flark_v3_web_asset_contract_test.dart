import 'dart:io';

import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:test/test.dart';

import '../../../tool/flark_wasm_buildinfo.dart';

const _rootWasmPath = 'lib/assets/wasm/flark_comrak_bridge.wasm';
const _flutterWasmPath =
    'packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm';
const _rootWorkerPath = 'lib/assets/worker/flark_v3_parser_worker.js';
const _flutterWorkerPath =
    'packages/flark_flutter/lib/assets/worker/flark_v3_parser_worker.js';
const _rootBuildinfoPath = 'lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo';
const _flutterBuildinfoPath =
    'packages/flark_flutter/lib/assets/wasm/'
    'flark_comrak_bridge.wasm.buildinfo';
const _engineLabAssetVersionPath =
    'example/lib/v3_engine_lab_web_asset_version.dart';

void main() {
  test('Wasm freshness closure includes the stable v3 Web wrapper', () {
    final inputs = flarkWasmSourceInputs(
      Directory('native/comrak_bridge'),
    ).map((file) => file.absolute.path.replaceAll('\\', '/')).toList();
    expect(
      inputs,
      contains(endsWith('/native/comrak_bridge/src/v3_wasm_api.rs')),
      reason:
          'The staged module freshness receipt must invalidate when its v3 '
          'scalar Web ABI changes.',
    );
  });

  test(
    'runtime keeps external Worker and Wasm URLs independently explicit',
    () {
      final assets = FlarkV3WebRuntimeAssets(
        workerUri: Uri.parse('https://static.example/flark/worker-v7.js'),
        wasmUri: Uri.parse('https://modules.example/flark/parser-v11.wasm'),
      );

      expect(
        assets.workerUri,
        Uri.parse('https://static.example/flark/worker-v7.js'),
      );
      expect(
        assets.wasmUri,
        Uri.parse('https://modules.example/flark/parser-v11.wasm'),
      );

      final defaults = FlarkV3WebRuntimeAssets.packageDefaults(
        baseUri: Uri.parse('https://tests.example/suite/'),
      );
      expect(
        defaults.workerUri,
        Uri.parse(
          'https://tests.example/suite/packages/flark/assets/worker/'
          'flark_v3_parser_worker.js',
        ),
      );
      expect(
        defaults.wasmUri,
        Uri.parse(
          'https://tests.example/suite/packages/flark/assets/wasm/'
          'flark_comrak_bridge.wasm',
        ),
      );
    },
  );

  test('Web query path delegates bounded structural selection to Rust', () {
    final source = File(
      'lib/src/v3/runtime/web/flark_v3_web_host_store.dart',
    ).readAsStringSync();

    expect(source, contains('flark_v3_wasm_host_query_structural'));
    expect(
      source,
      isNot(contains('flark_v3_wasm_host_role_record_count')),
      reason: 'Raw role enumeration is not part of the public Web query ABI.',
    );
    expect(
      source,
      isNot(contains('flark_v3_wasm_host_read_role_record')),
      reason:
          'Dart must not reconstruct structural viewports from role ordinal 0.',
    );
  });

  test('root web assets exist and Flutter mirrors their exact bytes', () {
    final rootWasm = File(_rootWasmPath);
    final flutterWasm = File(_flutterWasmPath);
    final rootWorker = File(_rootWorkerPath);
    final flutterWorker = File(_flutterWorkerPath);
    final rootBuildinfo = File(_rootBuildinfoPath);
    final flutterBuildinfo = File(_flutterBuildinfoPath);

    for (final asset in <File>[
      rootWasm,
      flutterWasm,
      rootWorker,
      flutterWorker,
      rootBuildinfo,
      flutterBuildinfo,
    ]) {
      expect(
        asset.existsSync(),
        isTrue,
        reason:
            'Missing packaged web asset ${asset.path}. Run '
            './scripts/build_comrak_wasm.sh after updating the canonical root '
            'Worker or Rust module.',
      );
    }

    final rootWasmBytes = rootWasm.readAsBytesSync();
    expect(rootWasmBytes, hasLength(greaterThan(8)));
    expect(rootWasmBytes.take(4), orderedEquals(const <int>[0, 97, 115, 109]));
    expect(
      flutterWasm.readAsBytesSync(),
      orderedEquals(rootWasmBytes),
      reason: 'The Flutter Wasm mirror diverged from the root build authority.',
    );

    final rootWorkerBytes = rootWorker.readAsBytesSync();
    expect(rootWorkerBytes, isNotEmpty);
    expect(
      flutterWorker.readAsBytesSync(),
      orderedEquals(rootWorkerBytes),
      reason:
          'The Flutter Worker mirror diverged from the canonical root Worker.',
    );
    expect(
      flutterBuildinfo.readAsBytesSync(),
      orderedEquals(rootBuildinfo.readAsBytesSync()),
      reason:
          'The Flutter Wasm build receipt diverged from the root build '
          'authority.',
    );
  });

  test('staged Wasm does not embed the source checkout path', () {
    final checkoutPath = Directory.current.absolute.path.replaceAll('\\', '/');
    for (final path in const [_rootWasmPath, _flutterWasmPath]) {
      final bytes = File(path).readAsBytesSync();
      final searchableBytes = String.fromCharCodes(bytes);
      expect(
        searchableBytes,
        isNot(contains(checkoutPath)),
        reason:
            '$path contains the absolute source checkout. Build with a Rust '
            'path remap so published archives remain relocatable and '
            'reproducible.',
      );
    }
  });

  test('parser, endpoint, host, and Dart share one grammar revision', () {
    final parserSource = File(
      'native/comrak_bridge/crates/flark_parser/src/exact_clean.rs',
    ).readAsStringSync();
    final parserRevisionMatch = RegExp(
      r'pub const M11_GRAMMAR_REVISION: u32 = ([0-9]+);',
    ).firstMatch(parserSource);
    expect(
      parserRevisionMatch,
      isNotNull,
      reason: 'The parser grammar revision declaration is missing.',
    );
    expect(
      int.parse(parserRevisionMatch!.group(1)!),
      9,
      reason: 'Current parser publication requires parser grammar revision 9.',
    );
    expect(
      flarkV3CurrentGrammarRevision,
      9,
      reason:
          'The Dart parser transport must pin the current grammar generation.',
    );
    expect(
      flarkV3CurrentGrammarRevision,
      int.parse(parserRevisionMatch.group(1)!),
      reason:
          'Dart host/publication authority must advance with parser restart '
          'authority before rebuilding native and Wasm artifacts.',
    );

    final bridgeSource = File(
      'native/comrak_bridge/src/lib.rs',
    ).readAsStringSync();
    expect(
      bridgeSource,
      contains(
        'FLARK_V3_GRAMMAR_REVISION: u32 = '
        'flark_parser::M11_GRAMMAR_REVISION',
      ),
    );
    expect(
      File(
        'native/comrak_bridge/src/v3_candidate_endpoint.rs',
      ).readAsStringSync(),
      contains('GRAMMAR_REVISION: u32 = crate::FLARK_V3_GRAMMAR_REVISION'),
    );
    expect(
      File('native/comrak_bridge/src/v3_host_native_api.rs').readAsStringSync(),
      contains('grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION'),
    );

    for (final sourcePath in <String>[
      'lib/src/v3/runtime/public/flark_v3_document_runtime.dart',
      'lib/src/v3/runtime/native/flark_v3_native_host_store.dart',
      'lib/src/v3/runtime/web/flark_v3_web_host_store.dart',
    ]) {
      expect(
        File(sourcePath).readAsStringSync(),
        contains('flarkV3CurrentGrammarRevision'),
        reason: '$sourcePath must derive from the shared Dart authority.',
      );
    }
  });

  test('grammar revision 9 keeps reference facts additive', () {
    final engineInlineSource = File(
      'native/comrak_bridge/crates/flark_engine/src/inline_projection.rs',
    ).readAsStringSync();
    final parserGrammarSource = File(
      'native/comrak_bridge/crates/flark_parser/src/exact_clean.rs',
    ).readAsStringSync();
    final parserInternalSource = File(
      'native/comrak_bridge/crates/flark_engine/src/parser_internal.rs',
    ).readAsStringSync();
    final inlineDirectSource = File(
      'native/comrak_bridge/crates/flark_parser/src/inline_direct.rs',
    ).readAsStringSync();
    final candidateEndpointSource = File(
      'native/comrak_bridge/src/v3_candidate_endpoint.rs',
    ).readAsStringSync();
    final blockSource = File(
      'native/comrak_bridge/crates/flark_engine/src/block_sequence.rs',
    ).readAsStringSync();
    final parserPublicationSource = File(
      'native/comrak_bridge/crates/flark_parser/src/publication.rs',
    ).readAsStringSync();
    final hostStoreSource = File(
      'native/comrak_bridge/src/v3_host_store.rs',
    ).readAsStringSync();
    final publicationWireSource = File(
      'native/comrak_bridge/src/v3_publication_wire.rs',
    ).readAsStringSync();
    final nativeHostApiSource = File(
      'native/comrak_bridge/src/v3_host_native_api.rs',
    ).readAsStringSync();
    final documentQuerySource = File(
      'lib/src/v3/runtime/public/flark_v3_document_query.dart',
    ).readAsStringSync();
    final inlineFactsSource = File(
      'lib/src/v3/runtime/public/flark_v3_inline_facts.dart',
    ).readAsStringSync();
    final hotSidecarSource = File(
      'lib/src/v3/host/flark_v3_hot_inline_sidecar_protocol.dart',
    ).readAsStringSync();
    final nativeStoreSource = File(
      'lib/src/v3/runtime/native/flark_v3_native_host_store.dart',
    ).readAsStringSync();
    final webStoreSource = File(
      'lib/src/v3/runtime/web/flark_v3_web_host_store.dart',
    ).readAsStringSync();

    expect(
      engineInlineSource,
      allOf(
        contains(
          'const INLINE_PROJECTION_STREAM_TAG: u32 = '
          'u32::from_le_bytes(*b"IFO2");',
        ),
        contains('const INLINE_PROJECTION_PAGE_MAGIC: [u8; 4] = *b"IFP2";'),
        contains('const INLINE_PROJECTION_SCHEMA: u32 = 2;'),
        contains(
          'const PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_MAGIC: [u8; 4] = '
          '*b"IPB5";',
        ),
        contains(
          'const PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_VERSION: u32 = 1;',
        ),
        allOf(
          contains(
            'pub(crate) const PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA: u32 = '
            '5;',
          ),
          contains(
            'const INLINE_LINK_VALUE_STREAM_TAG: u32 = '
            'u32::from_le_bytes(*b"ILV1");',
          ),
          contains('output[..8].copy_from_slice(b"FLKIV001");'),
        ),
      ),
      reason:
          'The stable fact pages and current dual-root value bundle must '
          'remain coherent.',
    );
    expect(
      engineInlineSource,
      allOf(
        allOf(
          contains('BackslashEscape = 7,'),
          contains('7 => Ok(Self::BackslashEscape),'),
          contains('HardLineBreak = 8,'),
          contains('8 => Ok(Self::HardLineBreak),'),
          contains('CharacterReference = 9,'),
          contains('9 => Ok(Self::CharacterReference),'),
          allOf(
            contains('DirectLink = 10,'),
            contains('10 => Ok(Self::DirectLink),'),
            contains('DirectImage = 11,'),
            contains('11 => Ok(Self::DirectImage),'),
            allOf(
              contains('ReferenceLink = 12,'),
              contains('12 => Ok(Self::ReferenceLink),'),
              contains('ReferenceImage = 13,'),
              contains('13 => Ok(Self::ReferenceImage),'),
            ),
          ),
        ),
        allOf(
          contains('pub fn new_character_reference('),
          contains('M11InlineProjectionFactPayload::CharacterReference'),
          contains(
            'relative_len != 2 || content_offset != 1 || content_len != 1 || '
            'closer_len != 0',
          ),
          isNot(contains('*b"IFO3"')),
          isNot(contains('*b"IFP3"')),
        ),
      ),
      reason:
          'Kinds 7 through 13 are additive within the existing schema-2 kind '
          'byte and 20-byte record. The current grammar revision partitions '
          'the global reference-link/image semantics; '
          'the IFO2/IFP2 family must not be bumped.',
    );
    expect(
      parserGrammarSource,
      contains('pub const M11_GRAMMAR_REVISION: u32 = 9;'),
    );
    expect(
      flarkV3CurrentGrammarRevision,
      9,
      reason:
          'Dart must reject parser bindings without the reference-resolution '
          'grammar while retaining the additive inline record family.',
    );
    expect(
      parserInternalSource,
      allOf(
        contains('pub enum M11ReferenceResolution'),
        contains('ValueTooLarge'),
        contains('pub struct M11ReferenceResolver'),
      ),
      reason:
          'Reference lookup must distinguish a missing label from a real '
          'winner whose cooked value cannot fit the bounded value lane.',
    );
    expect(
      inlineDirectSource,
      allOf(
        contains('M11InlineDirectKind::ReferenceLink'),
        contains('M11InlineDirectKind::ReferenceImage'),
        contains('M11ReferenceResolution::ValueTooLarge'),
        contains('self.exhaustive_bracket_classification = false'),
      ),
      reason:
          'Reference uses must be parser-resolved and fail the whole leaf '
          'closed when the bounded semantic value cannot be represented.',
    );
    expect(
      candidateEndpointSource,
      contains('HotInlineState::AwaitingReferenceResolver'),
      reason:
          'The endpoint must await the revision-owned resolver instead of '
          'asking Dart to predict reference semantics.',
    );
    expect(
      blockSource,
      contains(
        'pub(crate) const PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA: u32 = 3;',
      ),
    );
    expect(
      engineInlineSource,
      isNot(
        contains(
          'pub(crate) const PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA: u32 = '
          '3;',
        ),
      ),
      reason:
          'Persistent inline Projection schema 5 must remain distinct from '
          'the block Projection schema 3 route.',
    );

    expect(
      parserPublicationSource,
      allOf(
        contains('pub const M11_INLINE_META_MAGIC: &[u8; 8] = b"FLKIN002";'),
        contains('pub const M11_INLINE_PAGE_MAGIC: &[u8; 8] = b"FLKIP002";'),
        contains('pub const M11_INLINE_SCHEMA: u32 = 2;'),
        isNot(contains('b"FLKIN003"')),
        isNot(contains('b"FLKIP003"')),
      ),
    );
    expect(_dartByteListMagic(documentQuerySource, '_inlineMagic'), 'FLKIN002');
    expect(documentQuerySource, contains('const int _inlineSchema = 2;'));

    expect(
      hostStoreSource,
      allOf(
        contains('const M11_VIEWPORT_INLINE_SCHEMA: u32 = 8;'),
        contains('matches!(kind, 1..=8)'),
        contains('7 => end - start == 2 && opener == 1 && closer == 0'),
        contains(
          '8 => opener > 0 && '
          'matches!(content_end - content_start, 1 | 2) && closer == 0',
        ),
      ),
    );
    expect(documentQuerySource, contains('const int _viewportSchemaV8 = 8;'));
    expect(
      inlineFactsSource,
      allOf(
        allOf(
          contains('7 => FlarkV3InlineFactKind.escapedPunctuation'),
          contains('8 => FlarkV3InlineFactKind.hardLineBreak'),
          contains('12 => FlarkV3InlineFactKind.referenceLink'),
          contains('13 => FlarkV3InlineFactKind.referenceImage'),
        ),
        allOf(
          contains('FlarkV3InlineFactKind.escapedPunctuation =>'),
          contains('FlarkV3InlineFactKind.hardLineBreak =>'),
          contains(
            'openerBytes == 1 && contentLength == 1 && closerBytes == 0',
          ),
          contains('parent.kind == FlarkV3InlineFactKind.escapedPunctuation'),
          contains('parent.kind == FlarkV3InlineFactKind.hardLineBreak'),
        ),
      ),
      reason:
          'Dart must decode kinds 7 and 8 as atomic collapsed-closer facts '
          'with no child facts.',
    );

    expect(
      publicationWireSource,
      contains('pub const HOT_INLINE_SIDECAR_SCHEMA: u32 = 3;'),
    );
    expect(hotSidecarSource, contains('static const int supportedSchema = 3;'));
    expect(
      nativeHostApiSource,
      contains('pub const FLARK_V3_HOST_INLINE_SIDECAR_QUERY_SCHEMA: u32 = 3;'),
    );
    for (final (path, source) in <(String, String)>[
      (
        'lib/src/v3/runtime/native/flark_v3_native_host_store.dart',
        nativeStoreSource,
      ),
      ('lib/src/v3/runtime/web/flark_v3_web_host_store.dart', webStoreSource),
    ]) {
      expect(
        source,
        contains('const int _inlineSidecarQuerySchema = 3;'),
        reason: '$path must match the Rust inline-sidecar query generation.',
      );
    }
  });

  test('engine lab cache key matches the staged Worker and Wasm set', () {
    final version = flarkV3WebAssetVersion(
      wasm: File(_rootWasmPath),
      wasmBuildinfo: File(_rootBuildinfoPath),
      worker: File(_rootWorkerPath),
    );
    final generated = File(_engineLabAssetVersionPath).readAsStringSync();

    expect(
      generated,
      allOf(contains('v3EngineLabWebAssetVersion'), contains("'$version';")),
      reason:
          'The engine lab asset URL fingerprint is stale. Rebuild Wasm with '
          './scripts/build_comrak_wasm.sh so browsers cannot retain an older '
          'Worker or Wasm module.',
    );
  });

  test('Flutter declares both mirrored web assets', () {
    final pubspec = File(
      'packages/flark_flutter/pubspec.yaml',
    ).readAsStringSync();
    expect(pubspec, contains('- lib/assets/wasm/flark_comrak_bridge.wasm'));
    expect(pubspec, contains('- lib/assets/worker/flark_v3_parser_worker.js'));
  });

  test('external Worker source is compatible with a strict CSP boot path', () {
    final source = File(_rootWorkerPath).readAsStringSync();
    final forbidden = <({RegExp pattern, String construct})>[
      (pattern: RegExp(r'\bimportScripts\s*\('), construct: 'importScripts'),
      (
        pattern: RegExp(r'\b(?:eval|Function)\s*\('),
        construct: 'dynamic code evaluation',
      ),
      (pattern: RegExp(r'\bnew\s+Function\s*\('), construct: 'new Function'),
      (
        pattern: RegExp(r'\bnew\s+Blob\s*\('),
        construct: 'Blob-backed Worker source',
      ),
      (
        pattern: RegExp(r'\bURL\.createObjectURL\s*\('),
        construct: 'object-URL Worker source',
      ),
      (pattern: RegExp(r'''["'](?:blob|data):'''), construct: 'blob/data URL'),
      (
        pattern: RegExp(r'^\s*(?:import|export)\s', multiLine: true),
        construct: 'module-only import/export syntax',
      ),
      (pattern: RegExp(r'\bimport\s*\('), construct: 'dynamic module import'),
    ];

    for (final (:pattern, :construct) in forbidden) {
      expect(
        pattern.hasMatch(source),
        isFalse,
        reason:
            'The packaged Worker uses $construct. V3 must boot as one external '
            'classic Worker with an independently configured Wasm URI.',
      );
    }
  });

  test('Worker preserves a dedicated inline-sidecar host-poll byte lane', () {
    final source = File(_rootWorkerPath).readAsStringSync();
    expect(source, contains('COMMAND_DISPATCH_INLINE_SIDECAR_HOST_POLL = 7'));
    expect(
      source,
      contains('flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll'),
    );
    expect(source, contains("operation = 'dispatchInlineSidecarHostPoll'"));
  });

  test('Worker preserves a dedicated viewport host-poll byte lane', () {
    final source = File(_rootWorkerPath).readAsStringSync();
    expect(
      source,
      contains('COMMAND_DISPATCH_VIEWPORT_PRESENTATION_HOST_POLL = 8'),
    );
    expect(
      source,
      contains(
        'flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll',
      ),
    );
    expect(
      source,
      contains("operation = 'dispatchViewportPresentationHostPoll'"),
    );
  });
}

String _dartByteListMagic(String source, String name) {
  final match = RegExp(
    'const List<int> ${RegExp.escape(name)} = <int>\\[([^\\]]+)\\];',
  ).firstMatch(source);
  expect(match, isNotNull, reason: 'Missing Dart byte-list magic $name.');
  return String.fromCharCodes(
    RegExp(
      r'[0-9]+',
    ).allMatches(match!.group(1)!).map((value) => int.parse(value.group(0)!)),
  );
}
