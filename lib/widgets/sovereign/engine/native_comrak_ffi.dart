import 'dart:convert';

import 'package:flutter/foundation.dart';

/// Native parse profile for comrak bridge calls.
enum NativeComrakProfile {
  /// CommonMark core compliance mode.
  commonMarkCore,

  /// GitHub Flavored Markdown extension mode.
  commonMarkGfm,
}

/// Input payload passed to the native comrak bridge.
@immutable
class NativeComrakParseInput {
  /// Controller revision associated with [utf8Text].
  final int revision;

  /// Markdown profile requested for the native parse.
  final NativeComrakProfile profile;

  /// Markdown source encoded as UTF-8 bytes.
  final Uint8List utf8Text;

  /// Creates a native parse request.
  const NativeComrakParseInput({
    required this.revision,
    required this.profile,
    required this.utf8Text,
  }) : assert(revision >= 0);
}

/// UTF-8 byte range emitted by the native parser.
@immutable
class NativeComrakRange {
  /// Inclusive start byte offset.
  final int startByte;

  /// Exclusive end byte offset.
  final int endByte;

  /// Creates a byte range over `[startByte, endByte)`.
  const NativeComrakRange({required this.startByte, required this.endByte})
      : assert(startByte >= 0),
        assert(endByte >= 0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is NativeComrakRange &&
          startByte == other.startByte &&
          endByte == other.endByte;

  @override
  int get hashCode => Object.hash(startByte, endByte);
}

/// Structural block span emitted by the native parser.
@immutable
class NativeComrakBlockSpan {
  /// Native block type name.
  final String type;

  /// Source byte range for the block.
  final NativeComrakRange range;

  /// Optional native block metadata.
  final Map<String, Object?> payload;

  /// Creates a native block span.
  const NativeComrakBlockSpan({
    required this.type,
    required this.range,
    this.payload = const {},
  });

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is NativeComrakBlockSpan &&
          type == other.type &&
          range == other.range &&
          mapEquals(payload, other.payload);

  @override
  int get hashCode => Object.hash(
        type,
        range,
        Object.hashAllUnordered(
          payload.entries.map((entry) => Object.hash(entry.key, entry.value)),
        ),
      );
}

/// Inline style token emitted by the native parser.
@immutable
class NativeComrakInlineToken {
  /// Source byte range for the token.
  final NativeComrakRange range;

  /// Native style names applied to [range].
  final Set<String> styles;

  /// Creates a native inline token.
  const NativeComrakInlineToken({required this.range, required this.styles});

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is NativeComrakInlineToken &&
          range == other.range &&
          setEquals(styles, other.styles);

  @override
  int get hashCode => Object.hash(range, Object.hashAllUnordered(styles));
}

/// Diagnostic emitted by the native parser.
@immutable
class NativeComrakDiagnostic {
  /// Source byte range associated with the diagnostic.
  final NativeComrakRange range;

  /// Human-readable diagnostic message.
  final String message;

  /// Optional stable diagnostic code.
  final String? code;

  /// Whether this diagnostic represents an error instead of a warning.
  final bool isError;

  /// Creates a native diagnostic.
  const NativeComrakDiagnostic({
    required this.range,
    required this.message,
    this.code,
    this.isError = false,
  });

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is NativeComrakDiagnostic &&
          range == other.range &&
          message == other.message &&
          code == other.code &&
          isError == other.isError;

  @override
  int get hashCode => Object.hash(range, message, code, isError);
}

/// Parsed native markdown payload for a controller revision.
@immutable
class NativeComrakParseResult {
  /// Controller revision represented by this result.
  final int revision;

  /// Structural block spans.
  final List<NativeComrakBlockSpan> blocks;

  /// Inline style tokens.
  final List<NativeComrakInlineToken> inlineTokens;

  /// Source marker byte ranges.
  final List<NativeComrakRange> markerRanges;

  /// Byte ranges excluded from normal inline styling.
  final List<NativeComrakRange> exclusionRanges;

  /// Diagnostics emitted by the native parser.
  final List<NativeComrakDiagnostic> diagnostics;

  /// Creates a native parse result.
  const NativeComrakParseResult({
    required this.revision,
    this.blocks = const [],
    this.inlineTokens = const [],
    this.markerRanges = const [],
    this.exclusionRanges = const [],
    this.diagnostics = const [],
  }) : assert(revision >= 0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is NativeComrakParseResult &&
          revision == other.revision &&
          listEquals(blocks, other.blocks) &&
          listEquals(inlineTokens, other.inlineTokens) &&
          listEquals(markerRanges, other.markerRanges) &&
          listEquals(exclusionRanges, other.exclusionRanges) &&
          listEquals(diagnostics, other.diagnostics);

  @override
  int get hashCode => Object.hash(
        revision,
        Object.hashAll(blocks),
        Object.hashAll(inlineTokens),
        Object.hashAll(markerRanges),
        Object.hashAll(exclusionRanges),
        Object.hashAll(diagnostics),
      );
}

/// JSON codec for the native bridge payload contract.
class NativeComrakPayloadCodec {
  /// This codec only exposes static helpers.
  const NativeComrakPayloadCodec._();

  /// Decodes a native JSON [payload] for [revision].
  static NativeComrakParseResult decode({
    required int revision,
    required Uint8List payload,
  }) {
    if (payload.isEmpty) {
      return NativeComrakParseResult(revision: revision);
    }

    final decoded = jsonDecode(utf8.decode(payload));
    if (decoded is! Map<String, dynamic>) {
      throw const FormatException(
        'Native comrak payload must be a JSON object',
      );
    }

    List<Map<String, dynamic>> mapList(dynamic raw) {
      if (raw is! List) return const [];
      return raw.whereType<Map<String, dynamic>>().toList(growable: false);
    }

    int readInt(dynamic value, {int fallback = 0}) {
      if (value is int) return value;
      if (value is num) return value.toInt();
      return fallback;
    }

    NativeComrakRange readRange(Map<String, dynamic> json) {
      return NativeComrakRange(
        startByte: readInt(json['startByte']),
        endByte: readInt(json['endByte']),
      );
    }

    final blocks = <NativeComrakBlockSpan>[
      for (final block in mapList(decoded['blocks']))
        NativeComrakBlockSpan(
          type: (block['type'] as String?) ?? '',
          range: readRange(block),
          payload: (block['payload'] is Map<String, dynamic>)
              ? (block['payload'] as Map<String, dynamic>)
              : const <String, Object?>{},
        ),
    ];

    final inlineTokens = <NativeComrakInlineToken>[
      for (final token in mapList(decoded['inlineTokens']))
        NativeComrakInlineToken(
          range: readRange(token),
          styles: (token['styles'] is List)
              ? (token['styles'] as List).whereType<String>().toSet()
              : const <String>{},
        ),
    ];

    final markerRanges = <NativeComrakRange>[
      for (final range in mapList(decoded['markerRanges'])) readRange(range),
    ];

    final exclusionRanges = <NativeComrakRange>[
      for (final range in mapList(decoded['exclusionRanges'])) readRange(range),
    ];

    final diagnostics = <NativeComrakDiagnostic>[
      for (final diagnostic in mapList(decoded['diagnostics']))
        NativeComrakDiagnostic(
          range: readRange(diagnostic),
          message: (diagnostic['message'] as String?) ?? '',
          code: diagnostic['code'] as String?,
          isError: diagnostic['isError'] == true,
        ),
    ];

    return NativeComrakParseResult(
      revision: revision,
      blocks: blocks,
      inlineTokens: inlineTokens,
      markerRanges: markerRanges,
      exclusionRanges: exclusionRanges,
      diagnostics: diagnostics,
    );
  }
}

/// Bridge contract used by the parse backend.
///
/// A later PR will provide the actual dart:ffi + DynamicLibrary bindings.
abstract interface class NativeComrakBridge {
  /// Parses [input] with the native comrak bridge.
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input);
}

/// Failure categories for loading the native comrak bridge.
enum NativeComrakBridgeLoadFailureKind {
  /// The current Dart runtime does not support `dart:ffi`.
  unsupportedFfi,

  /// The current operating system is not supported.
  unsupportedPlatform,

  /// No candidate dynamic library was found.
  libraryNotFound,

  /// A required native symbol could not be resolved.
  symbolLookupFailed,

  /// The native library ABI version does not match this package.
  abiVersionMismatch,

  /// The dynamic library existed but failed to load.
  loadFailed,
}

/// Exception describing why the native comrak bridge could not load.
@immutable
class NativeComrakBridgeLoadException implements Exception {
  /// Stable failure category.
  final NativeComrakBridgeLoadFailureKind kind;

  /// Human-readable failure message.
  final String message;

  /// Operating system reported during loading, if known.
  final String? platform;

  /// Dynamic library name that was attempted, if known.
  final String? libraryName;

  /// User-provided override path, if any.
  final String? overrideLibraryPath;

  /// Candidate paths considered by the loader.
  final List<String> candidatePaths;

  /// Actionable steps a consumer can take to fix the failure.
  final List<String> remediationSteps;

  /// Underlying platform or FFI error, if available.
  final Object? cause;

  /// Creates a native bridge load exception.
  const NativeComrakBridgeLoadException({
    required this.kind,
    required this.message,
    this.platform,
    this.libraryName,
    this.overrideLibraryPath,
    this.candidatePaths = const [],
    this.remediationSteps = const [],
    this.cause,
  });

  /// Short failure summary suitable for UI or logs.
  String get summary => message;

  @override
  String toString() {
    final parts = <String>[
      'NativeComrakBridgeLoadException: $message',
      if (platform != null) 'platform: $platform',
      if (libraryName != null) 'library: $libraryName',
      if (overrideLibraryPath != null && overrideLibraryPath!.isNotEmpty)
        'overridePath: $overrideLibraryPath',
      if (candidatePaths.isNotEmpty) ...[
        'candidates:',
        for (final path in candidatePaths) '  - $path',
      ],
      if (remediationSteps.isNotEmpty) ...[
        'remediation:',
        for (final step in remediationSteps) '  - $step',
      ],
      if (cause != null) 'cause: $cause',
    ];
    return parts.join('\n');
  }
}

/// Result of probing native bridge availability without throwing.
@immutable
class NativeComrakBridgePreflightResult {
  /// Whether the native bridge can be loaded.
  final bool isAvailable;

  /// Load error when [isAvailable] is false.
  final NativeComrakBridgeLoadException? error;

  const NativeComrakBridgePreflightResult._({
    required this.isAvailable,
    this.error,
  });

  /// Creates a successful preflight result.
  const NativeComrakBridgePreflightResult.available()
      : this._(isAvailable: true);

  /// Creates a failed preflight result with [error].
  const NativeComrakBridgePreflightResult.unavailable(
    NativeComrakBridgeLoadException error,
  ) : this._(isAvailable: false, error: error);
}
