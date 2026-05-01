import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'native_comrak_ffi.dart';

const int _kAbiVersion = 1;
const int _kStatusOk = 0;

final class _NativeComrakResponse extends Struct {
  @Uint32()
  external int abiVersion;

  @Uint32()
  external int revision;

  @Uint16()
  external int statusCode;

  @Uint16()
  external int reserved;

  external Pointer<Uint8> payloadPtr;

  @Uint32()
  external int payloadLen;
}

typedef _NativeVersionFn = Uint32 Function();
typedef _NativeVersionDart = int Function();

typedef _NativeParseFn = Pointer<_NativeComrakResponse> Function(
  Uint32 revision,
  Uint8 profile,
  Pointer<Uint8> textPtr,
  Uint32 textLen,
);
typedef _NativeParseDart = Pointer<_NativeComrakResponse> Function(
  int revision,
  int profile,
  Pointer<Uint8> textPtr,
  int textLen,
);

typedef _NativeFreeFn = Void Function(Pointer<_NativeComrakResponse>);
typedef _NativeFreeDart = void Function(Pointer<_NativeComrakResponse>);

class _NativeComrakSymbols {
  final _NativeVersionDart bridgeVersion;
  final _NativeParseDart parse;
  final _NativeFreeDart freeResponse;

  const _NativeComrakSymbols({
    required this.bridgeVersion,
    required this.parse,
    required this.freeResponse,
  });

  factory _NativeComrakSymbols.fromLibrary(DynamicLibrary library) {
    return _NativeComrakSymbols(
      bridgeVersion:
          library.lookupFunction<_NativeVersionFn, _NativeVersionDart>(
        'sovereign_comrak_bridge_version',
      ),
      parse: library.lookupFunction<_NativeParseFn, _NativeParseDart>(
        'sovereign_comrak_parse',
      ),
      freeResponse: library.lookupFunction<_NativeFreeFn, _NativeFreeDart>(
        'sovereign_comrak_response_free',
      ),
    );
  }
}

class FfiNativeComrakBridge implements NativeComrakBridge {
  final _NativeComrakSymbols _symbols;
  final int _loadedAbiVersion;

  const FfiNativeComrakBridge._(this._symbols, this._loadedAbiVersion);

  factory FfiNativeComrakBridge.load({String? overrideLibraryPath}) {
    final loadContext = _openLibrary(overrideLibraryPath);
    final symbols = _lookupSymbols(loadContext);
    final loadedAbiVersion = _readAbiVersion(
      symbols,
      platform: loadContext.platform,
      libraryName: loadContext.libraryName,
      overrideLibraryPath: overrideLibraryPath,
      candidates: loadContext.candidates,
    );
    return FfiNativeComrakBridge._(symbols, loadedAbiVersion);
  }

  static _LoadedLibraryContext _openLibrary(String? overrideLibraryPath) {
    final platform = Platform.operatingSystem;
    if (overrideLibraryPath != null && overrideLibraryPath.isNotEmpty) {
      try {
        return _LoadedLibraryContext(
          library: DynamicLibrary.open(overrideLibraryPath),
          platform: platform,
          libraryName: _basename(overrideLibraryPath),
          candidates: [overrideLibraryPath],
        );
      } catch (error) {
        throw _buildLoadException(
          kind: File(overrideLibraryPath).existsSync()
              ? NativeComrakBridgeLoadFailureKind.loadFailed
              : NativeComrakBridgeLoadFailureKind.libraryNotFound,
          platform: platform,
          libraryName: _basename(overrideLibraryPath),
          overrideLibraryPath: overrideLibraryPath,
          candidates: [overrideLibraryPath],
          message: 'Failed to load native comrak bridge from override path.',
          cause: error,
        );
      }
    }

    if (Platform.isIOS) {
      // iOS bundles dynamic symbols into the process image.
      return _LoadedLibraryContext(
        library: DynamicLibrary.process(),
        platform: platform,
        libraryName: 'process()',
        candidates: const [],
      );
    }

    final libName = switch (platform) {
      'android' => 'libsovereign_comrak_bridge.so',
      'linux' => 'libsovereign_comrak_bridge.so',
      'macos' => 'libsovereign_comrak_bridge.dylib',
      'windows' => 'sovereign_comrak_bridge.dll',
      _ => throw _buildLoadException(
          kind: NativeComrakBridgeLoadFailureKind.unsupportedPlatform,
          platform: platform,
          libraryName: null,
          overrideLibraryPath: overrideLibraryPath,
          candidates: const [],
          message: 'Unsupported platform for native comrak bridge.',
        ),
    };
    final candidates = _candidateLibraryPaths(libName);

    try {
      return _LoadedLibraryContext(
        library: DynamicLibrary.open(libName),
        platform: platform,
        libraryName: libName,
        candidates: candidates,
      );
    } catch (error) {
      for (final candidate in candidates) {
        if (!File(candidate).existsSync()) continue;
        try {
          return _LoadedLibraryContext(
            library: DynamicLibrary.open(candidate),
            platform: platform,
            libraryName: libName,
            candidates: candidates,
          );
        } catch (candidateError) {
          throw _buildLoadException(
            kind: NativeComrakBridgeLoadFailureKind.loadFailed,
            platform: platform,
            libraryName: libName,
            overrideLibraryPath: null,
            candidates: candidates,
            message:
                'Found native comrak bridge candidate, but loading it failed.',
            cause: candidateError,
          );
        }
      }
      throw _buildLoadException(
        kind: NativeComrakBridgeLoadFailureKind.libraryNotFound,
        platform: platform,
        libraryName: libName,
        overrideLibraryPath: null,
        candidates: candidates,
        message: 'Failed to load native comrak bridge dynamic library.',
        cause: error,
      );
    }
  }

  static _NativeComrakSymbols _lookupSymbols(_LoadedLibraryContext context) {
    try {
      return _NativeComrakSymbols.fromLibrary(context.library);
    } catch (error) {
      throw _buildLoadException(
        kind: NativeComrakBridgeLoadFailureKind.symbolLookupFailed,
        platform: context.platform,
        libraryName: context.libraryName,
        overrideLibraryPath: null,
        candidates: context.candidates,
        message:
            'Native comrak bridge loaded, but required symbols were missing.',
        cause: error,
      );
    }
  }

  static int _readAbiVersion(
    _NativeComrakSymbols symbols, {
    required String platform,
    required String? libraryName,
    required String? overrideLibraryPath,
    required List<String> candidates,
  }) {
    final loadedAbiVersion = symbols.bridgeVersion();
    if (loadedAbiVersion != _kAbiVersion) {
      throw _buildLoadException(
        kind: NativeComrakBridgeLoadFailureKind.abiVersionMismatch,
        platform: platform,
        libraryName: libraryName,
        overrideLibraryPath: overrideLibraryPath,
        candidates: candidates,
        message:
            'Native comrak bridge ABI mismatch (expected $_kAbiVersion, got $loadedAbiVersion).',
      );
    }
    return loadedAbiVersion;
  }

  static List<String> _candidateLibraryPaths(String libName) {
    return <String>[
      Directory.current.uri
          .resolve('native/comrak_bridge/target/release/$libName')
          .toFilePath(),
      File(
        Platform.resolvedExecutable,
      ).parent.uri.resolve(libName).toFilePath(),
    ];
  }

  static String _basename(String path) {
    return File(path).uri.pathSegments.isEmpty
        ? path
        : File(path).uri.pathSegments.last;
  }

  static NativeComrakBridgeLoadException _buildLoadException({
    required NativeComrakBridgeLoadFailureKind kind,
    required String? platform,
    required String? libraryName,
    required String? overrideLibraryPath,
    required List<String> candidates,
    required String message,
    Object? cause,
  }) {
    final remediation = <String>[
      'For app builds, rebuild the app so sovereign_editor\'s build hook can compile and bundle native assets.',
      'For local package development, run ./scripts/build_comrak_all.sh --strict from the package root.',
      ...switch (platform) {
        'macos' => const <String>[
            'Verify libsovereign_comrak_bridge.dylib is bundled with the macOS app or exists in native/comrak_bridge/target/release for local package tests.',
          ],
        'linux' => const <String>[
            'Verify libsovereign_comrak_bridge.so is bundled with the Linux app or exists in native/comrak_bridge/target/release for local package tests.',
          ],
        'ios' => const <String>[
            'Verify native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework exists and is linked in the consuming app.',
            'Rebuild/reinstall the app so Dart FFI can resolve symbols via DynamicLibrary.process().',
          ],
        'android' => const <String>[
            'Verify native/comrak_bridge/dist/android/jniLibs/*/libsovereign_comrak_bridge.so exists and is packaged by the consuming app.',
            'Rebuild/reinstall the app after staging JNI libs.',
          ],
        _ => const <String>[],
      },
      if (kind == NativeComrakBridgeLoadFailureKind.symbolLookupFailed ||
          kind == NativeComrakBridgeLoadFailureKind.abiVersionMismatch)
        'The app and native bridge may be out of sync; rebuild native artifacts and reinstall the app.',
    ];
    return NativeComrakBridgeLoadException(
      kind: kind,
      message: message,
      platform: platform,
      libraryName: libraryName,
      overrideLibraryPath: overrideLibraryPath,
      candidatePaths: candidates,
      remediationSteps: remediation,
      cause: cause,
    );
  }

  @override
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input) async {
    if (_loadedAbiVersion != _kAbiVersion) {
      return NativeComrakParseResult(
        revision: input.revision,
        diagnostics: [
          NativeComrakDiagnostic(
            range: const NativeComrakRange(startByte: 0, endByte: 0),
            message:
                'ABI mismatch: bridge=$_kAbiVersion library=$_loadedAbiVersion.',
            code: 'COMRAK_ABI_MISMATCH',
            isError: true,
          ),
        ],
      );
    }

    final textBytes = input.utf8Text;
    final textPtr = calloc<Uint8>(textBytes.length);
    try {
      textPtr.asTypedList(textBytes.length).setAll(0, textBytes);
      final responsePtr = _symbols.parse(
        input.revision,
        _mapProfile(input.profile),
        textPtr,
        textBytes.length,
      );
      if (responsePtr == nullptr) {
        return NativeComrakParseResult(
          revision: input.revision,
          diagnostics: [
            NativeComrakDiagnostic(
              range: const NativeComrakRange(startByte: 0, endByte: 0),
              message: 'Native bridge returned null response pointer.',
              code: 'COMRAK_NULL_RESPONSE',
              isError: true,
            ),
          ],
        );
      }

      try {
        final response = responsePtr.ref;
        final payload = _copyPayload(response);
        NativeComrakParseResult result;
        try {
          result = NativeComrakPayloadCodec.decode(
            revision: response.revision,
            payload: payload,
          );
        } on FormatException catch (error) {
          result = NativeComrakParseResult(
            revision: response.revision,
            diagnostics: [
              NativeComrakDiagnostic(
                range: const NativeComrakRange(startByte: 0, endByte: 0),
                message: 'Failed to decode native payload: $error',
                code: 'COMRAK_PAYLOAD_DECODE_ERROR',
                isError: true,
              ),
            ],
          );
        }

        if (response.statusCode != _kStatusOk) {
          result = _appendDiagnostic(
            result,
            NativeComrakDiagnostic(
              range: const NativeComrakRange(startByte: 0, endByte: 0),
              message:
                  'Native parse failed with status ${response.statusCode}.',
              code: 'COMRAK_NATIVE_STATUS_${response.statusCode}',
              isError: true,
            ),
          );
        }
        return result;
      } finally {
        _symbols.freeResponse(responsePtr);
      }
    } finally {
      calloc.free(textPtr);
    }
  }

  static int _mapProfile(NativeComrakProfile profile) {
    return switch (profile) {
      NativeComrakProfile.commonMarkCore => 0,
      NativeComrakProfile.commonMarkGfm => 1,
    };
  }

  static Uint8List _copyPayload(_NativeComrakResponse response) {
    final len = response.payloadLen;
    if (len <= 0 || response.payloadPtr == nullptr) {
      return Uint8List(0);
    }
    return Uint8List.fromList(response.payloadPtr.asTypedList(len));
  }

  static NativeComrakParseResult _appendDiagnostic(
    NativeComrakParseResult result,
    NativeComrakDiagnostic diagnostic,
  ) {
    return NativeComrakParseResult(
      revision: result.revision,
      blocks: result.blocks,
      inlineTokens: result.inlineTokens,
      markerRanges: result.markerRanges,
      exclusionRanges: result.exclusionRanges,
      diagnostics: [...result.diagnostics, diagnostic],
    );
  }
}

NativeComrakBridge createNativeComrakBridge({String? overrideLibraryPath}) {
  return FfiNativeComrakBridge.load(overrideLibraryPath: overrideLibraryPath);
}

NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
}) {
  try {
    FfiNativeComrakBridge.load(overrideLibraryPath: overrideLibraryPath);
    return const NativeComrakBridgePreflightResult.available();
  } on NativeComrakBridgeLoadException catch (error) {
    return NativeComrakBridgePreflightResult.unavailable(error);
  } catch (error) {
    return NativeComrakBridgePreflightResult.unavailable(
      NativeComrakBridgeLoadException(
        kind: NativeComrakBridgeLoadFailureKind.loadFailed,
        message: 'Unexpected failure while preflighting native comrak bridge.',
        platform: Platform.operatingSystem,
        overrideLibraryPath: overrideLibraryPath,
        remediationSteps: const [
          'For app builds, rebuild the app so sovereign_editor\'s build hook can compile and bundle native assets.',
          'For local package development, run ./scripts/build_comrak_all.sh --strict from the package root.',
        ],
        cause: error,
      ),
    );
  }
}

class _LoadedLibraryContext {
  final DynamicLibrary library;
  final String platform;
  final String? libraryName;
  final List<String> candidates;

  const _LoadedLibraryContext({
    required this.library,
    required this.platform,
    required this.libraryName,
    required this.candidates,
  });
}
