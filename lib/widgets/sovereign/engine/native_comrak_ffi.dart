import 'dart:convert';

import 'package:flutter/foundation.dart';

/// Native parse profile for comrak bridge calls.
enum NativeComrakProfile { commonMarkCore, commonMarkGfm }

@immutable
class NativeComrakParseInput {
  final int revision;
  final NativeComrakProfile profile;
  final Uint8List utf8Text;

  const NativeComrakParseInput({
    required this.revision,
    required this.profile,
    required this.utf8Text,
  }) : assert(revision >= 0);
}

@immutable
class NativeComrakRange {
  final int startByte;
  final int endByte;

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

@immutable
class NativeComrakBlockSpan {
  final String type;
  final NativeComrakRange range;
  final Map<String, Object?> payload;

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

@immutable
class NativeComrakInlineToken {
  final NativeComrakRange range;
  final Set<String> styles;

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

@immutable
class NativeComrakDiagnostic {
  final NativeComrakRange range;
  final String message;
  final String? code;
  final bool isError;

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

@immutable
class NativeComrakParseResult {
  final int revision;
  final List<NativeComrakBlockSpan> blocks;
  final List<NativeComrakInlineToken> inlineTokens;
  final List<NativeComrakRange> markerRanges;
  final List<NativeComrakRange> exclusionRanges;
  final List<NativeComrakDiagnostic> diagnostics;

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
  const NativeComrakPayloadCodec._();

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
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input);
}

enum NativeComrakBridgeLoadFailureKind {
  unsupportedFfi,
  unsupportedPlatform,
  libraryNotFound,
  symbolLookupFailed,
  abiVersionMismatch,
  loadFailed,
}

@immutable
class NativeComrakBridgeLoadException implements Exception {
  final NativeComrakBridgeLoadFailureKind kind;
  final String message;
  final String? platform;
  final String? libraryName;
  final String? overrideLibraryPath;
  final List<String> candidatePaths;
  final List<String> remediationSteps;
  final Object? cause;

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

@immutable
class NativeComrakBridgePreflightResult {
  final bool isAvailable;
  final NativeComrakBridgeLoadException? error;

  const NativeComrakBridgePreflightResult._({
    required this.isAvailable,
    this.error,
  });

  const NativeComrakBridgePreflightResult.available()
      : this._(isAvailable: true);

  const NativeComrakBridgePreflightResult.unavailable(
    NativeComrakBridgeLoadException error,
  ) : this._(isAvailable: false, error: error);
}
