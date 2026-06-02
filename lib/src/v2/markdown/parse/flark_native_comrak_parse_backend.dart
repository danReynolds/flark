import 'dart:convert';
import 'dart:typed_data';

import '../../core/document/flark_utf8_utf16_mapper.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../native/native_comrak_bridge_factory.dart';
import '../../native/native_comrak_ffi.dart';
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
    if (request.markdown.isEmpty) {
      return FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: request.revision,
        sourceTextLength: 0,
        blocks: const [],
        inlineTokens: const [],
      );
    }

    final native = await _bridge.parse(
      NativeComrakParseInput(
        revision: request.revision,
        profile: _nativeProfile(request.profile),
        utf8Text: Uint8List.fromList(utf8.encode(request.markdown)),
      ),
    );
    return _mapNativeResult(request, native);
  }
}

FlarkNativeComrakParseBackend? _requiredDefaultBackend;

FlarkMarkdownParseResult _mapNativeResult(
  FlarkMarkdownParseRequest request,
  NativeComrakParseResult native,
) {
  final mapper = FlarkUtf8Utf16Mapper(request.markdown);
  final renderBlocks = _normalizeNativeCodeBlockRanges(
    request.markdown,
    mapper,
    _renderableNativeBlocks(native.blocks),
  );
  final mappedMarkerRanges = [
    for (final range in native.markerRanges) _mapRange(mapper, range),
  ]..sort(_compareSourceRanges);
  final syntheticListItems = _syntheticListItems(
    request.markdown,
    mapper,
    renderBlocks,
  );
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
        mappedMarkerRanges,
      ))
        _mapRange(mapper, block.range),
  ];
  final markerOnlyHeadingRanges = [
    for (final block in renderBlocks)
      if (_isMarkerOnlyNativeHeading(
        request.markdown,
        mapper,
        block,
        mappedMarkerRanges,
      ))
        _mapRange(mapper, block.range),
  ];
  final markerOnlyListItemRanges = [
    for (final block in renderBlocks)
      if (_isMarkerOnlyNativeListItem(
        request.markdown,
        mapper,
        block,
        mappedMarkerRanges,
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
          if (!_isReplacedBySyntheticListItem(
            block,
            mapper,
            syntheticListItems,
          ))
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
  return [
    for (final block in blocks)
      if (_isRenderableNativeBlock(block, blocks)) block,
  ];
}

bool _isRenderableNativeBlock(
  NativeComrakBlockSpan block,
  List<NativeComrakBlockSpan> blocks,
) {
  final type = _blockType(block.type);
  if (type == 'list' || type == 'tableRow' || type == 'tableCell') {
    return false;
  }

  if (type == 'paragraph') {
    return !blocks.any((candidate) {
      if (identical(candidate, block)) return false;
      final candidateType = _blockType(candidate.type);
      if (candidateType != 'listItem' &&
          candidateType != 'blockquote' &&
          candidateType != 'table') {
        return false;
      }
      return _nativeRangeContains(candidate.range, block.range);
    });
  }

  if (type == 'listItem' && block.payload['checked'] is! bool) {
    return !blocks.any((candidate) {
      if (identical(candidate, block)) return false;
      if (_blockType(candidate.type) != 'listItem') return false;
      if (candidate.payload['checked'] is! bool) return false;
      return _nativeRangesOverlap(candidate.range, block.range);
    });
  }

  return true;
}

bool _nativeRangeContains(NativeComrakRange outer, NativeComrakRange inner) {
  return inner.startByte >= outer.startByte && inner.endByte <= outer.endByte;
}

bool _nativeRangesOverlap(NativeComrakRange a, NativeComrakRange b) {
  return a.startByte < b.endByte && b.startByte < a.endByte;
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

bool _isReplacedBySyntheticListItem(
  NativeComrakBlockSpan block,
  FlarkUtf8Utf16Mapper mapper,
  List<_SyntheticListItem> syntheticItems,
) {
  final type = _blockType(block.type);
  if (type != 'paragraph' && type != 'thematicBreak') return false;
  final range = _mapRange(mapper, block.range);
  return syntheticItems.any((item) => item.sourceRange.intersects(range));
}

bool _isMarkerOnlyNativeBlockquote(
  String markdown,
  FlarkUtf8Utf16Mapper mapper,
  NativeComrakBlockSpan block,
  List<FlarkSourceRange> markerRanges,
) {
  if (_blockType(block.type) != 'blockquote') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (_nativeMarkerExtendsBlockSource(markdown, sourceRange, markerRanges)) {
    return false;
  }
  return _isMarkerOnlyBlockquoteSource(markdown, sourceRange);
}

bool _nativeMarkerExtendsBlockSource(
  String markdown,
  FlarkSourceRange blockRange,
  List<FlarkSourceRange> markerRanges,
) {
  for (final markerRange in markerRanges) {
    final marker = markerRange;
    if (marker.start > blockRange.start) break;
    if (marker.start > blockRange.start || marker.end <= blockRange.end) {
      continue;
    }
    if (marker.end > markdown.length || blockRange.end > markdown.length) {
      continue;
    }
    final extension = markdown.substring(blockRange.end, marker.end);
    if (extension.isNotEmpty && extension.trim().isEmpty) return true;
  }
  return false;
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
  List<FlarkSourceRange> markerRanges,
) {
  if (_blockType(block.type) != 'heading') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (_nativeMarkerExtendsBlockSource(markdown, sourceRange, markerRanges)) {
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
  List<FlarkSourceRange> markerRanges,
) {
  if (_blockType(block.type) != 'listItem') return false;
  final sourceRange = _mapRange(mapper, block.range);
  if (_nativeMarkerExtendsBlockSource(markdown, sourceRange, markerRanges)) {
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
    for (final marker in markerRanges) {
      if (marker.start < range.start) continue;
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
    for (final marker in markerRanges) {
      if (marker.start <= openingNewline) continue;
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
