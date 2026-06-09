import 'dart:convert';
import 'dart:typed_data';

import '../../core/document/flark_utf8_utf16_mapper.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../native/native_comrak_bridge_factory.dart';
import '../../native/native_comrak_ffi.dart';
import '../source/flark_markdown_fenced_code_scanner.dart';
import 'flark_markdown_parse_backend.dart';
import 'flark_markdown_parse_protocol.dart';
import 'flark_markdown_parse_result.dart';
import 'flark_markdown_profile.dart';

final _blockquoteMarkerPattern = RegExp(r'^[ \t]*>[ \t]?');
final _listItemMarkerPattern = RegExp(
  r'^[ ]{0,3}(?:(\d{1,9}[.)])|([-*+]))[ \t]+(?:\[([ xX])\][ \t]+)?',
);

final class FlarkNativeComrakParseBackend implements FlarkMarkdownParseBackend {
  const FlarkNativeComrakParseBackend({required NativeComrakBridge bridge})
    : _bridge = bridge;

  static FlarkNativeComrakParseBackend? tryLoad({String? overrideLibraryPath}) {
    final preflight = preflightNativeComrakBridge(
      overrideLibraryPath: overrideLibraryPath,
    );
    if (!preflight.isAvailable) return null;
    return FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: overrideLibraryPath,
    );
  }

  static NativeComrakBridgePreflightResult preflight({
    String? overrideLibraryPath,
  }) {
    return preflightNativeComrakBridge(
      overrideLibraryPath: overrideLibraryPath,
    );
  }

  factory FlarkNativeComrakParseBackend.withNativeBridge({
    String? overrideLibraryPath,
  }) {
    return FlarkNativeComrakParseBackend(
      bridge: createNativeComrakBridge(
        overrideLibraryPath: overrideLibraryPath,
      ),
    );
  }

  static FlarkNativeComrakParseBackend requiredDefault() {
    return _requiredDefaultBackend ??=
        FlarkNativeComrakParseBackend.withNativeBridge();
  }

  final NativeComrakBridge _bridge;

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'comrak_native_v2_adapter',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [
          FlarkMarkdownProfile.commonMarkCore,
          FlarkMarkdownProfile.commonMarkGfm,
        ],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    return (await parseWithProfile(request)).result;
  }

  /// Parses [request] and returns phase timings for large-document diagnosis.
  Future<FlarkNativeComrakProfiledParseResult> parseWithProfile(
    FlarkMarkdownParseRequest request,
  ) async {
    final totalStopwatch = Stopwatch()..start();
    if (request.markdown.isEmpty) {
      final result = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: request.revision,
        sourceTextLength: 0,
        blocks: const [],
        inlineTokens: const [],
      );
      totalStopwatch.stop();
      return FlarkNativeComrakProfiledParseResult(
        result: result,
        profile: FlarkNativeComrakParseProfile(
          total: totalStopwatch.elapsed,
          utf8Encode: Duration.zero,
          bridgeTotal: Duration.zero,
          bridgeInputCopy: Duration.zero,
          nativeParse: Duration.zero,
          payloadCopy: Duration.zero,
          payloadDecode: Duration.zero,
          resultMapping: Duration.zero,
          inputBytes: 0,
          payloadBytes: 0,
          nativeBlockCount: 0,
          nativeInlineTokenCount: 0,
          nativeMarkerRangeCount: 0,
          hasBridgeProfile: _bridge is ProfiledNativeComrakBridge,
        ),
      );
    }

    final encodeStopwatch = Stopwatch()..start();
    final utf8Text = Uint8List.fromList(utf8.encode(request.markdown));
    encodeStopwatch.stop();

    final input = NativeComrakParseInput(
      revision: request.revision,
      profile: _nativeProfile(request.profile),
      utf8Text: utf8Text,
    );
    final bridge = _bridge;
    final bridgeStopwatch = Stopwatch()..start();
    final NativeComrakParseResult native;
    final NativeComrakBridgeParseProfile? bridgeProfile;
    if (bridge is ProfiledNativeComrakBridge) {
      final profiled = await bridge.parseWithProfile(input);
      native = profiled.result;
      bridgeProfile = profiled.profile;
    } else {
      native = await bridge.parse(input);
      bridgeProfile = null;
    }
    bridgeStopwatch.stop();

    final mappingStopwatch = Stopwatch()..start();
    final result = _mapNativeResult(request, native);
    mappingStopwatch.stop();
    totalStopwatch.stop();

    return FlarkNativeComrakProfiledParseResult(
      result: result,
      profile: FlarkNativeComrakParseProfile(
        total: totalStopwatch.elapsed,
        utf8Encode: encodeStopwatch.elapsed,
        bridgeTotal: bridgeProfile?.total ?? bridgeStopwatch.elapsed,
        bridgeInputCopy: bridgeProfile?.inputCopy ?? Duration.zero,
        nativeParse: bridgeProfile?.nativeParse ?? bridgeStopwatch.elapsed,
        payloadCopy: bridgeProfile?.payloadCopy ?? Duration.zero,
        payloadDecode: bridgeProfile?.payloadDecode ?? Duration.zero,
        resultMapping: mappingStopwatch.elapsed,
        inputBytes: utf8Text.length,
        payloadBytes: bridgeProfile?.payloadBytes ?? 0,
        nativeBlockCount: native.blocks.length,
        nativeInlineTokenCount: native.inlineTokens.length,
        nativeMarkerRangeCount: native.markerRanges.length,
        hasBridgeProfile: bridgeProfile != null,
      ),
    );
  }
}

FlarkNativeComrakParseBackend? _requiredDefaultBackend;

/// Native Comrak parse result paired with end-to-end phase timings.
final class FlarkNativeComrakProfiledParseResult {
  /// Creates a profiled Flark native parse result.
  const FlarkNativeComrakProfiledParseResult({
    required this.result,
    required this.profile,
  });

  /// Mapped Flark parse result.
  final FlarkMarkdownParseResult result;

  /// End-to-end phase timings for [result].
  final FlarkNativeComrakParseProfile profile;
}

/// End-to-end phase timings for Flark's native Comrak parse pipeline.
final class FlarkNativeComrakParseProfile {
  /// Creates parse pipeline phase timings.
  const FlarkNativeComrakParseProfile({
    required this.total,
    required this.utf8Encode,
    required this.bridgeTotal,
    required this.bridgeInputCopy,
    required this.nativeParse,
    required this.payloadCopy,
    required this.payloadDecode,
    required this.resultMapping,
    required this.inputBytes,
    required this.payloadBytes,
    required this.nativeBlockCount,
    required this.nativeInlineTokenCount,
    required this.nativeMarkerRangeCount,
    required this.hasBridgeProfile,
  });

  /// Total backend parse time.
  final Duration total;

  /// Dart UTF-8 encoding of source markdown.
  final Duration utf8Encode;

  /// Bridge call total. With a profiled FFI bridge, this excludes Dart mapping.
  final Duration bridgeTotal;

  /// Copy from Dart UTF-8 bytes into native input memory.
  final Duration bridgeInputCopy;

  /// Native Comrak parse plus native payload construction.
  final Duration nativeParse;

  /// Copy from native response memory into Dart.
  final Duration payloadCopy;

  /// Dart decode of the native bridge payload.
  final Duration payloadDecode;

  /// Dart mapping from decoded native result into Flark parse result.
  final Duration resultMapping;

  /// UTF-8 source byte count.
  final int inputBytes;

  /// Native response payload byte count.
  final int payloadBytes;

  /// Native block count before Flark mapping.
  final int nativeBlockCount;

  /// Native inline token count before Flark mapping.
  final int nativeInlineTokenCount;

  /// Native marker range count before Flark mapping.
  final int nativeMarkerRangeCount;

  /// Whether bridge-local phases are available.
  final bool hasBridgeProfile;
}

FlarkMarkdownParseResult _mapNativeResult(
  FlarkMarkdownParseRequest request,
  NativeComrakParseResult native,
) {
  final mapper = FlarkUtf8Utf16Mapper(request.markdown);
  final fenceLayout = FlarkMarkdownFenceLayout.scan(request.markdown);
  final nativeRenderBlocks = _renderableNativeBlocks(native.blocks);
  final syntheticCodeBlocks = _syntheticCodeBlocks(
    request.markdown,
    mapper,
    nativeRenderBlocks,
    fenceLayout,
  );
  final renderBlocks = _normalizeNativeCodeBlockRanges(
    request.markdown,
    mapper,
    [...nativeRenderBlocks, ...syntheticCodeBlocks],
  );
  final mappedMarkerRanges = [
    for (final range in native.markerRanges) _mapRange(mapper, range),
    ..._syntheticCodeFenceMarkerRanges(
      request.markdown,
      mapper,
      syntheticCodeBlocks,
      fenceLayout,
    ),
  ]..sort(_compareSourceRanges);
  final markerExtensionIndex = _MarkerExtensionIndex(mappedMarkerRanges);
  final syntheticListPlan = _syntheticListItemPlan(
    request.markdown,
    mapper,
    renderBlocks,
  );
  final syntheticListItems = syntheticListPlan.items;
  final syntheticListMarkerRanges = [
    for (final item in syntheticListItems) item.markerRange,
  ];
  final mappedBlockRanges = [
    for (final block in renderBlocks) _mapRange(mapper, block.range),
  ];
  final mappedBlockRangeIndex = _FlarkSourceRangeIndex(mappedBlockRanges);
  final referenceDefinitionRanges = _referenceDefinitionRanges(
    request.markdown,
  ).where((range) => !mappedBlockRangeIndex.overlaps(range)).toList();
  final rawHtmlRanges = [
    for (final block in native.blocks)
      if (_blockType(block.type) == 'htmlBlock') _mapRange(mapper, block.range),
    for (final token in native.inlineTokens)
      if (token.styles.any((style) => _inlineType(style) == 'htmlInline'))
        _mapRange(mapper, token.range),
  ];
  final nativeInlineHiddenRanges = _nativeInlineHiddenRanges(
    request.markdown,
    mapper,
    native.inlineTokens,
  );
  final partialStrongIntentInlineTokens = [
    for (final token in native.inlineTokens)
      if (_isPartialStrongIntentEmphasis(request.markdown, mapper, token))
        token,
  ];
  final partialStrongIntentMarkerRanges = [
    for (final token in partialStrongIntentInlineTokens)
      ..._partialStrongIntentMarkerRanges(
        request.markdown,
        _mapRange(mapper, token.range),
      ),
  ];
  final markerOnlyBlockquoteRanges = [
    for (final block in renderBlocks)
      if (_isMarkerOnlyNativeBlockquote(
        request.markdown,
        mapper,
        block,
        markerExtensionIndex,
      ))
        _mapRange(mapper, block.range),
  ];
  final markerOnlyHeadingRanges = [
    for (final block in renderBlocks)
      if (_isMarkerOnlyNativeHeading(
        request.markdown,
        mapper,
        block,
        markerExtensionIndex,
      ))
        _mapRange(mapper, block.range),
  ];
  final markerOnlyListItemRanges = [
    for (final block in renderBlocks)
      if (_isMarkerOnlyNativeListItem(
        request.markdown,
        mapper,
        block,
        markerExtensionIndex,
      ))
        _mapRange(mapper, block.range),
  ];
  final markerOnlyBlockRangeKeys = {
    for (final range in [
      ...markerOnlyBlockquoteRanges,
      ...markerOnlyHeadingRanges,
      ...markerOnlyListItemRanges,
    ])
      _rangeKey(range),
  };
  final codeFenceOpeningLineBreakRanges = _codeFenceOpeningLineBreakRanges(
    request.markdown,
    mapper,
    renderBlocks,
    mappedMarkerRanges,
  );
  final codeFenceOpeningMarkerRanges = _codeFenceOpeningMarkerRanges(
    request.markdown,
    mapper,
    renderBlocks,
  );
  final codeFenceOpeningInfoRanges = _codeFenceOpeningInfoRanges(
    request.markdown,
    mapper,
    renderBlocks,
  );
  final codeFenceClosingLineRanges = _codeFenceClosingLineRanges(
    request.markdown,
    mapper,
    renderBlocks,
    mappedMarkerRanges,
  );
  final nativeMarkdownMarkerRanges =
      _nativeMarkdownMarkerHiddenRanges(mappedMarkerRanges, [
        ...referenceDefinitionRanges,
        ...rawHtmlRanges,
        for (final range in nativeInlineHiddenRanges) range.sourceRange,
        ...markerOnlyBlockquoteRanges,
        ...markerOnlyHeadingRanges,
        ...markerOnlyListItemRanges,
        ...syntheticListMarkerRanges,
        ...codeFenceOpeningMarkerRanges,
        ...codeFenceOpeningInfoRanges,
        ...codeFenceClosingLineRanges,
        ...partialStrongIntentMarkerRanges,
      ]);
  final hiddenRanges = [
    for (final range in referenceDefinitionRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.referenceDefinition,
        type: 'referenceDefinition',
        sourceRange: range,
      ),
    for (final range in rawHtmlRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.rawHtml,
        type: 'rawHtml',
        sourceRange: range,
      ),
    ...nativeInlineHiddenRanges,
    ...nativeMarkdownMarkerRanges,
    for (final range in codeFenceOpeningMarkerRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: range,
      ),
    for (final range in codeFenceOpeningInfoRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: range,
      ),
    for (final range in codeFenceOpeningLineBreakRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: range,
      ),
    for (final range in codeFenceClosingLineRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: range,
      ),
    for (final range in syntheticListMarkerRanges)
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: range,
      ),
  ];
  final hiddenSourceRanges = [
    for (final hiddenRange in hiddenRanges) hiddenRange.sourceRange,
  ];
  final hiddenSourceRangeIndex = _FlarkSourceRangeIndex(hiddenSourceRanges);
  final replacementRanges =
      [
            for (final range in native.replacementRanges)
              if (range.text.isNotEmpty) _mapReplacementRange(mapper, range),
          ]
          .where((range) => !hiddenSourceRangeIndex.overlaps(range.sourceRange))
          .toList(growable: false);
  final blocks =
      [
        for (final block in renderBlocks)
          if (!syntheticListPlan.replacedBlocks.contains(block))
            _mapBlock(
              mapper,
              block,
              children: _tableChildBlocks(mapper, block, native.blocks),
              overrideType:
                  markerOnlyBlockRangeKeys.contains(
                    _rangeKey(_mapRange(mapper, block.range)),
                  )
                  ? 'paragraph'
                  : null,
            ),
        for (final item in syntheticListItems) item.toBlock(),
      ]..sort((a, b) {
        final startCompare = a.sourceRange.start.compareTo(b.sourceRange.start);
        if (startCompare != 0) return startCompare;
        return a.sourceRange.length.compareTo(b.sourceRange.length);
      });
  final diagnostics = <FlarkMarkdownDiagnostic>[
    if (native.revision != request.revision)
      FlarkMarkdownDiagnostic(
        code: 'COMRAK_REVISION_MISMATCH',
        message: 'Native parse revision mismatch.',
        sourceRange: const FlarkSourceRange(0, 0),
      ),
  ];

  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: request.revision,
    sourceTextLength: request.markdown.length,
    blocks: blocks,
    inlineTokens: [
      for (final token in native.inlineTokens)
        if (!partialStrongIntentInlineTokens.contains(token))
          ..._mapInlineToken(request.markdown, mapper, token),
    ],
    hiddenRanges: hiddenRanges,
    replacementRanges: replacementRanges,
    diagnostics: [
      ...diagnostics,
      for (final diagnostic in native.diagnostics)
        FlarkMarkdownDiagnostic(
          code: diagnostic.code ?? 'COMRAK_DIAGNOSTIC',
          message: diagnostic.message,
          sourceRange: _mapRange(mapper, diagnostic.range),
          extensions: {'isError': diagnostic.isError},
        ),
    ],
    extensions: {
      'nativeParser': 'comrak',
      'nativeRevision': native.revision,
      'nativeExclusionRanges': [
        for (final range in native.exclusionRanges)
          _rangeJson(_mapRange(mapper, range)),
      ],
    },
  );
}

List<NativeComrakBlockSpan> _renderableNativeBlocks(
  List<NativeComrakBlockSpan> blocks,
) {
  // Index the container blocks once so per-block renderability is O(log n)
  // instead of scanning every block (which was O(blocks^2)).
  final containerIndex = _NativeRangeIndex([
    for (final block in blocks)
      if (_blockType(block.type) == 'listItem' ||
          _blockType(block.type) == 'blockquote' ||
          _blockType(block.type) == 'table')
        block.range,
  ]);
  final checkedListItemIndex = _NativeRangeIndex([
    for (final block in blocks)
      if (_blockType(block.type) == 'listItem' &&
          block.payload['checked'] is bool)
        block.range,
  ]);
  return [
    for (final block in blocks)
      if (_isRenderableNativeBlock(block, containerIndex, checkedListItemIndex))
        block,
  ];
}

List<NativeComrakBlockSpan> _syntheticCodeBlocks(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
  FlarkMarkdownFenceLayout fenceLayout,
) {
  if (markdown.isEmpty) return const [];
  final nativeCodeRanges = [
    for (final block in renderBlocks)
      if (_blockType(block.type) == 'codeBlock') _mapRange(mapper, block.range),
  ];
  final nativeCodeRangeIndex = _FlarkSourceRangeIndex(nativeCodeRanges);
  final blocks = <NativeComrakBlockSpan>[];

  // The shared fence layout is the fence model of record; manufacturing
  // synthetic code blocks from anything else risks disagreeing with the
  // policy layer about where a fence opens or closes.
  for (final context in fenceLayout.contexts) {
    final sourceRange = FlarkSourceRange(
      context.openingLineStart,
      context.closingLineEnd ?? context.bodyEnd(markdown),
    ).validate(markdown.length);
    if (nativeCodeRangeIndex.overlaps(sourceRange)) continue;
    blocks.add(
      NativeComrakBlockSpan(
        type: 'fenced_code',
        range: NativeComrakRange(
          startByte: mapper.utf8OffsetForUtf16Offset(sourceRange.start),
          endByte: mapper.utf8OffsetForUtf16Offset(sourceRange.end),
        ),
        payload: context.language == null
            ? const <String, Object?>{}
            : <String, Object?>{'language': context.language},
      ),
    );
  }

  return blocks;
}

List<FlarkSourceRange> _syntheticCodeFenceMarkerRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> syntheticCodeBlocks,
  FlarkMarkdownFenceLayout fenceLayout,
) {
  final ranges = <FlarkSourceRange>[];
  for (final block in syntheticCodeBlocks) {
    final range = _mapRange(mapper, block.range);
    if (range.start < 0 || range.start >= markdown.length) continue;
    final context = fenceLayout.openerAt(range.start);
    if (context == null) continue;

    final openingMarkerStart =
        context.openingLineStart + context.openingIndent.length;
    ranges.add(
      FlarkSourceRange(
        openingMarkerStart,
        openingMarkerStart + context.markerLength,
      ).validate(markdown.length),
    );

    final closingLineStart = context.closingLineStart;
    final closingLineEnd = context.closingLineEnd;
    if (closingLineStart != null && closingLineEnd != null) {
      var markerStart = closingLineStart;
      while (markerStart < closingLineEnd) {
        final codeUnit = markdown.codeUnitAt(markerStart);
        if (codeUnit != 0x20 && codeUnit != 0x09) break;
        markerStart++;
      }
      if (markerStart < closingLineEnd) {
        ranges.add(
          FlarkSourceRange(
            markerStart,
            closingLineEnd,
          ).validate(markdown.length),
        );
      }
    }
  }
  return ranges;
}

bool _isRenderableNativeBlock(
  NativeComrakBlockSpan block,
  _NativeRangeIndex containerIndex,
  _NativeRangeIndex checkedListItemIndex,
) {
  final type = _blockType(block.type);
  if (type == 'list' || type == 'tableRow' || type == 'tableCell') {
    return false;
  }

  if (type == 'paragraph') {
    // Hidden when a list item, blockquote, or table contains the paragraph.
    return !containerIndex.containsRange(
      block.range.startByte,
      block.range.endByte,
    );
  }

  if (type == 'listItem' && block.payload['checked'] is! bool) {
    // Hidden when a checked task-list item overlaps this plain list item.
    // The block itself is unchecked, so it is absent from the checked index.
    return !checkedListItemIndex.overlapsRange(
      block.range.startByte,
      block.range.endByte,
    );
  }

  return true;
}

bool _nativeRangeContains(NativeComrakRange outer, NativeComrakRange inner) {
  return inner.startByte >= outer.startByte && inner.endByte <= outer.endByte;
}

final class _SyntheticListItem {
  const _SyntheticListItem({
    required this.sourceRange,
    required this.markerRange,
    required this.attributes,
  });

  final FlarkSourceRange sourceRange;
  final FlarkSourceRange markerRange;
  final Map<String, Object?> attributes;

  FlarkMarkdownBlockNode toBlock() {
    return FlarkMarkdownBlockNode(
      kind: FlarkMarkdownBlockKind.listItem,
      type: 'listItem',
      sourceRange: sourceRange,
      attributes: attributes,
    );
  }
}

List<_SyntheticListItem> _syntheticListItems(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
) {
  final nativeListItemRanges = [
    for (final block in renderBlocks)
      if (_blockType(block.type) == 'listItem') _mapRange(mapper, block.range),
  ];
  final nativeListItemRangeIndex = _FlarkSourceRangeIndex(nativeListItemRanges);
  final items = <_SyntheticListItem>[];
  var lineStart = 0;
  while (lineStart <= markdown.length) {
    final newline = markdown.indexOf('\n', lineStart);
    final lineEnd = newline < 0 ? markdown.length : newline;
    var line = markdown.substring(lineStart, lineEnd);
    if (line.endsWith('\r')) line = line.substring(0, line.length - 1);
    final match = _listItemMarkerPattern.firstMatch(line);
    if (match != null) {
      final sourceRange = FlarkSourceRange(lineStart, lineEnd);
      final hasNativeListItem = nativeListItemRangeIndex.overlaps(sourceRange);
      if (!hasNativeListItem) {
        final orderedMarker = match.group(1);
        final checkedMarker = match.group(3);
        items.add(
          _SyntheticListItem(
            sourceRange: sourceRange,
            markerRange: FlarkSourceRange(lineStart, lineStart + match.end),
            attributes: {
              'listKind': orderedMarker == null ? 'unordered' : 'ordered',
              if (checkedMarker != null)
                'checked': checkedMarker.toLowerCase() == 'x',
            },
          ),
        );
      }
    }
    if (newline < 0) break;
    lineStart = newline + 1;
  }
  return items;
}

final class _SyntheticListItemPlan {
  const _SyntheticListItemPlan({
    required this.items,
    required this.replacedBlocks,
  });

  static const empty = _SyntheticListItemPlan(
    items: <_SyntheticListItem>[],
    replacedBlocks: <NativeComrakBlockSpan>{},
  );

  /// Synthetic items to render (and whose markers become hidden ranges).
  final List<_SyntheticListItem> items;

  /// Native blocks fully represented by [items], excluded from the result.
  final Set<NativeComrakBlockSpan> replacedBlocks;
}

/// Decides which synthetic list items render and which native blocks they
/// replace.
///
/// A native paragraph or thematic break is replaced only when *every*
/// non-blank line in its range is a synthetic-item line — Comrak parses
/// in-progress markers like `- ` or `- [` as paragraph text, and those whole
/// paragraphs should render as list items. A partially matching block (a
/// soft-wrapped paragraph whose last line merely starts with `- `) is kept
/// intact, and the candidate items inside it are dropped so the same line is
/// not rendered twice.
_SyntheticListItemPlan _syntheticListItemPlan(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
) {
  final candidates = _syntheticListItems(markdown, mapper, renderBlocks);
  if (candidates.isEmpty) return _SyntheticListItemPlan.empty;

  final itemLineStarts = {
    for (final item in candidates) item.sourceRange.start,
  };
  final replacedBlocks = Set<NativeComrakBlockSpan>.identity();
  final droppedItems = Set<_SyntheticListItem>.identity();

  for (final block in renderBlocks) {
    final type = _blockType(block.type);
    if (type != 'paragraph' && type != 'thematicBreak') continue;
    final range = _mapRange(mapper, block.range);
    final intersecting = [
      for (final item in candidates)
        if (item.sourceRange.intersects(range)) item,
    ];
    if (intersecting.isEmpty) continue;
    if (_everyLineIsSyntheticItem(markdown, range, itemLineStarts)) {
      replacedBlocks.add(block);
    } else {
      droppedItems.addAll(intersecting);
    }
  }

  return _SyntheticListItemPlan(
    items: [
      for (final item in candidates)
        if (!droppedItems.contains(item)) item,
    ],
    replacedBlocks: replacedBlocks,
  );
}

bool _everyLineIsSyntheticItem(
  String markdown,
  FlarkSourceRange range,
  Set<int> itemLineStarts,
) {
  if (range.start >= range.end) return false;
  var lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    markdown,
    range.start,
  );
  while (lineStart < range.end) {
    final newline = markdown.indexOf('\n', lineStart);
    final lineEnd = (newline < 0 ? markdown.length : newline).clamp(
      lineStart,
      range.end,
    );
    final lineText = markdown.substring(lineStart, lineEnd);
    if (lineText.trim().isNotEmpty && !itemLineStarts.contains(lineStart)) {
      return false;
    }
    if (newline < 0 || newline + 1 >= range.end) break;
    lineStart = newline + 1;
  }
  return true;
}

bool _isMarkerOnlyNativeBlockquote(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block,
  _MarkerExtensionIndex markerIndex,
) {
  if (_blockType(block.type) != 'blockquote') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (markerIndex.extendsBlock(markdown, sourceRange)) {
    return false;
  }
  return _isMarkerOnlyBlockquoteSource(markdown, sourceRange);
}

bool _isMarkerOnlyBlockquoteSource(String markdown, FlarkSourceRange range) {
  if (range.start < 0 ||
      range.end > markdown.length ||
      range.start >= range.end) {
    return false;
  }
  final fragment = markdown.substring(range.start, range.end);
  final lines = fragment.replaceAll(RegExp(r'[\r\n]+$'), '').split('\n').map((
    line,
  ) {
    return line.endsWith('\r') ? line.substring(0, line.length - 1) : line;
  });
  return lines.isNotEmpty && lines.every(_isMarkerOnlyBlockquoteLine);
}

bool _isMarkerOnlyBlockquoteLine(String text) {
  final match = _blockquoteMarkerPattern.firstMatch(text);
  if (match == null) return false;
  return text.trimLeft() == '>';
}

bool _isMarkerOnlyNativeHeading(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block,
  _MarkerExtensionIndex markerIndex,
) {
  if (_blockType(block.type) != 'heading') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (markerIndex.extendsBlock(markdown, sourceRange)) {
    return false;
  }
  return _isMarkerOnlyHeadingSource(markdown, sourceRange);
}

bool _isMarkerOnlyHeadingSource(String markdown, FlarkSourceRange range) {
  if (range.start < 0 ||
      range.end > markdown.length ||
      range.start >= range.end) {
    return false;
  }
  final fragment = markdown.substring(range.start, range.end);
  final lines = fragment.replaceAll(RegExp(r'[\r\n]+$'), '').split('\n').map((
    line,
  ) {
    return line.endsWith('\r') ? line.substring(0, line.length - 1) : line;
  });
  return lines.isNotEmpty && lines.every(_isMarkerOnlyHeadingLine);
}

bool _isMarkerOnlyHeadingLine(String text) {
  return RegExp(r'^[ \t]{0,3}#{1,6}$').hasMatch(text);
}

bool _isMarkerOnlyNativeListItem(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block,
  _MarkerExtensionIndex markerIndex,
) {
  if (_blockType(block.type) != 'listItem') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (markerIndex.extendsBlock(markdown, sourceRange)) {
    return false;
  }
  return _isMarkerOnlyListItemSource(markdown, sourceRange);
}

bool _isMarkerOnlyListItemSource(String markdown, FlarkSourceRange range) {
  if (range.start < 0 ||
      range.end > markdown.length ||
      range.start >= range.end) {
    return false;
  }
  final fragment = markdown.substring(range.start, range.end);
  final lines = fragment.replaceAll(RegExp(r'[\r\n]+$'), '').split('\n').map((
    line,
  ) {
    return line.endsWith('\r') ? line.substring(0, line.length - 1) : line;
  });
  return lines.isNotEmpty && lines.every(_isMarkerOnlyListItemLine);
}

bool _isMarkerOnlyListItemLine(String text) {
  return RegExp(r'^[ \t]*(?:[-*+]|\d{1,9}[.)])$').hasMatch(text);
}

List<FlarkMarkdownHiddenRange> _nativeMarkdownMarkerHiddenRanges(
  List<FlarkSourceRange> markerRanges,
  List<FlarkSourceRange> excludedRanges,
) {
  final hiddenRanges = <FlarkMarkdownHiddenRange>[];
  final excludedRangeIndex = _FlarkSourceRangeIndex(excludedRanges);
  for (final range in markerRanges) {
    final sourceRange = range;
    if (excludedRangeIndex.overlaps(sourceRange)) continue;
    hiddenRanges.add(
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.markdownMarker,
        type: 'markdownMarker',
        sourceRange: sourceRange,
      ),
    );
  }
  return hiddenRanges;
}

// Lower bound: first index in start-sorted [ranges] with `start >= value`.
int _firstMarkerIndexWithStartAtLeast(
  List<FlarkSourceRange> ranges,
  int value,
) {
  var low = 0;
  var high = ranges.length;
  while (low < high) {
    final mid = (low + high) >> 1;
    if (ranges[mid].start < value) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }
  return low;
}

List<FlarkSourceRange> _codeFenceOpeningLineBreakRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
  List<FlarkSourceRange> markerRanges,
) {
  final hiddenRanges = <FlarkSourceRange>[];

  for (final block in renderBlocks) {
    if (_blockType(block.type) != 'codeBlock') continue;
    final range = _mapRange(mapper, block.range);
    if (range.start < 0 || range.end > markdown.length) continue;

    final newline = markdown.indexOf('\n', range.start);
    if (newline < 0 || newline >= range.end) continue;

    var hasOpeningFenceMarker = false;
    for (
      var i = _firstMarkerIndexWithStartAtLeast(markerRanges, range.start);
      i < markerRanges.length;
      i += 1
    ) {
      final marker = markerRanges[i];
      if (marker.start >= newline) break;
      if (marker.end <= newline) {
        hasOpeningFenceMarker = true;
        break;
      }
    }
    if (!hasOpeningFenceMarker) continue;

    final lineBreakStart =
        newline > range.start && markdown.codeUnitAt(newline - 1) == 0x0D
        ? newline - 1
        : newline;
    hiddenRanges.add(FlarkSourceRange(lineBreakStart, newline + 1));
  }

  return hiddenRanges;
}

List<FlarkSourceRange> _codeFenceOpeningInfoRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
) {
  final hiddenRanges = <FlarkSourceRange>[];

  for (final block in renderBlocks) {
    if (_blockType(block.type) != 'codeBlock') continue;
    final range = _mapRange(mapper, block.range);
    if (range.start < 0 || range.end > markdown.length) continue;

    final newline = markdown.indexOf('\n', range.start);
    if (newline < 0 || newline >= range.end) continue;

    final markerEnd = _openingFenceMarkerEnd(
      markdown.substring(range.start, newline),
    );
    if (markerEnd == null) continue;

    final infoStart = range.start + markerEnd;
    final infoEnd =
        newline > infoStart && markdown.codeUnitAt(newline - 1) == 0x0D
        ? newline - 1
        : newline;
    if (infoEnd <= infoStart) continue;
    hiddenRanges.add(FlarkSourceRange(infoStart, infoEnd));
  }

  return hiddenRanges;
}

List<FlarkSourceRange> _codeFenceOpeningMarkerRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
) {
  final hiddenRanges = <FlarkSourceRange>[];

  for (final block in renderBlocks) {
    if (_blockType(block.type) != 'codeBlock') continue;
    final range = _mapRange(mapper, block.range);
    if (range.start < 0 || range.end > markdown.length) continue;

    final newline = markdown.indexOf('\n', range.start);
    if (newline < 0 || newline >= range.end) continue;

    final markerEnd = _openingFenceMarkerEnd(
      markdown.substring(range.start, newline),
    );
    if (markerEnd == null || markerEnd <= 0) continue;
    hiddenRanges.add(FlarkSourceRange(range.start, range.start + markerEnd));
  }

  return hiddenRanges;
}

List<FlarkSourceRange> _codeFenceClosingLineRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> renderBlocks,
  List<FlarkSourceRange> markerRanges,
) {
  final hiddenRanges = <FlarkSourceRange>[];

  for (final block in renderBlocks) {
    if (_blockType(block.type) != 'codeBlock') continue;
    final range = _mapRange(mapper, block.range);
    if (range.start < 0 || range.end > markdown.length) continue;

    final openingNewline = markdown.indexOf('\n', range.start);
    if (openingNewline < 0 || openingNewline >= range.end) continue;

    FlarkSourceRange? closingMarker;
    for (
      var i = _firstMarkerIndexWithStartAtLeast(
        markerRanges,
        openingNewline + 1,
      );
      i < markerRanges.length;
      i += 1
    ) {
      final marker = markerRanges[i];
      if (marker.start >= range.end) break;
      if (marker.start < range.start || marker.end > range.end) continue;
      if (closingMarker == null || marker.start > closingMarker.start) {
        closingMarker = marker;
      }
    }
    if (closingMarker == null) continue;

    final newline = markdown.lastIndexOf('\n', closingMarker.start);
    if (newline < range.start || newline >= closingMarker.start) continue;

    final lineBreakStart = newline == openingNewline
        ? closingMarker.start
        : newline > range.start && markdown.codeUnitAt(newline - 1) == 0x0D
        ? newline - 1
        : newline;
    hiddenRanges.add(FlarkSourceRange(lineBreakStart, closingMarker.end));
  }

  return hiddenRanges;
}

List<FlarkMarkdownHiddenRange> _nativeInlineHiddenRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakInlineToken> tokens,
) {
  final ranges = <FlarkMarkdownHiddenRange>[];
  for (final token in tokens) {
    final isImage = token.styles.any((style) => _inlineType(style) == 'image');
    final isLink = token.styles.any((style) => _inlineType(style) == 'link');
    if (!isImage && !isLink) continue;

    final sourceRange = _mapRange(mapper, token.range);
    final start = sourceRange.start;
    final end = sourceRange.end;
    if (start < 0 || end > markdown.length || start >= end) continue;

    final markerLength = isImage ? 2 : 1;
    if (start + markerLength >= end) continue;
    if (isImage) {
      if (!markdown.startsWith('![', start)) continue;
    } else if (markdown.codeUnitAt(start) != 0x5B) {
      continue;
    }

    final labelEnd = _findLinkLabelEnd(markdown, start + markerLength, end);
    if (labelEnd == null || labelEnd + 1 >= end) continue;

    final destinationEnd = switch (markdown.codeUnitAt(labelEnd + 1)) {
      0x28 => _findUnescaped(markdown, ')', labelEnd + 2, end),
      0x5B => _findUnescaped(markdown, ']', labelEnd + 2, end),
      _ => null,
    };
    if (destinationEnd == null || destinationEnd + 1 > end) continue;

    ranges.add(
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: FlarkSourceRange(start, start + markerLength),
      ),
    );
    ranges.add(
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.linkDestination,
        type: 'linkDestination',
        sourceRange: FlarkSourceRange(labelEnd, destinationEnd + 1),
      ),
    );
  }
  return ranges;
}

int? _findLinkLabelEnd(String source, int start, int limit) {
  var depth = 1;
  for (var index = start; index < limit; index++) {
    final unit = source.codeUnitAt(index);
    if (_isEscapedAt(source, index)) continue;
    if (unit == 0x5B) {
      depth++;
      continue;
    }
    if (unit != 0x5D) continue;
    depth--;
    if (depth == 0) return index;
  }
  return null;
}

int? _findUnescaped(String source, String needle, int start, int limit) {
  final needleUnit = needle.codeUnitAt(0);
  for (var index = start; index < limit; index++) {
    if (source.codeUnitAt(index) != needleUnit) continue;
    if (!_isEscapedAt(source, index)) return index;
  }
  return null;
}

bool _isEscapedAt(String source, int index) {
  var backslashCount = 0;
  var cursor = index - 1;
  while (cursor >= 0 && source.codeUnitAt(cursor) == 0x5C) {
    backslashCount++;
    cursor--;
  }
  return backslashCount.isOdd;
}

List<NativeComrakBlockSpan> _normalizeNativeCodeBlockRanges(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  List<NativeComrakBlockSpan> blocks,
) {
  return [
    for (final block in blocks)
      _normalizeNativeCodeBlockRange(markdown, mapper, block),
  ];
}

NativeComrakBlockSpan _normalizeNativeCodeBlockRange(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block,
) {
  if (_blockType(block.type) != 'codeBlock') return block;
  final sourceRange = _mapRange(mapper, block.range);
  if (sourceRange.start < 0 ||
      sourceRange.start >= markdown.length ||
      sourceRange.end > markdown.length) {
    return block;
  }

  final openerEnd = markdown.indexOf('\n', sourceRange.start);
  if (openerEnd < 0 || openerEnd >= markdown.length) return block;
  final openingFence = _openingFence(markdown, sourceRange.start, openerEnd);
  if (openingFence == null) return block;
  final closingLineStart = _closingFenceLineStart(
    markdown,
    openerEnd + 1,
    openingFence,
  );
  if (closingLineStart != null) {
    final closingFenceEnd = _closingFenceMarkerEnd(
      markdown,
      closingLineStart,
      openingFence,
    );
    if (closingFenceEnd == null || sourceRange.end == closingFenceEnd) {
      return block;
    }
    return NativeComrakBlockSpan(
      type: block.type,
      range: NativeComrakRange(
        startByte: block.range.startByte,
        endByte: mapper.utf8OffsetForUtf16Offset(closingFenceEnd),
      ),
      payload: block.payload,
    );
  }
  if (sourceRange.end >= markdown.length) return block;

  return NativeComrakBlockSpan(
    type: block.type,
    range: NativeComrakRange(
      startByte: block.range.startByte,
      endByte: mapper.utf8OffsetForUtf16Offset(markdown.length),
    ),
    payload: block.payload,
  );
}

_FenceInfo? _openingFence(String markdown, int start, int lineEnd) {
  final line = markdown.substring(start, lineEnd);
  final match = RegExp(r'^[ \t]{0,3}(`{3,}|~{3,})').firstMatch(line);
  if (match == null) return null;
  final marker = match.group(1)!;
  return _FenceInfo(marker.codeUnitAt(0), marker.length);
}

int? _openingFenceMarkerEnd(String line) {
  var index = 0;
  while (index < line.length && index < 3) {
    final codeUnit = line.codeUnitAt(index);
    if (codeUnit != 0x20 && codeUnit != 0x09) break;
    index++;
  }
  if (index >= line.length) return null;

  final markerUnit = line.codeUnitAt(index);
  if (markerUnit != 0x60 && markerUnit != 0x7E) return null;

  final markerStart = index;
  while (index < line.length && line.codeUnitAt(index) == markerUnit) {
    index++;
  }
  if (index - markerStart < 3) return null;
  return index;
}

int? _closingFenceLineStart(
  String markdown,
  int start,
  _FenceInfo openingFence,
) {
  var lineStart = start;
  while (lineStart < markdown.length) {
    final newline = markdown.indexOf('\n', lineStart);
    if (_closingFenceMarkerEnd(markdown, lineStart, openingFence) != null) {
      return lineStart;
    }
    if (newline < 0) break;
    lineStart = newline + 1;
  }
  return null;
}

int? _closingFenceMarkerEnd(
  String markdown,
  int lineStart,
  _FenceInfo openingFence,
) {
  final newline = markdown.indexOf('\n', lineStart);
  final lineEnd = newline < 0 ? markdown.length : newline;
  final line = markdown.substring(lineStart, lineEnd);
  final normalized = line.endsWith('\r')
      ? line.substring(0, line.length - 1)
      : line;
  final match = RegExp(r'^[ \t]{0,3}(`+|~+)[ \t]*$').firstMatch(normalized);
  if (match == null) return null;
  final marker = match.group(1)!;
  if (marker.codeUnitAt(0) != openingFence.markerUnit ||
      marker.length < openingFence.length) {
    return null;
  }
  var markerStart = 0;
  while (markerStart < normalized.length) {
    final codeUnit = normalized.codeUnitAt(markerStart);
    if (codeUnit != 0x20 && codeUnit != 0x09) break;
    markerStart++;
  }
  return lineStart + markerStart + marker.length;
}

final class _FenceInfo {
  const _FenceInfo(this.markerUnit, this.length);

  final int markerUnit;
  final int length;
}

FlarkMarkdownBlockNode _mapBlock(
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block, {
  Iterable<FlarkMarkdownBlockNode> children = const [],
  String? overrideType,
}) {
  final mapped = overrideType ?? _blockType(block.type);
  return FlarkMarkdownBlockNode(
    kind: _blockKind(mapped),
    type: mapped,
    sourceRange: _mapRange(mapper, block.range),
    attributes: {
      ...block.payload,
      if (block.type == 'unordered_list') 'listKind': 'unordered',
      if (block.type == 'ordered_list') 'listKind': 'ordered',
      if (mapped == 'unknown') 'nativeType': block.type,
    },
    children: children,
  );
}

List<FlarkMarkdownBlockNode> _tableChildBlocks(
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan table,
  List<NativeComrakBlockSpan> blocks,
) {
  if (_blockType(table.type) != 'table') return const [];

  final rows = [
    for (final block in blocks)
      if (_blockType(block.type) == 'tableRow' &&
          _nativeRangeContains(table.range, block.range))
        block,
  ]..sort((a, b) => a.range.startByte.compareTo(b.range.startByte));

  return [
    for (final row in rows)
      _mapBlock(
        mapper,
        row,
        children: [
          for (final cell in blocks)
            if (_blockType(cell.type) == 'tableCell' &&
                _nativeRangeContains(row.range, cell.range))
              _mapBlock(mapper, cell),
        ]..sort((a, b) => a.sourceRange.start.compareTo(b.sourceRange.start)),
      ),
  ];
}

Iterable<FlarkMarkdownInlineToken> _mapInlineToken(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakInlineToken token,
) sync* {
  final range = _mapRange(mapper, token.range);
  for (final style in _orderedStyles(token.styles)) {
    final mapped = _inlineType(style);
    if (mapped == 'link' && _isFootnoteShortcutReference(markdown, range)) {
      continue;
    }
    yield FlarkMarkdownInlineToken(
      kind: _inlineKind(mapped),
      type: mapped,
      sourceRange: range,
      attributes: {
        ...token.payload,
        if (mapped == 'unknown') 'nativeStyle': style,
      },
    );
  }
}

bool _isPartialStrongIntentEmphasis(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakInlineToken token,
) {
  final styles = token.styles.map(_inlineType).toSet();
  if (styles.length != 1 || !styles.contains('emphasis')) return false;
  return _partialStrongIntentMarkerRanges(
    markdown,
    _mapRange(mapper, token.range),
  ).isNotEmpty;
}

List<FlarkSourceRange> _partialStrongIntentMarkerRanges(
  String markdown,
  FlarkSourceRange range,
) {
  if (range.start < 0 || range.end > markdown.length) return const [];
  if (range.end - range.start < 3) return const [];

  final marker = markdown.codeUnitAt(range.start);
  if (marker != 0x2A && marker != 0x5F) return const [];
  if (markdown.codeUnitAt(range.end - 1) != marker) return const [];

  final hasSameMarkerBefore =
      range.start > 0 && markdown.codeUnitAt(range.start - 1) == marker;
  final hasSameMarkerAfter =
      range.end < markdown.length && markdown.codeUnitAt(range.end) == marker;
  if (!hasSameMarkerBefore && !hasSameMarkerAfter) return const [];

  return [
    FlarkSourceRange(range.start, range.start + 1),
    FlarkSourceRange(range.end - 1, range.end),
  ];
}

FlarkMarkdownReplacementRange _mapReplacementRange(
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakReplacementRange replacementRange,
) {
  final type = _replacementType(replacementRange.type);
  return FlarkMarkdownReplacementRange(
    kind: _replacementKind(type),
    type: type,
    sourceRange: _mapRange(mapper, replacementRange.range),
    replacementText: replacementRange.text,
  );
}

NativeComrakProfile _nativeProfile(FlarkMarkdownProfile profile) {
  return switch (profile) {
    FlarkMarkdownProfile.commonMarkCore => NativeComrakProfile.commonMarkCore,
    FlarkMarkdownProfile.commonMarkGfm => NativeComrakProfile.commonMarkGfm,
  };
}

String _blockType(String type) {
  return switch (type) {
    'paragraph' => 'paragraph',
    'header' => 'heading',
    'heading' => 'heading',
    'blockquote' => 'blockquote',
    'unordered_list' || 'ordered_list' || 'list' => 'list',
    'list_item' || 'listItem' => 'listItem',
    'thematic_break' || 'thematicBreak' => 'thematicBreak',
    'fenced_code' || 'codeBlock' => 'codeBlock',
    'html_block' || 'htmlBlock' => 'htmlBlock',
    'table' => 'table',
    'table_row' || 'tableRow' => 'tableRow',
    'table_cell' || 'tableCell' => 'tableCell',
    _ => 'unknown',
  };
}

FlarkMarkdownBlockKind _blockKind(String type) {
  return switch (type) {
    'paragraph' => FlarkMarkdownBlockKind.paragraph,
    'heading' => FlarkMarkdownBlockKind.heading,
    'blockquote' => FlarkMarkdownBlockKind.blockquote,
    'list' => FlarkMarkdownBlockKind.list,
    'listItem' => FlarkMarkdownBlockKind.listItem,
    'thematicBreak' => FlarkMarkdownBlockKind.thematicBreak,
    'codeBlock' => FlarkMarkdownBlockKind.codeBlock,
    'htmlBlock' => FlarkMarkdownBlockKind.htmlBlock,
    'table' => FlarkMarkdownBlockKind.table,
    'tableRow' => FlarkMarkdownBlockKind.tableRow,
    'tableCell' => FlarkMarkdownBlockKind.tableCell,
    _ => FlarkMarkdownBlockKind.unknown,
  };
}

String _inlineType(String style) {
  return switch (style) {
    'bold' || 'strong' => 'strong',
    'italic' || 'emphasis' => 'emphasis',
    'code' || 'inlineCode' => 'inlineCode',
    'link' => 'link',
    'image' => 'image',
    'autolink' => 'autolink',
    'strikethrough' => 'strikethrough',
    'htmlInline' || 'html_inline' => 'htmlInline',
    _ => 'unknown',
  };
}

FlarkMarkdownInlineKind _inlineKind(String type) {
  return switch (type) {
    'emphasis' => FlarkMarkdownInlineKind.emphasis,
    'strong' => FlarkMarkdownInlineKind.strong,
    'inlineCode' => FlarkMarkdownInlineKind.inlineCode,
    'link' => FlarkMarkdownInlineKind.link,
    'image' => FlarkMarkdownInlineKind.image,
    'autolink' => FlarkMarkdownInlineKind.autolink,
    'strikethrough' => FlarkMarkdownInlineKind.strikethrough,
    'htmlInline' => FlarkMarkdownInlineKind.htmlInline,
    _ => FlarkMarkdownInlineKind.unknown,
  };
}

String _replacementType(String type) {
  return switch (type) {
    'htmlEntity' || 'html_entity' => 'htmlEntity',
    _ => type.isEmpty ? 'unknown' : type,
  };
}

FlarkMarkdownReplacementRangeKind _replacementKind(String type) {
  return switch (type) {
    'htmlEntity' => FlarkMarkdownReplacementRangeKind.htmlEntity,
    _ => FlarkMarkdownReplacementRangeKind.unknown,
  };
}

List<String> _orderedStyles(Set<String> styles) {
  const priority = [
    'bold',
    'strong',
    'italic',
    'emphasis',
    'code',
    'inlineCode',
    'link',
    'image',
    'autolink',
    'strikethrough',
    'htmlInline',
    'html_inline',
  ];
  return [
    for (final style in priority)
      if (styles.contains(style)) style,
    for (final style in styles)
      if (!priority.contains(style)) style,
  ];
}

FlarkSourceRange _mapRange(
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakRange range,
) {
  final startByte = range.startByte.clamp(0, mapper.utf8Length);
  final endByte = range.endByte.clamp(0, mapper.utf8Length);
  final start = mapper.utf16OffsetForUtf8Offset(startByte);
  final end = mapper.utf16OffsetForUtf8Offset(endByte);
  if (end < start) return FlarkSourceRange(start, start);
  return FlarkSourceRange(start, end);
}

Map<String, int> _rangeJson(FlarkSourceRange range) {
  return {'start': range.start, 'end': range.end};
}

List<FlarkSourceRange> _referenceDefinitionRanges(String markdown) {
  final ranges = <FlarkSourceRange>[];
  var lineStart = 0;
  while (lineStart <= markdown.length) {
    final nextBreak = markdown.indexOf('\n', lineStart);
    final lineEnd = nextBreak == -1 ? markdown.length : nextBreak;
    final lineEndWithBreak = nextBreak == -1 ? lineEnd : nextBreak + 1;
    final line = markdown.substring(lineStart, lineEnd);
    if (_isReferenceDefinitionLine(line)) {
      ranges.add(FlarkSourceRange(lineStart, lineEndWithBreak));
    }
    if (nextBreak == -1) break;
    lineStart = nextBreak + 1;
  }
  return ranges;
}

bool _isReferenceDefinitionLine(String line) {
  if (RegExp(r'^[ \t]{0,3}\[\^[^\]\n]+\]:').hasMatch(line)) {
    return false;
  }
  return RegExp(r'^[ \t]{0,3}\[[^\]\n]+\]:[ \t]*\S').hasMatch(line);
}

bool _isFootnoteShortcutReference(
  String markdown,
  FlarkSourceRange sourceRange,
) {
  if (sourceRange.start < 0 ||
      sourceRange.end > markdown.length ||
      sourceRange.start >= sourceRange.end) {
    return false;
  }
  final source = markdown.substring(sourceRange.start, sourceRange.end);
  return RegExp(r'^\[\^[^\]\n]+\]$').hasMatch(source);
}

int _compareSourceRanges(FlarkSourceRange left, FlarkSourceRange right) {
  final startCompare = left.start.compareTo(right.start);
  if (startCompare != 0) return startCompare;
  return left.end.compareTo(right.end);
}

String _rangeKey(FlarkSourceRange range) {
  return '${range.start}:${range.end}';
}

/// Start-sorted byte-range index with a prefix-max-end array, giving O(log n)
/// containment and overlap queries over native block ranges.
final class _NativeRangeIndex {
  factory _NativeRangeIndex(Iterable<NativeComrakRange> ranges) {
    final sorted = <NativeComrakRange>[...ranges]
      ..sort((a, b) {
        final startCompare = a.startByte.compareTo(b.startByte);
        if (startCompare != 0) return startCompare;
        return a.endByte.compareTo(b.endByte);
      });
    final starts = List<int>.filled(sorted.length, 0);
    final prefixMaxEnd = List<int>.filled(sorted.length, 0);
    var maxEnd = -1;
    for (var i = 0; i < sorted.length; i += 1) {
      starts[i] = sorted[i].startByte;
      maxEnd = maxEnd > sorted[i].endByte ? maxEnd : sorted[i].endByte;
      prefixMaxEnd[i] = maxEnd;
    }
    return _NativeRangeIndex._(starts, prefixMaxEnd);
  }

  _NativeRangeIndex._(this._starts, this._prefixMaxEnd);

  final List<int> _starts;
  final List<int> _prefixMaxEnd;

  /// Whether any indexed range contains `[start, end)`.
  ///
  /// True iff some range has `rangeStart <= start && rangeEnd >= end`. Among
  /// ranges with `rangeStart <= start` (a start-sorted prefix), the prefix-max
  /// end witnesses such a range.
  bool containsRange(int start, int end) {
    final count = _countStartsAtMost(start);
    if (count == 0) return false;
    return _prefixMaxEnd[count - 1] >= end;
  }

  /// Whether any indexed range overlaps `[start, end)`.
  ///
  /// True iff some range has `rangeStart < end && rangeEnd > start`. Among
  /// ranges with `rangeStart < end` (a start-sorted prefix), the prefix-max end
  /// witnesses such a range.
  bool overlapsRange(int start, int end) {
    final count = _countStartsBelow(end);
    if (count == 0) return false;
    return _prefixMaxEnd[count - 1] > start;
  }

  int _countStartsAtMost(int value) {
    var low = 0;
    var high = _starts.length;
    while (low < high) {
      final mid = (low + high) >> 1;
      if (_starts[mid] <= value) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low;
  }

  int _countStartsBelow(int value) {
    var low = 0;
    var high = _starts.length;
    while (low < high) {
      final mid = (low + high) >> 1;
      if (_starts[mid] < value) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low;
  }
}

/// Index over start-sorted marker ranges answering "does any marker start at or
/// before a block and extend past its end with whitespace-only overhang", in
/// O(log n) for the common (no-extension) case instead of scanning all markers
/// before each block.
final class _MarkerExtensionIndex {
  factory _MarkerExtensionIndex(List<FlarkSourceRange> rangesSortedByStart) {
    final length = rangesSortedByStart.length;
    final starts = List<int>.filled(length, 0);
    final ends = List<int>.filled(length, 0);
    final prefixMaxEnd = List<int>.filled(length, 0);
    var maxEnd = -1;
    for (var i = 0; i < length; i += 1) {
      starts[i] = rangesSortedByStart[i].start;
      ends[i] = rangesSortedByStart[i].end;
      maxEnd = maxEnd > ends[i] ? maxEnd : ends[i];
      prefixMaxEnd[i] = maxEnd;
    }
    return _MarkerExtensionIndex._(starts, ends, prefixMaxEnd);
  }

  _MarkerExtensionIndex._(this._starts, this._ends, this._prefixMaxEnd);

  final List<int> _starts;
  final List<int> _ends;
  final List<int> _prefixMaxEnd;

  bool extendsBlock(String markdown, FlarkSourceRange blockRange) {
    var low = 0;
    var high = _starts.length;
    while (low < high) {
      final mid = (low + high) >> 1;
      if (_starts[mid] <= blockRange.start) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    final count = low;
    if (count == 0) return false;
    // Fast reject: no marker starting at/before the block extends past its end.
    if (_prefixMaxEnd[count - 1] <= blockRange.end) return false;
    if (blockRange.end > markdown.length) return false;
    for (var i = count - 1; i >= 0; i -= 1) {
      final end = _ends[i];
      if (end <= blockRange.end || end > markdown.length) continue;
      final extension = markdown.substring(blockRange.end, end);
      if (extension.isNotEmpty && extension.trim().isEmpty) return true;
    }
    return false;
  }
}

final class _FlarkSourceRangeIndex {
  _FlarkSourceRangeIndex(Iterable<FlarkSourceRange> ranges)
    : _ranges = List<FlarkSourceRange>.unmodifiable(
        <FlarkSourceRange>[...ranges]..sort(_compareSourceRanges),
      );

  final List<FlarkSourceRange> _ranges;

  bool overlaps(FlarkSourceRange range) {
    if (_ranges.isEmpty) return false;
    var index = _lastRangeStartingBefore(range.end);
    while (index >= 0) {
      final other = _ranges[index];
      if (other.end <= range.start) return false;
      if (range.start < other.end && other.start < range.end) return true;
      index -= 1;
    }
    return false;
  }

  int _lastRangeStartingBefore(int offset) {
    var low = 0;
    var high = _ranges.length;
    while (low < high) {
      final middle = (low + high) >> 1;
      if (_ranges[middle].start < offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low - 1;
  }
}
