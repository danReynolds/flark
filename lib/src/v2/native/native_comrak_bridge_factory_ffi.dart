import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';
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

typedef _NativeParseFn =
    Pointer<_NativeComrakResponse> Function(
      Uint32 revision,
      Uint8 profile,
      Pointer<Uint8> textPtr,
      Uint32 textLen,
    );
typedef _NativeParseDart =
    Pointer<_NativeComrakResponse> Function(
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
      bridgeVersion: library
          .lookupFunction<_NativeVersionFn, _NativeVersionDart>(
            'flark_comrak_bridge_version',
          ),
      parse: library.lookupFunction<_NativeParseFn, _NativeParseDart>(
        'flark_comrak_parse',
      ),
      freeResponse: library.lookupFunction<_NativeFreeFn, _NativeFreeDart>(
        'flark_comrak_response_free',
      ),
    );
  }
}

class FfiNativeComrakBridge implements ProfiledNativeComrakBridge {
  final _NativeComrakSymbols _symbols;
  final int _loadedAbiVersion;

  /// The load-time override path, kept so worker isolates re-resolve the
  /// same library (FFI function pointers cannot cross isolates).
  final String? _overrideLibraryPath;

  const FfiNativeComrakBridge._(
    this._symbols,
    this._loadedAbiVersion,
    this._overrideLibraryPath,
  );

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
    return FfiNativeComrakBridge._(
      symbols,
      loadedAbiVersion,
      overrideLibraryPath,
    );
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
      'android' => 'libflark_comrak_bridge.so',
      'linux' => 'libflark_comrak_bridge.so',
      'macos' => 'libflark_comrak_bridge.dylib',
      'windows' => 'flark_comrak_bridge.dll',
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
    final executableDirectory = File(Platform.resolvedExecutable).parent;
    final contentsDirectory = executableDirectory.parent;
    final currentDirectory = Directory.current;
    return <String>[
      currentDirectory.uri
          .resolve('native/comrak_bridge/target/release/$libName')
          .toFilePath(),
      currentDirectory.uri
          .resolve('../native/comrak_bridge/target/release/$libName')
          .toFilePath(),
      executableDirectory.uri.resolve(libName).toFilePath(),
      if (Platform.isMacOS)
        contentsDirectory.uri
            .resolve(
              'Frameworks/flark_comrak_bridge.framework/'
              'Versions/A/flark_comrak_bridge',
            )
            .toFilePath(),
      if (Platform.isMacOS)
        contentsDirectory.uri
            .resolve(
              'Frameworks/flark_comrak_bridge.framework/'
              'flark_comrak_bridge',
            )
            .toFilePath(),
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
      'For app builds, rebuild the app so flark\'s build hook can compile and bundle native assets.',
      'For local package development, run ./scripts/build_comrak_all.sh --strict from the package root.',
      ...switch (platform) {
        'macos' => const <String>[
          'Verify libflark_comrak_bridge.dylib is bundled with the macOS app or exists in native/comrak_bridge/target/release for local package tests.',
        ],
        'linux' => const <String>[
          'Verify libflark_comrak_bridge.so is bundled with the Linux app or exists in native/comrak_bridge/target/release for local package tests.',
        ],
        'ios' => const <String>[
          'Verify native/comrak_bridge/dist/ios/flark_comrak_bridge.xcframework exists and is linked in the consuming app.',
          'Rebuild/reinstall the app so Dart FFI can resolve symbols via DynamicLibrary.process().',
        ],
        'android' => const <String>[
          'Verify native/comrak_bridge/dist/android/jniLibs/*/libflark_comrak_bridge.so exists and is packaged by the consuming app.',
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
    return (await _parse(input, collectProfile: false)).result;
  }

  @override
  Future<NativeComrakProfiledParseResult> parseWithProfile(
    NativeComrakParseInput input,
  ) async {
    return _parse(input, collectProfile: true);
  }

  Future<NativeComrakProfiledParseResult> _parse(
    NativeComrakParseInput input, {
    required bool collectProfile,
  }) async {
    if (_loadedAbiVersion != _kAbiVersion) {
      return NativeComrakProfiledParseResult(
        result: NativeComrakParseResult(
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
        ),
        profile: NativeComrakBridgeParseProfile(
          total: Duration.zero,
          inputCopy: Duration.zero,
          nativeParse: Duration.zero,
          payloadCopy: Duration.zero,
          payloadDecode: Duration.zero,
          inputBytes: input.utf8Text.length,
          payloadBytes: 0,
        ),
      );
    }

    // See flarkNativeParseIsolateThresholdBytes for the threshold rationale
    // and the test escape hatch.
    if (input.utf8Text.length < flarkNativeParseIsolateThresholdBytes) {
      return _parseWithSymbols(_symbols, input, collectProfile: collectProfile);
    }

    // FFI function pointers cannot cross isolates, so the worker re-resolves
    // the library itself. dlopen of an already-loaded library is a refcount
    // bump and the lookups are trivial, so per-call setup is microseconds;
    // the main-isolate load() already validated loadability and ABI.
    final overrideLibraryPath = _overrideLibraryPath;
    return Isolate.run(() {
      final symbols = _lookupSymbols(_openLibrary(overrideLibraryPath));
      return _parseWithSymbols(symbols, input, collectProfile: collectProfile);
    });
  }

  static NativeComrakProfiledParseResult _parseWithSymbols(
    _NativeComrakSymbols symbols,
    NativeComrakParseInput input, {
    required bool collectProfile,
  }) {
    final totalStopwatch = collectProfile ? (Stopwatch()..start()) : null;
    Duration inputCopy = Duration.zero;
    Duration nativeParse = Duration.zero;
    Duration payloadCopy = Duration.zero;
    Duration payloadDecode = Duration.zero;
    var payloadBytes = 0;

    final textBytes = input.utf8Text;
    final textPtr = calloc<Uint8>(textBytes.length);
    try {
      final inputCopyStopwatch = collectProfile ? (Stopwatch()..start()) : null;
      textPtr.asTypedList(textBytes.length).setAll(0, textBytes);
      inputCopyStopwatch?.stop();
      inputCopy = inputCopyStopwatch?.elapsed ?? Duration.zero;
      final nativeStopwatch = collectProfile ? (Stopwatch()..start()) : null;
      final responsePtr = symbols.parse(
        input.revision,
        _mapProfile(input.profile),
        textPtr,
        textBytes.length,
      );
      nativeStopwatch?.stop();
      nativeParse = nativeStopwatch?.elapsed ?? Duration.zero;
      if (responsePtr == nullptr) {
        final result = NativeComrakParseResult(
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
        totalStopwatch?.stop();
        return NativeComrakProfiledParseResult(
          result: result,
          profile: NativeComrakBridgeParseProfile(
            total: totalStopwatch?.elapsed ?? Duration.zero,
            inputCopy: inputCopy,
            nativeParse: nativeParse,
            payloadCopy: payloadCopy,
            payloadDecode: payloadDecode,
            inputBytes: textBytes.length,
            payloadBytes: payloadBytes,
          ),
        );
      }

      try {
        final response = responsePtr.ref;
        payloadBytes = response.payloadLen;
        final payloadCopyStopwatch = collectProfile
            ? (Stopwatch()..start())
            : null;
        final payload = _copyPayload(response);
        payloadCopyStopwatch?.stop();
        payloadCopy = payloadCopyStopwatch?.elapsed ?? Duration.zero;
        NativeComrakParseResult result;
        try {
          final payloadDecodeStopwatch = collectProfile
              ? (Stopwatch()..start())
              : null;
          result = NativeComrakPayloadCodec.decode(
            revision: response.revision,
            payload: payload,
          );
          payloadDecodeStopwatch?.stop();
          payloadDecode = payloadDecodeStopwatch?.elapsed ?? Duration.zero;
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
          result = result.withDiagnostic(
            NativeComrakDiagnostic(
              range: const NativeComrakRange(startByte: 0, endByte: 0),
              message:
                  'Native parse failed with status ${response.statusCode}.',
              code: 'COMRAK_NATIVE_STATUS_${response.statusCode}',
              isError: true,
            ),
          );
        }
        totalStopwatch?.stop();
        return NativeComrakProfiledParseResult(
          result: result,
          profile: NativeComrakBridgeParseProfile(
            total: totalStopwatch?.elapsed ?? Duration.zero,
            inputCopy: inputCopy,
            nativeParse: nativeParse,
            payloadCopy: payloadCopy,
            payloadDecode: payloadDecode,
            inputBytes: textBytes.length,
            payloadBytes: payloadBytes,
          ),
        );
      } finally {
        symbols.freeResponse(responsePtr);
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
          'For app builds, rebuild the app so flark\'s build hook can compile and bundle native assets.',
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
