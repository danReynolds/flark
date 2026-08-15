@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:test/test.dart';

extension type _FetchResponse(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSString get statusText;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

extension type _WasmExportDescriptor(JSObject _) implements JSObject {
  external JSString get name;
  external JSString get kind;
}

extension type _WasmInstance(JSObject _) implements JSObject {
  external JSObject get exports;
}

extension type _WasmFunction(JSObject _) implements JSObject {
  external JSNumber get length;
}

void main() {
  test(
    'staged Wasm exposes the stable scalar v3 endpoint and host ABI',
    () async {
      final wasmUri = Uri.base.resolve(
        'packages/flark/assets/wasm/flark_comrak_bridge.wasm',
      );
      final response =
          await (globalContext
                      .getProperty<JSFunction>('fetch'.toJS)
                      .callAsFunction(globalContext, wasmUri.toString().toJS)
                  as JSPromise<_FetchResponse>)
              .toDart;
      expect(
        response.ok.toDart,
        isTrue,
        reason:
            'Failed to fetch $wasmUri: ${response.status.toDartInt} '
            '${response.statusText.toDart}',
      );

      final bytes = await response.arrayBuffer().toDart;
      final webAssembly = globalContext.getProperty<JSObject>(
        'WebAssembly'.toJS,
      );
      final module =
          await (webAssembly
                      .getProperty<JSFunction>('compile'.toJS)
                      .callAsFunction(webAssembly, bytes)
                  as JSPromise<JSObject>)
              .toDart;
      final moduleType = webAssembly.getProperty<JSObject>('Module'.toJS);
      final exportDescriptors =
          (moduleType
                      .getProperty<JSFunction>('exports'.toJS)
                      .callAsFunction(moduleType, module)
                  as JSArray<_WasmExportDescriptor>)
              .toDart;
      final exports = <String, String>{
        for (final descriptor in exportDescriptors)
          descriptor.name.toDart: descriptor.kind.toDart,
      };

      expect(exports['memory'], 'memory');
      for (final name in _requiredFunctionArities.keys) {
        expect(
          exports[name],
          'function',
          reason: 'The staged Wasm module is missing required v3 export $name.',
        );
      }
      for (final name in const <String>[
        'flark_v3_wasm_host_role_record_count',
        'flark_v3_wasm_host_read_role_record',
      ]) {
        expect(
          exports,
          isNot(contains(name)),
          reason:
              '$name leaks raw role enumeration into the Web ABI. '
              'Structural queries must use the bounded point-query export.',
        );
      }
      expect(
        exports.keys.where(
          (name) =>
              name.startsWith('flark_v3_endpoint_') ||
              name.startsWith('flark_v3_host_'),
        ),
        isEmpty,
        reason:
            'Native aggregate/finalizer ABI symbols must not leak into Wasm. '
            'Web callers use only scalar flark_v3_wasm_* wrappers.',
      );

      final instance =
          await (webAssembly
                      .getProperty<JSFunction>('instantiate'.toJS)
                      .callAsFunction(webAssembly, module, JSObject())
                  as JSPromise<_WasmInstance>)
              .toDart;
      for (final MapEntry(key: name, value: arity)
          in _requiredFunctionArities.entries) {
        final function = _WasmFunction(
          instance.exports.getProperty<JSObject>(name.toJS),
        );
        expect(
          function.length.toDartInt,
          arity,
          reason: 'The staged Wasm export $name changed scalar ABI arity.',
        );
      }

      final imports =
          (moduleType
                      .getProperty<JSFunction>('imports'.toJS)
                      .callAsFunction(moduleType, module)
                  as JSArray<JSObject>)
              .toDart;
      expect(
        imports,
        isEmpty,
        reason:
            'The packaged v3 module must instantiate with an empty import object '
            'in both the Worker and main-context host.',
      );
    },
  );
}

const _requiredFunctionArities = <String, int>{
  'flark_v3_wasm_alloc': 1,
  'flark_v3_wasm_free': 2,
  'flark_v3_wasm_checkpoint_b_probe': 3,
  'flark_v3_wasm_endpoint_native_abi_version': 0,
  'flark_v3_wasm_endpoint_config_standard': 1,
  'flark_v3_wasm_endpoint_create': 2,
  'flark_v3_wasm_endpoint_recover': 3,
  'flark_v3_wasm_endpoint_dispatch': 5,
  'flark_v3_wasm_endpoint_dispatch_host_poll': 5,
  'flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll': 5,
  'flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll': 5,
  'flark_v3_wasm_endpoint_close': 5,
  'flark_v3_wasm_endpoint_poll': 6,
  'flark_v3_wasm_endpoint_poll_candidate': 4,
  'flark_v3_wasm_endpoint_encode': 5,
  'flark_v3_wasm_endpoint_remove': 2,
  'flark_v3_wasm_endpoint_emergency_destroy': 2,
  'flark_v3_wasm_host_native_abi_version': 0,
  'flark_v3_wasm_host_config_standard': 1,
  'flark_v3_wasm_host_create': 2,
  'flark_v3_wasm_host_observe_source': 4,
  'flark_v3_wasm_host_begin_offer': 4,
  'flark_v3_wasm_host_begin_inline_sidecar_offer': 4,
  'flark_v3_wasm_host_begin_viewport_presentation_offer': 4,
  'flark_v3_wasm_host_begin_references_delta': 5,
  'flark_v3_wasm_host_begin_exact_base_delta': 5,
  'flark_v3_wasm_host_admit_packet': 5,
  'flark_v3_wasm_host_admit_inline_sidecar_packet': 5,
  'flark_v3_wasm_host_admit_viewport_presentation_packet': 5,
  'flark_v3_wasm_host_request_commit': 4,
  'flark_v3_wasm_host_request_inline_sidecar_commit': 4,
  'flark_v3_wasm_host_request_viewport_presentation_commit': 4,
  'flark_v3_wasm_host_abort_offer': 4,
  'flark_v3_wasm_host_abort_inline_sidecar_offer': 4,
  'flark_v3_wasm_host_abort_viewport_presentation_offer': 4,
  'flark_v3_wasm_host_poll': 6,
  'flark_v3_wasm_host_poll_inline_sidecar': 6,
  'flark_v3_wasm_host_poll_viewport_presentation': 6,
  'flark_v3_wasm_host_acknowledge_delivery': 4,
  'flark_v3_wasm_host_acknowledge_inline_sidecar_delivery': 4,
  'flark_v3_wasm_host_acknowledge_viewport_presentation_delivery': 4,
  'flark_v3_wasm_host_query_structural': 6,
  'flark_v3_wasm_host_query_structural_range': 6,
  'flark_v3_wasm_host_query_structural_ordinal_window': 4,
  'flark_v3_wasm_host_query_inline_sidecar': 6,
  'flark_v3_wasm_host_query_viewport_presentation': 6,
  'flark_v3_wasm_host_close': 3,
  'flark_v3_wasm_host_remove': 2,
  'flark_v3_wasm_host_emergency_destroy': 2,
};
