import 'dart:typed_data';

import '../../host/host.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_block_quote_projection.dart';
import 'flark_v3_bullet_list_projection.dart';
import 'flark_v3_indented_code_projection.dart';
import 'flark_v3_inline_facts.dart';
import 'flark_v3_ordered_list_projection.dart';

/// Direction used when an exact source position lies on a structural edge.
enum FlarkV3DocumentQueryAffinity { upstream, downstream }

/// Hard caller-isolate limits for one structural query.
///
/// The native/Web host may return a typed source gap instead of exceeding any
/// limit. These bounds constrain copied result size and tree work; they do not
/// change Markdown interpretation.
final class FlarkV3DocumentQueryBudget {
  const FlarkV3DocumentQueryBudget({
    this.maximumEncodedBytes = 4096,
    this.maximumOpenDepth = 16,
    this.maximumLeafCount = 64,
    this.maximumTreeNodesVisited = 256,
  }) : assert(maximumEncodedBytes > 0),
       assert(maximumOpenDepth > 0),
       assert(maximumLeafCount > 0),
       assert(maximumTreeNodesVisited > 0);

  final int maximumEncodedBytes;
  final int maximumOpenDepth;
  final int maximumLeafCount;
  final int maximumTreeNodesVisited;
}

FlarkV3RecursiveGreenLogicalAtom _decodeRecursiveGreenLogicalAtom(
  int tag,
  int argument0,
  int argument1, {
  required int physicalUtf8,
  required int physicalUtf16,
  required int logicalUtf8,
  required int logicalUtf16,
}) {
  Never invalid() => throw const FlarkV3DocumentQueryException(
    'The recursive-Green logical atom has invalid geometry.',
  );
  bool metrics(int p8, int p16, int l8, int l16) =>
      physicalUtf8 == p8 &&
      physicalUtf16 == p16 &&
      logicalUtf8 == l8 &&
      logicalUtf16 == l16;
  switch (tag) {
    case 0:
      if (argument0 != 0 ||
          argument1 != 0 ||
          logicalUtf8 != 0 ||
          logicalUtf16 != 0) {
        invalid();
      }
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.none,
      );
    case 1:
      if (argument0 != 0 ||
          argument1 != 0 ||
          physicalUtf8 != logicalUtf8 ||
          physicalUtf16 != logicalUtf16) {
        invalid();
      }
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.identity,
      );
    case 2:
      if (argument1 < 1 ||
          argument1 > 3 ||
          !metrics(1, 1, argument1, argument1)) {
        invalid();
      }
      return FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.tabToSpaces,
        targetOwnerDepth: argument0,
        spaces: argument1,
      );
    case 3:
      if (argument0 != 0 ||
          argument1 != 0 ||
          logicalUtf8 != 0 ||
          logicalUtf16 != 0) {
        invalid();
      }
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.hiddenUpstream,
      );
    case 4:
      if (argument0 != 0 || argument1 != 0 || !metrics(1, 1, 1, 1)) invalid();
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.lfToLf,
      );
    case 5:
      if (argument0 != 0 || argument1 != 0 || !metrics(2, 2, 1, 1)) invalid();
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.crLfToLf,
      );
    case 6:
      if (argument0 != 0 || argument1 != 0 || !metrics(1, 1, 1, 1)) invalid();
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.loneCrToLf,
      );
    case 7:
      if (argument0 != 0 || argument1 != 0 || !metrics(1, 1, 3, 1)) invalid();
      return const FlarkV3RecursiveGreenLogicalAtom._(
        kind: FlarkV3RecursiveGreenLogicalAtomKind.nulToReplacement,
      );
    default:
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green viewport has an unknown logical atom.',
      );
  }
}

/// One exact half-open range in both UTF-8 and UTF-16 coordinates.
final class FlarkV3SourceSpan {
  const FlarkV3SourceSpan({
    required this.startUtf8,
    required this.endUtf8,
    required this.startUtf16,
    required this.endUtf16,
  }) : assert(startUtf8 >= 0),
       assert(endUtf8 >= startUtf8),
       assert(startUtf16 >= 0),
       assert(endUtf16 >= startUtf16);

  final int startUtf8;
  final int endUtf8;
  final int startUtf16;
  final int endUtf16;
}

enum FlarkV3DocumentStructureKind {
  empty,
  paragraph,
  unknown,
  fencedCode,
  heading,
  thematicBreak,
  indentedCode,
  blockQuote,
  bulletList,
  orderedList,
}

enum FlarkV3CodeFenceMarker { backtick, tilde }

/// Parser-certified source geometry for one fenced code block.
///
/// The ranges remain source-backed. Consumers can project [bodySource]
/// without interpreting the opener, info string, or closer in Dart.
final class FlarkV3FencedCodeFacts {
  const FlarkV3FencedCodeFacts({
    required this.marker,
    required this.openingIndent,
    required this.openingMarker,
    required this.rawInfoSource,
    required this.bodySource,
    required this.closingMarker,
  }) : assert(openingIndent >= 0 && openingIndent <= 3);

  final FlarkV3CodeFenceMarker marker;
  final int openingIndent;
  final FlarkV3SourceSpan openingMarker;
  final FlarkV3SourceSpan rawInfoSource;
  final FlarkV3SourceSpan bodySource;
  final FlarkV3SourceSpan? closingMarker;

  bool get closed => closingMarker != null;
}

/// Parser-certified source geometry shared by every supported heading syntax.
///
/// [contentSource] is the marker-free inline projection. Dart consumers do
/// not inspect heading source to recover any of these facts.
sealed class FlarkV3HeadingFacts {
  const FlarkV3HeadingFacts({required this.level, required this.contentSource})
    : assert(level >= 1 && level <= 6);

  final int level;
  final FlarkV3SourceSpan contentSource;
}

/// Exact parser facts specific to an ATX heading.
final class FlarkV3AtxHeadingFacts extends FlarkV3HeadingFacts {
  const FlarkV3AtxHeadingFacts({
    required super.level,
    required super.contentSource,
    required this.openingMarker,
    required this.closingMarker,
  });

  final FlarkV3SourceSpan openingMarker;
  final FlarkV3SourceSpan? closingMarker;

  bool get hasClosingMarker => closingMarker != null;
}

/// Exact parser facts specific to a Setext heading.
final class FlarkV3SetextHeadingFacts extends FlarkV3HeadingFacts {
  const FlarkV3SetextHeadingFacts({
    required super.level,
    required super.contentSource,
    required this.openingIndent,
    required this.contentLineEnding,
    required this.underlineMarker,
    required this.underlineLineEnding,
  }) : assert(level <= 2),
       assert(openingIndent >= 0 && openingIndent <= 3);

  /// Indentation before the underline marker, in source bytes/ASCII columns.
  final int openingIndent;

  /// The structural line ending between heading content and its underline.
  ///
  /// It remains canonical source but is outside [contentSource], so a
  /// marker-free editor does not expose a trailing editable blank line.
  final FlarkV3SourceSpan contentLineEnding;
  final FlarkV3SourceSpan underlineMarker;
  final FlarkV3SourceSpan underlineLineEnding;
}

enum FlarkV3ThematicBreakMarker { asterisk, hyphen, underscore }

/// Exact parser facts for one atomic thematic-break block.
///
/// A thematic break has no source-backed display text. [markerEnvelope]
/// identifies the complete span from its first marker through its final
/// marker, including any interleaved spaces or tabs. Canonical source remains
/// available through [FlarkV3DocumentStructure.source]; consumers render an
/// atomic divider without inspecting Markdown bytes.
final class FlarkV3ThematicBreakFacts {
  const FlarkV3ThematicBreakFacts({
    required this.marker,
    required this.markerCount,
    required this.openingIndent,
    required this.hasBofBom,
    required this.markerEnvelope,
    required this.lineEnding,
  }) : assert(markerCount >= 3),
       assert(openingIndent >= 0 && openingIndent <= 3);

  final FlarkV3ThematicBreakMarker marker;
  final int markerCount;
  final int openingIndent;
  final bool hasBofBom;
  final FlarkV3SourceSpan markerEnvelope;
  final FlarkV3SourceSpan lineEnding;
}

/// Parser-certified summary for one top-level indented code block.
///
/// The code block has no single contiguous marker-free source span: every
/// physical line owns a hidden indentation prefix. [deindentColumns] and the
/// aggregate lengths describe the parser-authored projection recipe, while a
/// separately demanded, revision-bound projection payload supplies the exact
/// per-line hide/copy runs. Dart must not recover those runs by recognizing
/// Markdown indentation itself.
final class FlarkV3IndentedCodeFacts {
  const FlarkV3IndentedCodeFacts({
    required this.deindentColumns,
    required this.hasBofBom,
    required this.lineCount,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
    required this.terminalLineEndingBytes,
  }) : assert(deindentColumns > 0),
       assert(lineCount > 0),
       assert(projectedUtf8Length >= 0),
       assert(projectedUtf16Length >= 0),
       assert(terminalLineEndingBytes >= 0 && terminalLineEndingBytes <= 2);

  /// Columns removed from every admitted physical line.
  final int deindentColumns;

  /// Whether the first hidden prefix also owns the UTF-8 BOF BOM.
  final bool hasBofBom;

  /// Physical lines through the final nonblank code line.
  ///
  /// Leading and trailing blank lines are separate structural leaves.
  final int lineCount;

  /// Exact source-backed display lengths after certified prefixes are hidden.
  ///
  /// These lengths do not include CommonMark's semantic virtual final LF.
  final int projectedUtf8Length;
  final int projectedUtf16Length;

  /// Width of the final physical source line ending: 0, 1, or 2 bytes.
  final int terminalLineEndingBytes;
}

/// Exact parser summary for one supported depth-1 block quote.
///
/// The first block-quote slice contains one Paragraph child whose content is a
/// noncontiguous sequence of physical-line runs. A separately demanded path
/// payload supplies those runs; Dart never locates quote markers itself.
final class FlarkV3BlockQuoteFacts {
  const FlarkV3BlockQuoteFacts({
    required this.lineCount,
    required this.childFirstLine,
    required this.childLineCount,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
  }) : assert(lineCount > 0),
       assert(childFirstLine >= 0),
       assert(childLineCount > 0),
       assert(childFirstLine + childLineCount <= lineCount),
       assert(projectedUtf8Length > 0),
       assert(projectedUtf16Length > 0);

  final int lineCount;
  final int childFirstLine;
  final int childLineCount;
  final int projectedUtf8Length;
  final int projectedUtf16Length;
}

enum FlarkV3BulletListMarker {
  hyphen('-'),
  plus('+'),
  asterisk('*');

  const FlarkV3BulletListMarker(this.sourceCharacter);

  final String sourceCharacter;
}

/// Exact parser summary for one supported top-level tight bullet list.
///
/// The list display is a noncontiguous sequence of item content and physical
/// line endings. A separately demanded projection payload owns the exact
/// hide/copy recipe and selected-item editing inputs; Dart does not recover
/// list syntax from source text.
final class FlarkV3BulletListFacts {
  const FlarkV3BulletListFacts({
    required this.marker,
    required this.itemCount,
    required this.terminalEmptyRelativeStartUtf8,
    required this.paragraphCount,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
  }) : assert(itemCount > 0),
       assert(
         terminalEmptyRelativeStartUtf8 == null
             ? paragraphCount == itemCount
             : paragraphCount + 1 == itemCount,
       ),
       assert(projectedUtf8Length >= 0),
       assert(projectedUtf16Length >= 0),
       assert((projectedUtf8Length == 0) == (projectedUtf16Length == 0));

  final FlarkV3BulletListMarker marker;
  final int itemCount;

  /// UTF-8 start of the final empty item relative to the list, when present.
  final int? terminalEmptyRelativeStartUtf8;

  final int paragraphCount;
  final int projectedUtf8Length;
  final int projectedUtf16Length;

  /// This first exact list slice deliberately admits tight lists only.
  bool get tight => true;
  bool get hasTerminalEmptyItem => terminalEmptyRelativeStartUtf8 != null;
}

enum FlarkV3OrderedListDelimiter {
  period('.'),
  parenthesis(')');

  const FlarkV3OrderedListDelimiter(this.sourceCharacter);

  final String sourceCharacter;
}

/// Exact parser summary for one supported top-level tight ordered list.
///
/// List-level start and delimiter are structural facts. Exact authored marker
/// spelling for the selected item (including zero padding and nonsequential
/// values) belongs to the separately authenticated compact projection.
final class FlarkV3OrderedListFacts {
  const FlarkV3OrderedListFacts({
    required this.start,
    required this.delimiter,
    required this.itemCount,
    required this.terminalEmptyRelativeStartUtf8,
    required this.paragraphCount,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
  }) : assert(start >= 0 && start <= 999999999),
       assert(itemCount > 0),
       assert(
         terminalEmptyRelativeStartUtf8 == null
             ? paragraphCount == itemCount
             : paragraphCount + 1 == itemCount,
       ),
       assert(projectedUtf8Length >= 0),
       assert(projectedUtf16Length >= 0),
       assert((projectedUtf8Length == 0) == (projectedUtf16Length == 0));

  final int start;
  final FlarkV3OrderedListDelimiter delimiter;
  final int itemCount;
  final int? terminalEmptyRelativeStartUtf8;
  final int paragraphCount;
  final int projectedUtf8Length;
  final int projectedUtf16Length;

  bool get tight => true;
  bool get hasTerminalEmptyItem => terminalEmptyRelativeStartUtf8 != null;
}

/// One node in the exact structural ancestry selected by a point query.
///
/// Container kinds are decoded only from parser-authored path records. Their
/// presence never makes Dart recognize Markdown syntax.
enum FlarkV3DocumentPointPathNodeKind { blockQuote, paragraph, list, listItem }

/// One exact source envelope and projection-run slice in a point path.
///
/// [source] is an envelope, not necessarily contiguous visible content. When
/// [isNoncontiguous] is true, consumers must use the parser-authored projection
/// payload rather than treating bytes inside the envelope as one text run.
final class FlarkV3DocumentPointPathNode {
  const FlarkV3DocumentPointPathNode({
    required this.kind,
    required this.source,
    required this.depth,
    required this.parentIndex,
    required this.firstRun,
    required this.runCount,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
    required this.isNoncontiguous,
    required this.isSelected,
  }) : assert(depth >= 0),
       assert(parentIndex == null || parentIndex >= 0),
       assert(firstRun >= 0),
       assert(runCount > 0),
       assert(projectedUtf8Length >= 0),
       assert(projectedUtf16Length >= 0),
       assert((projectedUtf8Length == 0) == (projectedUtf16Length == 0));

  final FlarkV3DocumentPointPathNodeKind kind;
  final FlarkV3SourceSpan source;
  final int depth;
  final int? parentIndex;
  final int firstRun;
  final int runCount;
  final int projectedUtf8Length;
  final int projectedUtf16Length;
  final bool isNoncontiguous;
  final bool isSelected;
}

/// Exact outer-to-inner structural ancestry for one point query.
final class FlarkV3DocumentPointPath {
  FlarkV3DocumentPointPath._(this.nodes)
    : assert(nodes.isNotEmpty),
      assert(nodes.last.isSelected);

  final List<FlarkV3DocumentPointPathNode> nodes;

  /// Outermost parser-authored node in this ancestry.
  FlarkV3DocumentPointPathNode get root => nodes.first;

  /// Convenience view retained for the shipped block-quote projection.
  ///
  /// Generic path consumers should inspect [root]. This accessor does not
  /// define the shape or depth of a point path.
  FlarkV3DocumentPointPathNode get blockQuoteAncestor {
    final ancestor = root;
    if (ancestor.kind != FlarkV3DocumentPointPathNodeKind.blockQuote) {
      throw StateError('This point path has no outer block-quote ancestor.');
    }
    return ancestor;
  }

  FlarkV3DocumentPointPathNode get selectedLeaf => nodes.last;
}

/// Stable semantic names in recursive-Green kind registry schema 1.
enum FlarkV3RecursiveGreenKind {
  document(1),
  blockQuote(2),
  list(3),
  item(4),
  paragraph(5),
  indentedCode(6),
  fencedCode(7),
  htmlBlock(8),
  heading(12),
  thematicBreak(13);

  const FlarkV3RecursiveGreenKind(this.id);

  final int id;

  bool get isInlineBearing =>
      this == FlarkV3RecursiveGreenKind.paragraph ||
      this == FlarkV3RecursiveGreenKind.heading;
}

enum FlarkV3RecursiveGreenCoveragePart {
  content,
  containerMarker,
  blockMarker,
  gap,
  terminal,
}

enum FlarkV3RecursiveGreenLogicalAtomKind {
  none,
  identity,
  tabToSpaces,
  hiddenUpstream,
  lfToLf,
  crLfToLf,
  loneCrToLf,
  nulToReplacement,
}

/// One parser-authored physical-to-logical projection atom.
final class FlarkV3RecursiveGreenLogicalAtom {
  const FlarkV3RecursiveGreenLogicalAtom._({
    required this.kind,
    this.targetOwnerDepth,
    this.spaces,
  });

  final FlarkV3RecursiveGreenLogicalAtomKind kind;
  final int? targetOwnerDepth;
  final int? spaces;
}

/// One final-kind ancestor in an authenticated recursive-Green point path.
final class FlarkV3RecursiveGreenAncestor {
  const FlarkV3RecursiveGreenAncestor._({
    required this.frameId,
    required this.kindId,
  });

  /// The full unsigned 64-bit parser identity, retained exactly on Web too.
  final BigInt frameId;
  final int kindId;

  FlarkV3RecursiveGreenKind? get kind {
    for (final candidate in FlarkV3RecursiveGreenKind.values) {
      if (candidate.id == kindId) return candidate;
    }
    return null;
  }
}

final class FlarkV3RecursiveGreenQueryWork {
  const FlarkV3RecursiveGreenQueryWork({
    required this.eventsScanned,
    required this.storagePagesVisited,
    required this.maximumOpenDepth,
  });

  final int eventsScanned;
  final int storagePagesVisited;
  final int maximumOpenDepth;
}

/// Why the current preview grammar returned exact source instead of guessing.
///
/// The private parser still records the precise opener for differential and
/// promotion tests. The ordinary API deliberately does not freeze a public
/// enumeration of grammar features that have not shipped yet.
enum FlarkV3DocumentUnknownReason { blankBoundary, unsupportedSource }

/// Exact root structure supported by the current narrow grammar milestone.
final class FlarkV3DocumentStructure {
  const FlarkV3DocumentStructure({
    required this.kind,
    required this.source,
    required this.visibleSource,
    required this.referenceDefinitionCount,
    this.unknownReason,
    this.fencedCode,
    this.heading,
    this.thematicBreak,
    this.indentedCode,
    this.blockQuote,
    this.bulletList,
    this.orderedList,
  }) : assert(referenceDefinitionCount >= 0),
       assert(
         kind == FlarkV3DocumentStructureKind.unknown
             ? unknownReason != null
             : unknownReason == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.fencedCode
             ? fencedCode != null
             : fencedCode == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.heading
             ? heading != null
             : heading == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.thematicBreak
             ? thematicBreak != null
             : thematicBreak == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.indentedCode
             ? indentedCode != null
             : indentedCode == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.blockQuote
             ? blockQuote != null
             : blockQuote == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.bulletList
             ? bulletList != null
             : bulletList == null,
       ),
       assert(
         kind == FlarkV3DocumentStructureKind.orderedList
             ? orderedList != null
             : orderedList == null,
       );

  final FlarkV3DocumentStructureKind kind;
  final FlarkV3SourceSpan source;
  final FlarkV3SourceSpan visibleSource;
  final int referenceDefinitionCount;
  final FlarkV3DocumentUnknownReason? unknownReason;
  final FlarkV3FencedCodeFacts? fencedCode;
  final FlarkV3HeadingFacts? heading;
  final FlarkV3ThematicBreakFacts? thematicBreak;
  final FlarkV3IndentedCodeFacts? indentedCode;
  final FlarkV3BlockQuoteFacts? blockQuote;
  final FlarkV3BulletListFacts? bulletList;
  final FlarkV3OrderedListFacts? orderedList;

  /// Parser-certified inline content owned by this leaf kind, if any.
  ///
  /// This is a capability contract, not Dart-side Markdown recognition.
  FlarkV3SourceSpan? get inlineContentSource => switch (kind) {
    FlarkV3DocumentStructureKind.paragraph => visibleSource,
    FlarkV3DocumentStructureKind.heading => heading!.contentSource,
    _ => null,
  };

  /// Whether a parser-authored whole-leaf inline sidecar may bind this leaf.
  ///
  /// Empty heading content remains exact structure but has no inline
  /// authority to request, publish, or cache.
  bool get canCarryInlineFacts {
    final inlineSource = inlineContentSource;
    return inlineSource != null &&
        inlineSource.startUtf8 != inlineSource.endUtf8;
  }
}

/// Exact physical-to-visible projection summary for the current narrow root.
final class FlarkV3DocumentProjection {
  const FlarkV3DocumentProjection({
    required this.kind,
    required this.source,
    required this.projectedSource,
    required this.runCount,
  }) : assert(runCount >= 0);

  final FlarkV3DocumentStructureKind kind;
  final FlarkV3SourceSpan source;
  final FlarkV3SourceSpan projectedSource;
  final int runCount;
}

/// Hard caller-isolate limits for one consecutive top-level block page.
final class FlarkV3DocumentBlockRangeBudget {
  const FlarkV3DocumentBlockRangeBudget({
    this.maximumEncodedBytes = 4096,
    this.maximumBlockCount = 24,
    this.maximumStoragePagesVisited = 25,
    this.maximumOpenDepth = 16,
    this.maximumTreeNodesVisited = 320,
  }) : assert(maximumEncodedBytes > 0),
       assert(maximumBlockCount > 0),
       assert(maximumStoragePagesVisited > 0),
       assert(maximumOpenDepth > 0),
       assert(maximumTreeNodesVisited > 0);

  final int maximumEncodedBytes;
  final int maximumBlockCount;

  /// Includes the initial point-location page.
  ///
  /// The default guarantees that a 24-block output quantum can cross 24
  /// maximally sparse packed leaves. This remains a bounded 100 KiB
  /// authentication window; callers may choose a smaller latency quantum and
  /// accept additional continuation frames.
  final int maximumStoragePagesVisited;
  final int maximumOpenDepth;
  final int maximumTreeNodesVisited;
}

/// One parser-authored structure-only top-level block.
///
/// Inline and noncontiguous display sidecars deliberately remain absent. The
/// active point-query path may independently refine the selected block.
final class FlarkV3DocumentStructuralBlock {
  const FlarkV3DocumentStructuralBlock({
    required this.ordinal,
    required this.structure,
    required this.projection,
  }) : assert(ordinal >= 0);

  final int ordinal;
  final FlarkV3DocumentStructure structure;
  final FlarkV3DocumentProjection projection;
}

/// Opaque, runtime-owned cursor for the next page of one exact range.
///
/// Applications can retain and return this value only to the runtime that
/// minted it. It exposes identity for cache invalidation but no host bytes.
abstract interface class FlarkV3DocumentBlockRangeContinuation {
  int get sourceRevision;
  int get structureGeneration;
  FlarkV3SourceSpan get requestedSource;
}

sealed class FlarkV3DocumentBlockRangeResult {
  const FlarkV3DocumentBlockRangeResult({required this.sourceRevision});

  final int sourceRevision;
}

/// One exact-current bounded page of consecutive structural blocks.
final class FlarkV3DocumentStructuralBlockRange
    extends FlarkV3DocumentBlockRangeResult {
  FlarkV3DocumentStructuralBlockRange({
    required super.sourceRevision,
    required this.structureRevision,
    required this.structureGeneration,
    required this.requestedSource,
    required this.coveredSource,
    required List<FlarkV3DocumentStructuralBlock> blocks,
    required this.continuation,
  }) : blocks = List<FlarkV3DocumentStructuralBlock>.unmodifiable(blocks);

  final int structureRevision;
  final int structureGeneration;
  final FlarkV3SourceSpan requestedSource;
  final FlarkV3SourceSpan coveredSource;
  final List<FlarkV3DocumentStructuralBlock> blocks;
  final FlarkV3DocumentBlockRangeContinuation? continuation;

  bool get complete => continuation == null;
}

/// Parser-authored display family for one recursive-Green renderable row.
enum FlarkV3RecursiveGreenRowPresentationKind {
  inline,
  fencedCode,
  indentedCode,
  html,
  thematicBreak,
}

/// Parser-certified editing capability for one recursive-Green row.
///
/// [contiguous] exposes one exact physical source cut. [projectedReserved]
/// reserves the wire discriminant for a future row-keyed piecewise projection;
/// it deliberately grants no editing authority yet. [unavailable] records an
/// exact structural row whose logical content cannot be represented by the
/// current contiguous contract.
enum FlarkV3RecursiveGreenRowEditCapability {
  contiguous,
  projectedReserved,
  unavailable,
}

enum FlarkV3RecursiveGreenListStyle { bullet, ordered }

enum FlarkV3RecursiveGreenHeadingStyle { atx, setext }

sealed class FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenPathFact();
}

/// Exact normalized List open/close facts carried on one row ancestry.
final class FlarkV3RecursiveGreenListPathFact
    extends FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenListPathFact({
    required this.style,
    required this.start,
    required this.tight,
    this.bulletMarker,
    this.orderedDelimiter,
  }) : assert(
         style == FlarkV3RecursiveGreenListStyle.bullet
             ? bulletMarker != null && orderedDelimiter == null
             : bulletMarker == null && orderedDelimiter != null,
       );

  final FlarkV3RecursiveGreenListStyle style;
  final FlarkV3BulletListMarker? bulletMarker;
  final FlarkV3OrderedListDelimiter? orderedDelimiter;
  final int start;
  final bool tight;
}

/// Exact CommonMark Item open geometry for one row ancestry.
final class FlarkV3RecursiveGreenItemPathFact
    extends FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenItemPathFact({
    required this.markerOffset,
    required this.padding,
  });

  final int markerOffset;
  final int padding;
}

final class FlarkV3RecursiveGreenHeadingPathFact
    extends FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenHeadingPathFact({
    required this.level,
    required this.style,
  }) : assert(level >= 1 && level <= 6);

  final int level;
  final FlarkV3RecursiveGreenHeadingStyle style;
}

final class FlarkV3RecursiveGreenCodePathFact
    extends FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenCodePathFact({
    required this.marker,
    required this.fenceOffsetColumns,
    required this.minimumClosingLength,
  });

  final FlarkV3CodeFenceMarker marker;
  final int fenceOffsetColumns;
  final BigInt minimumClosingLength;
}

final class FlarkV3RecursiveGreenHtmlPathFact
    extends FlarkV3RecursiveGreenPathFact {
  const FlarkV3RecursiveGreenHtmlPathFact({required this.blockType});

  final int blockType;
}

/// One exact frame in a renderable row's outermost-to-owner ancestry.
final class FlarkV3RecursiveGreenRowPathFrame {
  const FlarkV3RecursiveGreenRowPathFrame({
    required this.frameId,
    required this.kind,
    required this.physicalSource,
    required this.isRowOwner,
    required this.isContainer,
    required this.hasOpenFact,
    required this.hasCloseFact,
    required this.fact,
  });

  final BigInt frameId;
  final FlarkV3RecursiveGreenKind kind;
  final FlarkV3SourceSpan physicalSource;
  final bool isRowOwner;
  final bool isContainer;
  final bool hasOpenFact;
  final bool hasCloseFact;
  final FlarkV3RecursiveGreenPathFact? fact;
}

/// One globally identified terminal render row in recursive Green authority.
///
/// [physicalSource] owns all row-local container and block markers.
/// [editableSource] is present only when [editCapability] is
/// [FlarkV3RecursiveGreenRowEditCapability.contiguous].
/// [path] is complete frame-for-frame authority for composable chrome.
final class FlarkV3RecursiveGreenRenderableRow {
  FlarkV3RecursiveGreenRenderableRow({
    required this.globalOrdinal,
    required this.frameId,
    required this.kind,
    required this.selected,
    required this.inlineCapable,
    required this.literal,
    required this.presentationKind,
    required this.editCapability,
    required this.physicalSource,
    required this.editableSource,
    required List<FlarkV3RecursiveGreenRowPathFrame> path,
  }) : assert(
         (editCapability ==
                 FlarkV3RecursiveGreenRowEditCapability.contiguous) ==
             (editableSource != null),
         'Only a contiguous recursive-Green row carries an editable span.',
       ),
       path = List<FlarkV3RecursiveGreenRowPathFrame>.unmodifiable(path);

  final BigInt globalOrdinal;
  final BigInt frameId;
  final FlarkV3RecursiveGreenKind kind;
  final bool selected;
  final bool inlineCapable;
  final bool literal;
  final FlarkV3RecursiveGreenRowPresentationKind presentationKind;
  final FlarkV3RecursiveGreenRowEditCapability editCapability;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan? editableSource;
  final List<FlarkV3RecursiveGreenRowPathFrame> path;
}

/// Exact-current bounded recursive-Green row directory returned by the normal
/// structural-range ABI.
///
/// Marker-free inline payloads are deliberately not embedded here. The
/// viewport-presentation aggregate binds them separately to [structuralAck],
/// row ordinal, frame ID, and editable span.
final class FlarkV3RecursiveGreenRowRange
    extends FlarkV3DocumentBlockRangeResult {
  FlarkV3RecursiveGreenRowRange({
    required super.sourceRevision,
    required this.structureRevision,
    required this.structureGeneration,
    required this.structuralAck,
    required this.requestedSource,
    required this.coveredSource,
    required this.startGlobalRowOrdinal,
    required this.totalGlobalRowCount,
    required this.selectedRowIndex,
    required List<FlarkV3RecursiveGreenRenderableRow> rows,
    required this.continuation,
  }) : rows = List<FlarkV3RecursiveGreenRenderableRow>.unmodifiable(rows);

  final int structureRevision;
  final int structureGeneration;
  final FlarkV3StructuralAck structuralAck;
  final FlarkV3SourceSpan requestedSource;
  final FlarkV3SourceSpan coveredSource;
  final BigInt startGlobalRowOrdinal;
  final BigInt totalGlobalRowCount;
  final int? selectedRowIndex;
  final List<FlarkV3RecursiveGreenRenderableRow> rows;
  final FlarkV3DocumentBlockRangeContinuation? continuation;

  bool get complete => continuation == null;
  FlarkV3RecursiveGreenRenderableRow? get selectedRow =>
      selectedRowIndex == null ? null : rows[selectedRowIndex!];
}

final class FlarkV3DocumentPendingBlockRange
    extends FlarkV3DocumentBlockRangeResult {
  const FlarkV3DocumentPendingBlockRange({
    required super.sourceRevision,
    required this.reason,
    required this.stableStructureRevision,
  });

  final FlarkV3DocumentPendingReason reason;
  final int? stableStructureRevision;
}

final class FlarkV3DocumentSourceGapBlockRange
    extends FlarkV3DocumentBlockRangeResult {
  const FlarkV3DocumentSourceGapBlockRange({
    required super.sourceRevision,
    required this.structureRevision,
    required this.structureGeneration,
    required this.requestedSource,
    required this.reason,
  });

  final int structureRevision;
  final int structureGeneration;
  final FlarkV3SourceSpan requestedSource;
  final FlarkV3DocumentQueryGapReason reason;
}

sealed class FlarkV3DocumentQueryResult {
  const FlarkV3DocumentQueryResult({required this.sourceRevision});

  final int sourceRevision;
}

/// Exact-current recursive-Green authority for one selected source atom.
///
/// This is intentionally not coerced into the legacy flat block projection.
/// [source] and [logicalAtom] are sufficient for an identity-mapped active
/// span; rendering the surrounding container requires a separate bounded
/// projection payload.
final class FlarkV3RecursiveGreenPointQuery extends FlarkV3DocumentQueryResult {
  FlarkV3RecursiveGreenPointQuery._({
    required super.sourceRevision,
    required this.structureRevision,
    required this.source,
    required this.pointUtf8,
    required this.pointUtf16,
    required this.affinity,
    required this.logicalUtf8Length,
    required this.logicalUtf16Length,
    required this.coveragePart,
    required this.logicalAtom,
    required this.ownerIndex,
    required List<FlarkV3RecursiveGreenAncestor> ancestry,
    required this.work,
    this.paragraphSource,
    this.inlineSource,
    this.inlineFacts,
  }) : ancestry = List<FlarkV3RecursiveGreenAncestor>.unmodifiable(ancestry);

  final int structureRevision;
  final FlarkV3SourceSpan source;
  final int pointUtf8;
  final int pointUtf16;
  final FlarkV3DocumentQueryAffinity affinity;
  final int logicalUtf8Length;
  final int logicalUtf16Length;
  final FlarkV3RecursiveGreenCoveragePart coveragePart;
  final FlarkV3RecursiveGreenLogicalAtom logicalAtom;
  final int ownerIndex;
  final List<FlarkV3RecursiveGreenAncestor> ancestry;
  final FlarkV3RecursiveGreenQueryWork work;

  /// Exact physical inline-bearing leaf selected by the installed
  /// recursive-Green
  /// sidecar, including any parser-owned container prefixes and line ending.
  ///
  /// Null means no exact sidecar for this point is installed yet.
  final FlarkV3SourceSpan? paragraphSource;

  /// Exact contiguous Paragraph or Heading content parsed for inline facts.
  ///
  /// This range is minted by the recursive-Green inline-leaf fence. Dart never
  /// discovers it by scanning Markdown markers or line prefixes.
  final FlarkV3SourceSpan? inlineSource;

  /// Parser-certified whole-leaf inline result for [inlineSource].
  final FlarkV3InlineFacts? inlineFacts;

  FlarkV3RecursiveGreenAncestor get owner => ancestry[ownerIndex];

  /// Whether this atom can back a direct source/display editing projection.
  bool get isIdentityEditableContent =>
      coveragePart == FlarkV3RecursiveGreenCoveragePart.content &&
      logicalAtom.kind == FlarkV3RecursiveGreenLogicalAtomKind.identity;
}

/// Exact-current, atomically installed structural facts.
final class FlarkV3DocumentStructuralQuery extends FlarkV3DocumentQueryResult {
  const FlarkV3DocumentStructuralQuery({
    required super.sourceRevision,
    required this.structureRevision,
    required this.structure,
    required this.projection,
    this.inlineFacts,
    this.indentedCodeProjection,
    this.pointPath,
    this.blockQuoteProjection,
    this.bulletListProjection,
    this.orderedListProjection,
  }) : assert(
         (inlineFacts == null ? 0 : 1) +
                 (indentedCodeProjection == null ? 0 : 1) +
                 (blockQuoteProjection == null ? 0 : 1) +
                 (bulletListProjection == null ? 0 : 1) +
                 (orderedListProjection == null ? 0 : 1) <=
             2,
         'A structural query carries at most one structural and one inline '
         'projection certificate.',
       ),
       assert(
         (inlineFacts == null ? 0 : 1) +
                     (indentedCodeProjection == null ? 0 : 1) +
                     (blockQuoteProjection == null ? 0 : 1) +
                     (bulletListProjection == null ? 0 : 1) +
                     (orderedListProjection == null ? 0 : 1) <
                 2 ||
             (inlineFacts != null &&
                 (bulletListProjection != null ||
                     orderedListProjection != null) &&
                 indentedCodeProjection == null &&
                 blockQuoteProjection == null &&
                 (bulletListProjection == null ||
                     orderedListProjection == null)),
         'Only one tight-list projection and its selected-item inline facts '
         'may be joined.',
       ),
       assert(
         blockQuoteProjection == null || pointPath != null,
         'A block-quote projection requires its exact point path.',
       ),
       assert(
         bulletListProjection == null || pointPath != null,
         'A bullet-list projection requires its exact point path.',
       ),
       assert(
         orderedListProjection == null || pointPath != null,
         'An ordered-list projection requires its exact point path.',
       );

  final int structureRevision;
  final FlarkV3DocumentStructure structure;
  final FlarkV3DocumentProjection projection;

  /// Optional parser-certified inline facts carried by viewport schema 8.
  ///
  /// For Paragraph and Heading these cover the whole inline-bearing leaf. For
  /// Tight lists may cover exactly the selected item content and are usable
  /// only when joined to the matching item projection. Absence is deliberately
  /// not equivalent to an empty authoritative result.
  final FlarkV3InlineFacts? inlineFacts;

  /// Optional parser-authored physical-line recipe for indented code.
  ///
  /// Absence means the selected structural block is exact but its bounded
  /// projection payload is not installed yet. Consumers must source-paint
  /// rather than deriving indentation in Dart.
  final FlarkV3IndentedCodeProjectionPayload? indentedCodeProjection;

  /// Optional exact container ancestry carried by viewport schema 4 or 5.
  final FlarkV3DocumentPointPath? pointPath;

  /// Optional parser-authored physical-line recipe for the selected quote path.
  final FlarkV3BlockQuoteProjectionPayload? blockQuoteProjection;

  /// Optional parser-authored item recipe and selected-item edit inputs.
  final FlarkV3BulletListProjectionPayload? bulletListProjection;

  /// Optional parser-authored ordered-item recipe and exact marker spelling.
  final FlarkV3OrderedListProjectionPayload? orderedListProjection;
}

enum FlarkV3DocumentPendingReason {
  initializing,
  sourceChanged,
  structurePending,
}

/// Honest current-source fallback while exact structure is not installed.
///
/// [stableStructureRevision] may still be painted from a consumer cache, but
/// it cannot authorize semantics, selection mapping, hit targets, or edits.
final class FlarkV3DocumentPendingQuery extends FlarkV3DocumentQueryResult {
  const FlarkV3DocumentPendingQuery({
    required super.sourceRevision,
    required this.reason,
    required this.stableStructureRevision,
  });

  final FlarkV3DocumentPendingReason reason;
  final int? stableStructureRevision;
}

enum FlarkV3DocumentQueryGapReason {
  openDepthLimit,
  encodedByteLimit,
  leafLimit,
  treeNodeLimit,
  undecodableClosure,
  unavailableFacts,
}

/// Exact-current source range returned when the requested structural closure
/// cannot fit the caller's declared query budget.
final class FlarkV3DocumentSourceGapQuery extends FlarkV3DocumentQueryResult {
  const FlarkV3DocumentSourceGapQuery({
    required super.sourceRevision,
    required this.structureRevision,
    required this.range,
    required this.reason,
  });

  final int structureRevision;
  final FlarkV3SourceSpan range;
  final FlarkV3DocumentQueryGapReason reason;
}

/// A corrupt or incompatible bounded host result, not a Markdown parse error.
final class FlarkV3DocumentQueryException implements Exception {
  const FlarkV3DocumentQueryException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3DocumentQueryException($message)';
}

/// Package-internal decoder for the bounded M1.1 viewport container.
///
/// Keeping this out of the host controller prevents Dart from becoming a
/// second Markdown parser. It validates and translates already-authoritative
/// Rust records into public value objects only.
final class FlarkV3DocumentQueryDecoder {
  const FlarkV3DocumentQueryDecoder._();

  static bool _bindingMatchesRecursiveGreenFrame(
    FlarkV3ProtocolU64 owner,
    BigInt frameId,
  ) {
    final tag = BigInt.one << 63;
    if (frameId <= BigInt.zero || frameId >= tag) return false;
    final encoded =
        (BigInt.from(owner.highWord) << 32) | BigInt.from(owner.lowWord);
    return encoded == (tag | frameId);
  }

  /// Joins one installed recursive-Green inline-leaf sidecar to its exact point.
  ///
  /// The binding supplies both physical leaf and contiguous inline
  /// ranges. This method validates those parser-authored cuts and decodes the
  /// sidecar bytes; it performs no Markdown recognition or source scan.
  static FlarkV3RecursiveGreenPointQuery joinRecursiveGreenInline({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required int expectedProfilePartition,
    required FlarkV3RecursiveGreenPointQuery query,
    required FlarkV3HotInlineSidecarBinding binding,
    required FlarkV3InlineSidecarQueryOutcome outcome,
  }) {
    if (query.sourceRevision != expectedSource.revision ||
        query.structureRevision != expectedSource.revision ||
        !(query.owner.kind?.isInlineBearing ?? false) ||
        !_bindingMatchesRecursiveGreenFrame(
          binding.blockOrdinal,
          query.owner.frameId,
        ) ||
        binding.parserProfile.value != expectedProfilePartition) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green inline sidecar has incompatible authority.',
      );
    }
    final paragraphSource = FlarkV3SourceSpan(
      startUtf8: binding.physicalStartUtf8,
      endUtf8: binding.physicalEndUtf8,
      startUtf16: binding.physicalStartUtf16,
      endUtf16: binding.physicalEndUtf16,
    );
    final inlineSource = FlarkV3SourceSpan(
      startUtf8: binding.visibleStartUtf8,
      endUtf8: binding.visibleEndUtf8,
      startUtf16: binding.visibleStartUtf16,
      endUtf16: binding.visibleEndUtf16,
    );
    final sourceBytes = expectedSource.metric.bytes;
    final sourceUtf16 = expectedSource.metric.utf16;
    if (!_containsSpan(paragraphSource, inlineSource) ||
        !_containsSpan(paragraphSource, query.source) ||
        paragraphSource.endUtf8 > sourceBytes ||
        paragraphSource.endUtf16 > sourceUtf16 ||
        sourceDocument.utf8ToUtf16(paragraphSource.startUtf8) !=
            paragraphSource.startUtf16 ||
        sourceDocument.utf8ToUtf16(paragraphSource.endUtf8) !=
            paragraphSource.endUtf16 ||
        sourceDocument.utf8ToUtf16(inlineSource.startUtf8) !=
            inlineSource.startUtf16 ||
        sourceDocument.utf8ToUtf16(inlineSource.endUtf8) !=
            inlineSource.endUtf16) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green inline sidecar has invalid source geometry.',
      );
    }

    final FlarkV3InlineFacts? facts = switch (outcome) {
      FlarkV3InlineSidecarQueryAuthoritative(
        :final factCount,
        :final encodedFacts,
        :final encodedValues,
      ) =>
        FlarkV3InlineFactsDecoder.decode(
          sourceDocument: sourceDocument,
          expectedSource: expectedSource,
          factSource: expectedSource,
          expectedProfilePartition: expectedProfilePartition,
          profilePartition: binding.parserProfile.value,
          expectedLeaf: inlineSource,
          factLeaf: inlineSource,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          factCount: factCount,
          encodedFacts: encodedFacts,
          inlineValues: encodedValues.isEmpty
              ? null
              : FlarkV3InlineValuesPayload(
                  sourceVersion: expectedSource,
                  profilePartition: binding.parserProfile.value,
                  source: inlineSource,
                  encodedBytes: encodedValues,
                ),
        ),
      FlarkV3InlineSidecarQueryUnsupported() =>
        FlarkV3InlineFactsDecoder.decode(
          sourceDocument: sourceDocument,
          expectedSource: expectedSource,
          factSource: expectedSource,
          expectedProfilePartition: expectedProfilePartition,
          profilePartition: binding.parserProfile.value,
          expectedLeaf: inlineSource,
          factLeaf: inlineSource,
          disposition: FlarkV3InlineFactsDisposition.unsupported,
          factCount: 0,
          encodedFacts: Uint8List(0),
        ),
      FlarkV3InlineSidecarQueryUnavailable() => null,
    };
    if (facts == null) return query;
    return FlarkV3RecursiveGreenPointQuery._(
      sourceRevision: query.sourceRevision,
      structureRevision: query.structureRevision,
      source: query.source,
      pointUtf8: query.pointUtf8,
      pointUtf16: query.pointUtf16,
      affinity: query.affinity,
      logicalUtf8Length: query.logicalUtf8Length,
      logicalUtf16Length: query.logicalUtf16Length,
      coveragePart: query.coveragePart,
      logicalAtom: query.logicalAtom,
      ownerIndex: query.ownerIndex,
      ancestry: query.ancestry,
      work: query.work,
      paragraphSource: paragraphSource,
      inlineSource: inlineSource,
      inlineFacts: facts,
    );
  }

  /// Decodes either the legacy flat viewport or recursive-Green schema 9.
  static FlarkV3DocumentQueryResult decodePointViewport({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required int expectedProfilePartition,
    required FlarkV3HostStructuralViewport viewport,
  }) {
    final bytes = viewport.encoded;
    if (bytes.length >= 12 &&
        ByteData.sublistView(bytes).getUint32(8, Endian.little) ==
            _viewportSchemaV9) {
      return _decodeRecursiveGreen(
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
        viewport: viewport,
      );
    }
    return decode(
      sourceDocument: sourceDocument,
      expectedSource: expectedSource,
      expectedProfilePartition: expectedProfilePartition,
      viewport: viewport,
    );
  }

  static FlarkV3DocumentStructuralQuery decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required int expectedProfilePartition,
    required FlarkV3HostStructuralViewport viewport,
  }) {
    if (viewport.sourceVersion != expectedSource) {
      throw const FlarkV3DocumentQueryException(
        'The viewport does not match the current exact source authority.',
      );
    }
    final sourceBytes = expectedSource.metric.bytes;
    final utf8ToUtf16 = sourceDocument.utf8ToUtf16;
    final bytes = viewport.encoded;
    final reader = _M11Reader(bytes);
    reader.expectMagic(_viewportMagic, 'viewport');
    final viewportSchema = reader.u32('viewport schema');
    final greenLength = reader.u32('green length');
    final projectionLength = reader.u32('projection length');
    late final int headerBytes;
    late final int payloadKind;
    late final int payloadLength;
    var pathNodeCount = 0;
    var pathTableLength = 0;
    switch (viewportSchema) {
      case _viewportSchemaV1:
        headerBytes = _viewportHeaderV1Bytes;
        payloadKind = _leafProjectionPayloadNone;
        payloadLength = 0;
        break;
      case _viewportSchemaV8:
        headerBytes = _viewportHeaderV8Bytes;
        payloadKind = _leafProjectionPayloadInline;
        payloadLength = reader.u32('inline length');
        break;
      case _viewportSchemaV3:
        headerBytes = _viewportHeaderV3Bytes;
        payloadKind = reader.variant('leaf projection payload kind');
        payloadLength = reader.u32('leaf projection payload length');
        break;
      case _viewportSchemaV4:
        headerBytes = _viewportHeaderV4Bytes;
        pathNodeCount = reader.u16('point-path node count');
        payloadKind = reader.u8('leaf projection payload kind');
        reader.expectU8(0, 'viewport schema-4 reserved field');
        pathTableLength = reader.u32('point-path table length');
        payloadLength = reader.u32('leaf projection payload length');
        break;
      case _viewportSchemaV5:
        headerBytes = _viewportHeaderV5Bytes;
        pathNodeCount = reader.u16('point-path node count');
        payloadKind = reader.u8('leaf projection payload kind');
        reader.expectU8(0, 'viewport schema-5 reserved field');
        pathTableLength = reader.u32('point-path table length');
        payloadLength = reader.u32('leaf projection payload length');
        break;
      case _viewportSchemaV6:
        headerBytes = _viewportHeaderV6Bytes;
        pathNodeCount = reader.u16('point-path node count');
        payloadKind = reader.u8('leaf projection payload kind');
        reader.expectU8(0, 'viewport schema-6 reserved field');
        pathTableLength = reader.u32('point-path table length');
        payloadLength = reader.u32('leaf projection payload length');
        break;
      case _viewportSchemaV7:
        headerBytes = _viewportHeaderV7Bytes;
        pathNodeCount = reader.u16('point-path node count');
        payloadKind = reader.u8('leaf projection payload kind');
        reader.expectU8(0, 'viewport schema-7 reserved field');
        pathTableLength = reader.u32('point-path table length');
        payloadLength = reader.u32('leaf projection payload length');
        break;
      default:
        throw const FlarkV3DocumentQueryException(
          'The host returned an unsupported viewport schema.',
        );
    }
    if (payloadKind != _leafProjectionPayloadNone &&
        payloadKind != _leafProjectionPayloadInline &&
        payloadKind != _leafProjectionPayloadIndentedCode &&
        payloadKind != _leafProjectionPayloadBlockQuote &&
        payloadKind != _leafProjectionPayloadList &&
        payloadKind != _leafProjectionPayloadListItem &&
        payloadKind != _leafProjectionPayloadOrderedListItem) {
      throw const FlarkV3DocumentQueryException(
        'The host returned an unknown leaf projection payload kind.',
      );
    }
    final pathEnvelopeMatchesSchema = switch (viewportSchema) {
      _viewportSchemaV4 =>
        payloadKind == _leafProjectionPayloadBlockQuote &&
            pathNodeCount == _blockQuotePointPathNodeCount &&
            pathTableLength ==
                _blockQuotePointPathNodeCount *
                    _documentPointPathV4NodeRecordBytes,
      _viewportSchemaV5 =>
        payloadKind == _leafProjectionPayloadList &&
            pathNodeCount > 0 &&
            pathTableLength ==
                pathNodeCount * _documentPointPathV5NodeRecordBytes,
      _viewportSchemaV6 =>
        payloadKind == _leafProjectionPayloadListItem &&
            pathNodeCount > 0 &&
            pathTableLength ==
                pathNodeCount * _documentPointPathV5NodeRecordBytes,
      _viewportSchemaV7 =>
        payloadKind == _leafProjectionPayloadOrderedListItem &&
            pathNodeCount > 0 &&
            pathTableLength ==
                pathNodeCount * _documentPointPathV5NodeRecordBytes,
      _ =>
        payloadKind != _leafProjectionPayloadBlockQuote &&
            payloadKind != _leafProjectionPayloadList &&
            payloadKind != _leafProjectionPayloadListItem &&
            payloadKind != _leafProjectionPayloadOrderedListItem &&
            pathNodeCount == 0 &&
            pathTableLength == 0,
    };
    if (!pathEnvelopeMatchesSchema) {
      throw const FlarkV3DocumentQueryException(
        'The viewport schema does not match its point-path payload.',
      );
    }
    if (greenLength != _greenRecordBytes ||
        projectionLength != _projectionRecordBytes ||
        bytes.length !=
            headerBytes +
                greenLength +
                projectionLength +
                pathTableLength +
                payloadLength) {
      throw const FlarkV3DocumentQueryException(
        'The host returned an incompatible bounded viewport envelope.',
      );
    }

    final greenStart = headerBytes;
    final projectionStart = greenStart + greenLength;
    final pathTableStart = projectionStart + projectionLength;
    final payloadStart = pathTableStart + pathTableLength;
    final structural = _decodeStructuralPair(
      Uint8List.sublistView(bytes, greenStart, projectionStart),
      Uint8List.sublistView(bytes, projectionStart, pathTableStart),
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final green = structural.structure;
    final projection = structural.projection;
    final pointPath = switch (viewportSchema) {
      _viewportSchemaV4 => _decodeBlockQuotePointPathV4(
        Uint8List.sublistView(bytes, pathTableStart, payloadStart),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
        structure: green,
        projection: projection,
      ),
      _viewportSchemaV5 => _decodePointPathV5(
        Uint8List.sublistView(bytes, pathTableStart, payloadStart),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
        nodeCount: pathNodeCount,
      ),
      _viewportSchemaV6 => _decodePointPathV5(
        Uint8List.sublistView(bytes, pathTableStart, payloadStart),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
        nodeCount: pathNodeCount,
      ),
      _viewportSchemaV7 => _decodePointPathV5(
        Uint8List.sublistView(bytes, pathTableStart, payloadStart),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
        nodeCount: pathNodeCount,
      ),
      _ => null,
    };
    FlarkV3InlineFacts? inlineFacts;
    FlarkV3IndentedCodeProjectionPayload? indentedCodeProjection;
    FlarkV3BlockQuoteProjectionPayload? blockQuoteProjection;
    FlarkV3BulletListProjectionPayload? bulletListProjection;
    FlarkV3OrderedListProjectionPayload? orderedListProjection;
    final payload = Uint8List.sublistView(bytes, payloadStart);
    switch (payloadKind) {
      case _leafProjectionPayloadNone:
        if (payloadLength != 0) {
          throw const FlarkV3DocumentQueryException(
            'A payload-free viewport contains trailing projection bytes.',
          );
        }
        break;
      case _leafProjectionPayloadInline:
        if (payloadLength == 0) {
          throw const FlarkV3DocumentQueryException(
            'An inline viewport omitted its projection payload.',
          );
        }
        inlineFacts = _decodeInlineFacts(
          payload,
          sourceDocument: sourceDocument,
          expectedSource: expectedSource,
          expectedProfilePartition: expectedProfilePartition,
          expectedLeaf:
              green.kind == FlarkV3DocumentStructureKind.bulletList ||
                  green.kind == FlarkV3DocumentStructureKind.orderedList
              ? null
              : projection.projectedSource,
          structure: green,
        );
        break;
      case _leafProjectionPayloadIndentedCode:
        final facts = green.indentedCode;
        if (payloadLength == 0 ||
            green.kind != FlarkV3DocumentStructureKind.indentedCode ||
            facts == null) {
          throw const FlarkV3DocumentQueryException(
            'An indented-code payload does not match its structural block.',
          );
        }
        try {
          indentedCodeProjection = FlarkV3IndentedCodeProjectionDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: expectedSource,
            source: green.source,
            facts: facts,
            encodedRecords: payload,
          );
        } on FlarkV3IndentedCodeProjectionDecodeException catch (error) {
          throw FlarkV3DocumentQueryException(error.message);
        }
        break;
      case _leafProjectionPayloadBlockQuote:
        final facts = green.blockQuote;
        if (payloadLength == 0 ||
            green.kind != FlarkV3DocumentStructureKind.blockQuote ||
            facts == null ||
            pointPath == null) {
          throw const FlarkV3DocumentQueryException(
            'A block-quote payload does not match its exact point path.',
          );
        }
        try {
          blockQuoteProjection = FlarkV3BlockQuoteProjectionDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: expectedSource,
            source: green.source,
            facts: facts,
            pointPath: pointPath,
            encodedRecords: payload,
          );
        } on FlarkV3BlockQuoteProjectionDecodeException catch (error) {
          throw FlarkV3DocumentQueryException(error.message);
        }
        break;
      case _leafProjectionPayloadList:
        final facts = green.bulletList;
        if (payloadLength == 0 ||
            green.kind != FlarkV3DocumentStructureKind.bulletList ||
            facts == null ||
            pointPath == null) {
          throw const FlarkV3DocumentQueryException(
            'A list payload does not match its exact point path.',
          );
        }
        try {
          bulletListProjection = FlarkV3BulletListProjectionDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: expectedSource,
            source: green.source,
            facts: facts,
            pointPath: pointPath,
            encodedRecords: payload,
          );
        } on FlarkV3BulletListProjectionDecodeException catch (error) {
          throw FlarkV3DocumentQueryException(error.message);
        }
        break;
      case _leafProjectionPayloadListItem:
        final facts = green.bulletList;
        if (payloadLength == 0 ||
            green.kind != FlarkV3DocumentStructureKind.bulletList ||
            facts == null ||
            pointPath == null) {
          throw const FlarkV3DocumentQueryException(
            'A compact list-item payload does not match its exact point path.',
          );
        }
        try {
          bulletListProjection =
              FlarkV3BulletListProjectionDecoder.decodeSelectedItem(
                sourceDocument: sourceDocument,
                expectedSource: expectedSource,
                source: green.source,
                facts: facts,
                pointPath: pointPath,
                encodedPayload: payload,
              );
        } on FlarkV3BulletListProjectionDecodeException catch (error) {
          throw FlarkV3DocumentQueryException(error.message);
        }
        break;
      case _leafProjectionPayloadOrderedListItem:
        final facts = green.orderedList;
        if (payloadLength == 0 ||
            green.kind != FlarkV3DocumentStructureKind.orderedList ||
            facts == null ||
            pointPath == null) {
          throw const FlarkV3DocumentQueryException(
            'A compact ordered-list item payload does not match its exact '
            'point path.',
          );
        }
        try {
          orderedListProjection =
              FlarkV3OrderedListProjectionDecoder.decodeSelectedItem(
                sourceDocument: sourceDocument,
                expectedSource: expectedSource,
                source: green.source,
                facts: facts,
                pointPath: pointPath,
                encodedPayload: payload,
              );
        } on FlarkV3OrderedListProjectionDecodeException catch (error) {
          throw FlarkV3DocumentQueryException(error.message);
        }
        break;
      default:
        throw const FlarkV3DocumentQueryException(
          'The host returned an unknown leaf projection payload kind.',
        );
    }

    return FlarkV3DocumentStructuralQuery(
      sourceRevision: expectedSource.revision,
      structureRevision: viewport.sourceVersion.revision,
      structure: green,
      projection: projection,
      inlineFacts: inlineFacts,
      indentedCodeProjection: indentedCodeProjection,
      pointPath: pointPath,
      blockQuoteProjection: blockQuoteProjection,
      bulletListProjection: bulletListProjection,
      orderedListProjection: orderedListProjection,
    );
  }

  static FlarkV3RecursiveGreenPointQuery _decodeRecursiveGreen({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3HostStructuralViewport viewport,
  }) {
    if (viewport.sourceVersion != expectedSource) {
      throw const FlarkV3DocumentQueryException(
        'The viewport does not match the current exact source authority.',
      );
    }
    final bytes = viewport.encoded;
    final reader = _M11Reader(bytes);
    reader.expectMagic(_viewportMagic, 'viewport');
    reader.expectU32(_viewportSchemaV9, 'recursive-Green viewport schema');
    reader.expectU32(
      _recursiveGreenViewportHeaderBytes,
      'recursive-Green viewport header size',
    );
    reader.expectU32(
      _recursiveGreenAncestorRecordBytes,
      'recursive-Green ancestor record size',
    );
    reader.expectU32(
      _recursiveGreenKindRegistrySchema,
      'recursive-Green kind registry schema',
    );
    reader.expectU32(
      _recursiveGreenCoverageSchema,
      'recursive-Green coverage schema',
    );
    reader.expectU32(
      _recursiveGreenLogicalAtomSchema,
      'recursive-Green logical-atom schema',
    );
    reader.expectU32(0, 'recursive-Green viewport flags');
    final ancestryCount = reader.u32('recursive-Green ancestry count');
    final ownerIndex = reader.u32('recursive-Green owner index');
    final ownerKind = reader.u16('recursive-Green owner kind');
    final coverageTag = reader.u8('recursive-Green coverage part');
    final logicalAtomTag = reader.u8('recursive-Green logical atom');
    final startUtf8 = reader.u32('recursive-Green source start UTF-8');
    final endUtf8 = reader.u32('recursive-Green source end UTF-8');
    final startUtf16 = reader.u32('recursive-Green source start UTF-16');
    final endUtf16 = reader.u32('recursive-Green source end UTF-16');
    final physicalUtf8 = reader.u32('recursive-Green physical UTF-8');
    final physicalUtf16 = reader.u32('recursive-Green physical UTF-16');
    final logicalUtf8 = reader.u32('recursive-Green logical UTF-8');
    final logicalUtf16 = reader.u32('recursive-Green logical UTF-16');
    final pointUtf8 = reader.u32('recursive-Green point UTF-8');
    final pointUtf16 = reader.u32('recursive-Green point UTF-16');
    final affinityTag = reader.u32('recursive-Green point affinity');
    final logicalArgument0 = reader.u32('recursive-Green logical argument 0');
    final logicalArgument1 = reader.u32('recursive-Green logical argument 1');
    final eventsScanned = reader.u32('recursive-Green events scanned');
    final storagePagesVisited = reader.u32(
      'recursive-Green storage pages visited',
    );
    final maximumOpenDepth = reader.u32('recursive-Green maximum open depth');

    final expectedBytes =
        _recursiveGreenViewportHeaderBytes +
        ancestryCount * _recursiveGreenAncestorRecordBytes;
    if (ancestryCount == 0 ||
        ownerIndex >= ancestryCount ||
        bytes.length != expectedBytes ||
        viewport.receipt.encodedBytes != expectedBytes ||
        storagePagesVisited != viewport.receipt.leafCount ||
        maximumOpenDepth != viewport.receipt.openDepth ||
        eventsScanned == 0 ||
        storagePagesVisited == 0 ||
        maximumOpenDepth < ancestryCount ||
        viewport.receipt.treeNodesVisited == 0) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green viewport disagrees with its host receipt.',
      );
    }
    final sourceBytes = expectedSource.metric.bytes;
    final sourceUtf16 = expectedSource.metric.utf16;
    if (startUtf8 >= endUtf8 ||
        startUtf16 >= endUtf16 ||
        endUtf8 > sourceBytes ||
        endUtf16 > sourceUtf16 ||
        endUtf8 - startUtf8 != physicalUtf8 ||
        endUtf16 - startUtf16 != physicalUtf16 ||
        sourceDocument.utf8ToUtf16(startUtf8) != startUtf16 ||
        sourceDocument.utf8ToUtf16(endUtf8) != endUtf16 ||
        pointUtf8 > sourceBytes ||
        pointUtf16 > sourceUtf16 ||
        sourceDocument.utf8ToUtf16(pointUtf8) != pointUtf16 ||
        logicalUtf8 < logicalUtf16 ||
        ((logicalUtf8 == 0) != (logicalUtf16 == 0))) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green viewport has invalid source coordinates.',
      );
    }
    final affinity = switch (affinityTag) {
      0 => FlarkV3DocumentQueryAffinity.upstream,
      1 => FlarkV3DocumentQueryAffinity.downstream,
      _ => throw const FlarkV3DocumentQueryException(
        'The recursive-Green viewport has an unknown affinity.',
      ),
    };
    final effectiveUtf8 =
        pointUtf8 == sourceBytes ||
            (affinity == FlarkV3DocumentQueryAffinity.upstream && pointUtf8 > 0)
        ? pointUtf8 - 1
        : pointUtf8;
    final effectiveUtf16 =
        pointUtf16 == sourceUtf16 ||
            (affinity == FlarkV3DocumentQueryAffinity.upstream &&
                pointUtf16 > 0)
        ? pointUtf16 - 1
        : pointUtf16;
    if (effectiveUtf8 < startUtf8 ||
        effectiveUtf8 >= endUtf8 ||
        effectiveUtf16 < startUtf16 ||
        effectiveUtf16 >= endUtf16 ||
        viewport.range.start.bytes != startUtf8 ||
        viewport.range.start.utf16 != startUtf16 ||
        viewport.range.end.bytes != endUtf8 ||
        viewport.range.end.utf16 != endUtf16) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green coverage does not contain its queried point.',
      );
    }

    final coveragePart = switch (coverageTag) {
      1 => FlarkV3RecursiveGreenCoveragePart.content,
      2 => FlarkV3RecursiveGreenCoveragePart.containerMarker,
      3 => FlarkV3RecursiveGreenCoveragePart.blockMarker,
      4 => FlarkV3RecursiveGreenCoveragePart.gap,
      5 => FlarkV3RecursiveGreenCoveragePart.terminal,
      _ => throw const FlarkV3DocumentQueryException(
        'The recursive-Green viewport has an unknown coverage part.',
      ),
    };
    final logicalAtom = _decodeRecursiveGreenLogicalAtom(
      logicalAtomTag,
      logicalArgument0,
      logicalArgument1,
      physicalUtf8: physicalUtf8,
      physicalUtf16: physicalUtf16,
      logicalUtf8: logicalUtf8,
      logicalUtf16: logicalUtf16,
    );
    final ancestry = <FlarkV3RecursiveGreenAncestor>[];
    var ownerFlags = 0;
    for (var index = 0; index < ancestryCount; index += 1) {
      final low = reader.u32('recursive-Green frame low word');
      final high = reader.u32('recursive-Green frame high word');
      final kind = reader.u16('recursive-Green ancestor kind');
      final flags = reader.u16('recursive-Green ancestor flags');
      reader.expectU32(0, 'recursive-Green ancestor reserved field');
      final isOwner = flags == _recursiveGreenAncestorOwnerFlag;
      if ((low == 0 && high == 0) ||
          kind == 0 ||
          (flags != 0 && !isOwner) ||
          isOwner != (index == ownerIndex)) {
        throw const FlarkV3DocumentQueryException(
          'The recursive-Green ancestry record is invalid.',
        );
      }
      if (isOwner) ownerFlags += 1;
      ancestry.add(
        FlarkV3RecursiveGreenAncestor._(
          frameId: (BigInt.from(high) << 32) | BigInt.from(low),
          kindId: kind,
        ),
      );
    }
    reader.expectEnd('recursive-Green viewport');
    if (ownerFlags != 1 || ancestry[ownerIndex].kindId != ownerKind) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green owner does not match its ancestry.',
      );
    }
    return FlarkV3RecursiveGreenPointQuery._(
      sourceRevision: expectedSource.revision,
      structureRevision: viewport.sourceVersion.revision,
      source: FlarkV3SourceSpan(
        startUtf8: startUtf8,
        endUtf8: endUtf8,
        startUtf16: startUtf16,
        endUtf16: endUtf16,
      ),
      pointUtf8: pointUtf8,
      pointUtf16: pointUtf16,
      affinity: affinity,
      logicalUtf8Length: logicalUtf8,
      logicalUtf16Length: logicalUtf16,
      coveragePart: coveragePart,
      logicalAtom: logicalAtom,
      ownerIndex: ownerIndex,
      ancestry: ancestry,
      work: FlarkV3RecursiveGreenQueryWork(
        eventsScanned: eventsScanned,
        storagePagesVisited: storagePagesVisited,
        maximumOpenDepth: maximumOpenDepth,
      ),
    );
  }

  static FlarkV3DecodedDocumentBlockRange decodeBlockRange({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3StructuralAck expectedStructuralAck,
    required FlarkV3HostStructuralBlockRange range,
  }) {
    if (range.sourceVersion != expectedSource) {
      throw const FlarkV3DocumentQueryException(
        'The block range does not match exact current source.',
      );
    }
    final bytes = range.encoded;
    if (bytes.lengthInBytes >= 12 &&
        ByteData.sublistView(bytes).getUint32(8, Endian.little) ==
            _recursiveGreenRowRangeSchema) {
      return _decodeRecursiveGreenRowRange(
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
        expectedStructuralAck: expectedStructuralAck,
        range: range,
      );
    }
    final headerEnd = bytes.length < _rangeHeaderBytes
        ? bytes.length
        : _rangeHeaderBytes;
    final header = _M11Reader(Uint8List.sublistView(bytes, 0, headerEnd));
    header.expectMagic(_rangeMagic, 'block range');
    header.expectU32(_rangeSchema, 'block range schema');
    header.expectU32(_rangeHeaderBytes, 'block range header size');
    header.expectU32(_rangeRecordBytes, 'block range record size');
    final blockCount = header.u32('block range count');
    final flags = header.u32('block range flags');
    header.expectU32(0, 'block range reserved field');
    header.expectEnd('block range header');

    final complete = flags & _rangeCompleteFlag != 0;
    if (flags & ~_rangeCompleteFlag != 0 ||
        blockCount != range.receipt.blockCount ||
        complete != range.receipt.complete ||
        complete != (range.continuation == null) ||
        bytes.length != _rangeHeaderBytes + blockCount * _rangeRecordBytes ||
        range.receipt.encodedBytes != bytes.length) {
      throw const FlarkV3DocumentQueryException(
        'The block-range envelope disagrees with its host receipt.',
      );
    }

    final sourceBytes = expectedSource.metric.bytes;
    final utf8ToUtf16 = sourceDocument.utf8ToUtf16;
    final blocks = <FlarkV3DocumentStructuralBlock>[];
    int? priorOrdinal;
    FlarkV3SourceSpan? priorSource;
    for (var index = 0; index < blockCount; index += 1) {
      final recordStart = _rangeHeaderBytes + index * _rangeRecordBytes;
      final record = Uint8List.sublistView(
        bytes,
        recordStart,
        recordStart + _rangeRecordBytes,
      );
      final reader = _M11Reader(record);
      final ordinal = reader.u64AsU32('block ordinal');
      final startUtf8 = reader.u32('block source start UTF-8');
      final startUtf16 = reader.u32('block source start UTF-16');
      final endUtf8 = reader.u32('block source end UTF-8');
      final endUtf16 = reader.u32('block source end UTF-16');
      if (endUtf8 < startUtf8 || endUtf16 < startUtf16) {
        throw const FlarkV3DocumentQueryException(
          'A block-range record has inverted source coordinates.',
        );
      }
      final recordSource = FlarkV3SourceSpan(
        startUtf8: startUtf8,
        startUtf16: startUtf16,
        endUtf8: endUtf8,
        endUtf16: endUtf16,
      );
      if (recordSource.endUtf8 > sourceBytes ||
          recordSource.endUtf16 > expectedSource.metric.utf16 ||
          utf8ToUtf16(recordSource.startUtf8) != recordSource.startUtf16 ||
          utf8ToUtf16(recordSource.endUtf8) != recordSource.endUtf16) {
        throw const FlarkV3DocumentQueryException(
          'A block-range record is outside exact source coordinates.',
        );
      }
      final structural = _decodeStructuralPair(
        Uint8List.sublistView(record, 24, 24 + _greenRecordBytes),
        Uint8List.sublistView(record, 104),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
      );
      if (!_sameSpan(recordSource, structural.structure.source) ||
          (priorOrdinal != null && ordinal != priorOrdinal + 1) ||
          (priorSource != null &&
              (priorSource.endUtf8 != recordSource.startUtf8 ||
                  priorSource.endUtf16 != recordSource.startUtf16)) ||
          !_sourceSpansOverlap(recordSource, range.requestedRange)) {
        throw const FlarkV3DocumentQueryException(
          'Block-range records are not one consecutive requested sequence.',
        );
      }
      blocks.add(
        FlarkV3DocumentStructuralBlock(
          ordinal: ordinal,
          structure: structural.structure,
          projection: structural.projection,
        ),
      );
      priorOrdinal = ordinal;
      priorSource = recordSource;
    }

    final covered = metricRange(range.coveredRange);
    if (blocks.isEmpty) {
      if (!complete ||
          !_sameSpan(covered, metricRange(range.requestedRange)) ||
          range.requestedRange.start != range.requestedRange.end) {
        throw const FlarkV3DocumentQueryException(
          'An empty block page does not close an empty requested range.',
        );
      }
    } else {
      final first = blocks.first.structure.source;
      final last = blocks.last.structure.source;
      if (first.startUtf8 != covered.startUtf8 ||
          first.startUtf16 != covered.startUtf16 ||
          last.endUtf8 != covered.endUtf8 ||
          last.endUtf16 != covered.endUtf16) {
        throw const FlarkV3DocumentQueryException(
          'Block records do not cover the host-receipted range.',
        );
      }
    }
    return FlarkV3DecodedDocumentBlockRange(
      blocks: blocks,
      coveredSource: covered,
    );
  }

  static FlarkV3DecodedDocumentBlockRange _decodeRecursiveGreenRowRange({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3StructuralAck expectedStructuralAck,
    required FlarkV3HostStructuralBlockRange range,
  }) {
    final bytes = range.encoded;
    if (bytes.lengthInBytes < _recursiveGreenRowRangeHeaderBytes) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green row-range header is truncated.',
      );
    }
    final header = _M11Reader(
      Uint8List.sublistView(bytes, 0, _recursiveGreenRowRangeHeaderBytes),
    );
    header.expectMagic(_rangeMagic, 'recursive-Green row range');
    header.expectU32(
      _recursiveGreenRowRangeSchema,
      'recursive-Green row-range schema',
    );
    header.expectU32(
      _recursiveGreenRowRangeHeaderBytes,
      'recursive-Green row-range header size',
    );
    header.expectU32(
      _recursiveGreenRowRecordBytes,
      'recursive-Green row record size',
    );
    header.expectU32(
      _recursiveGreenPathRecordBytes,
      'recursive-Green path record size',
    );
    final rowCount = header.u32('recursive-Green row count');
    final pathCount = header.u32('recursive-Green path count');
    final flags = header.u32('recursive-Green row-range flags');
    final encodedSelectedRow = header.u32('recursive-Green selected row index');
    final startGlobalRowOrdinal = header.u64(
      'recursive-Green start global row ordinal',
    );
    final totalGlobalRowCount = header.u64(
      'recursive-Green total global row count',
    );
    header.expectU32(
      _recursiveGreenKindRegistrySchema,
      'recursive-Green row kind registry',
    );
    header.expectU32(
      _recursiveGreenRowFactRegistrySchema,
      'recursive-Green row fact registry',
    );
    final sourceRevision = header.u32('recursive-Green row source revision');
    final parseGeneration = header.u32('recursive-Green row parse generation');
    final publicationWords = <int>[
      header.u32('recursive-Green publication session word 0'),
      header.u32('recursive-Green publication session word 1'),
      header.u32('recursive-Green publication session word 2'),
      header.u32('recursive-Green publication session word 3'),
    ];
    header.expectU32(0, 'recursive-Green row reserved word 0');
    header.expectU32(0, 'recursive-Green row reserved word 1');
    header.expectEnd('recursive-Green row-range header');

    final complete = flags & _recursiveGreenRowRangeCompleteFlag != 0;
    final selectedRowIndex = encodedSelectedRow == _recursiveGreenNoSelectedRow
        ? null
        : encodedSelectedRow;
    final expectedLength =
        _recursiveGreenRowRangeHeaderBytes +
        rowCount * _recursiveGreenRowRecordBytes +
        pathCount * _recursiveGreenPathRecordBytes;
    final expectedPublication = expectedStructuralAck.publicationSession;
    if (flags & ~_recursiveGreenRowRangeCompleteFlag != 0 ||
        rowCount != range.receipt.blockCount ||
        complete != range.receipt.complete ||
        complete != (range.continuation == null) ||
        range.receipt.encodedBytes != bytes.lengthInBytes ||
        bytes.lengthInBytes != expectedLength ||
        selectedRowIndex != null && selectedRowIndex >= rowCount ||
        sourceRevision != expectedSource.revision ||
        parseGeneration != expectedStructuralAck.parseGeneration ||
        publicationWords[0] != expectedPublication.word0 ||
        publicationWords[1] != expectedPublication.word1 ||
        publicationWords[2] != expectedPublication.word2 ||
        publicationWords[3] != expectedPublication.word3 ||
        totalGlobalRowCount < startGlobalRowOrdinal + BigInt.from(rowCount)) {
      throw const FlarkV3DocumentQueryException(
        'The recursive-Green row range disagrees with exact host authority.',
      );
    }

    final rawRows = <_DecodedRecursiveGreenRow>[];
    for (var index = 0; index < rowCount; index += 1) {
      final offset =
          _recursiveGreenRowRangeHeaderBytes +
          index * _recursiveGreenRowRecordBytes;
      final reader = _M11Reader(
        Uint8List.sublistView(
          bytes,
          offset,
          offset + _recursiveGreenRowRecordBytes,
        ),
      );
      final globalOrdinal = reader.u64('recursive-Green row ordinal');
      final frameId = reader.u64('recursive-Green row frame');
      final kind = _recursiveGreenKind(reader.u16('recursive-Green row kind'));
      final rowFlags = reader.u16('recursive-Green row flags');
      final pathStart = reader.u32('recursive-Green row path start');
      final rowPathCount = reader.u32('recursive-Green row path count');
      final presentationKind = _recursiveGreenRowPresentationKind(
        reader.u16('recursive-Green row presentation kind'),
      );
      final editCapability = _recursiveGreenRowEditCapability(
        reader.u16('recursive-Green row edit capability'),
      );
      final physicalSource = _recursiveGreenMetricSpan(
        reader,
        'recursive-Green row physical source',
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
      );
      final encodedEditableSource = _recursiveGreenMetricSpan(
        reader,
        'recursive-Green row editable source',
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
      );
      final editableSource = switch (editCapability) {
        FlarkV3RecursiveGreenRowEditCapability.contiguous =>
          encodedEditableSource,
        FlarkV3RecursiveGreenRowEditCapability.projectedReserved ||
        FlarkV3RecursiveGreenRowEditCapability.unavailable =>
          _isZeroSpan(encodedEditableSource)
              ? null
              : throw const FlarkV3DocumentQueryException(
                  'A non-contiguous recursive-Green row carried a nonzero '
                  'editable-source sentinel.',
                ),
      };
      reader.expectEnd('recursive-Green row record');
      final selected = rowFlags & _recursiveGreenRowSelectedFlag != 0;
      final inlineCapable = rowFlags & _recursiveGreenRowInlineCapableFlag != 0;
      if (rowFlags & ~_recursiveGreenRowFlagMask != 0 ||
          frameId == BigInt.zero ||
          globalOrdinal != startGlobalRowOrdinal + BigInt.from(index) ||
          rowPathCount == 0 ||
          pathStart + rowPathCount > pathCount ||
          (editableSource != null &&
              !_containsSpan(physicalSource, editableSource)) ||
          (editCapability !=
                  FlarkV3RecursiveGreenRowEditCapability.contiguous &&
              inlineCapable) ||
          selected != (selectedRowIndex == index) ||
          !_recursiveGreenPresentationMatchesKind(presentationKind, kind)) {
        throw const FlarkV3DocumentQueryException(
          'A recursive-Green row record is not canonical.',
        );
      }
      rawRows.add(
        _DecodedRecursiveGreenRow(
          globalOrdinal: globalOrdinal,
          frameId: frameId,
          kind: kind,
          selected: selected,
          inlineCapable: inlineCapable,
          literal: rowFlags & _recursiveGreenRowLiteralFlag != 0,
          pathStart: pathStart,
          pathCount: rowPathCount,
          presentationKind: presentationKind,
          editCapability: editCapability,
          physicalSource: physicalSource,
          editableSource: editableSource,
        ),
      );
    }

    final pathRecords = <FlarkV3RecursiveGreenRowPathFrame>[];
    final pathBase =
        _recursiveGreenRowRangeHeaderBytes +
        rowCount * _recursiveGreenRowRecordBytes;
    for (var index = 0; index < pathCount; index += 1) {
      final offset = pathBase + index * _recursiveGreenPathRecordBytes;
      final reader = _M11Reader(
        Uint8List.sublistView(
          bytes,
          offset,
          offset + _recursiveGreenPathRecordBytes,
        ),
      );
      final frameId = reader.u64('recursive-Green path frame');
      final kind = _recursiveGreenKind(reader.u16('recursive-Green path kind'));
      final pathFlags = reader.u16('recursive-Green path flags');
      final factKind = reader.u16('recursive-Green normalized fact kind');
      final reserved = reader.u16('recursive-Green path reserved field');
      final physicalSource = _recursiveGreenMetricSpan(
        reader,
        'recursive-Green path physical source',
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
      );
      final arguments = <int>[
        reader.u32('recursive-Green path argument 0'),
        reader.u32('recursive-Green path argument 1'),
        reader.u32('recursive-Green path argument 2'),
        reader.u32('recursive-Green path argument 3'),
      ];
      reader.expectEnd('recursive-Green path record');
      final hasOpenFact = pathFlags & _recursiveGreenPathOpenFactFlag != 0;
      final hasCloseFact = pathFlags & _recursiveGreenPathCloseFactFlag != 0;
      final fact = _decodeRecursiveGreenPathFact(
        factKind: factKind,
        kind: kind,
        arguments: arguments,
      );
      if (reserved != 0 ||
          pathFlags & ~_recursiveGreenPathFlagMask != 0 ||
          frameId == BigInt.zero ||
          (fact == null) != (!hasOpenFact && !hasCloseFact)) {
        throw const FlarkV3DocumentQueryException(
          'A recursive-Green path record is not canonical.',
        );
      }
      pathRecords.add(
        FlarkV3RecursiveGreenRowPathFrame(
          frameId: frameId,
          kind: kind,
          physicalSource: physicalSource,
          isRowOwner: pathFlags & _recursiveGreenPathOwnerFlag != 0,
          isContainer: pathFlags & _recursiveGreenPathContainerFlag != 0,
          hasOpenFact: hasOpenFact,
          hasCloseFact: hasCloseFact,
          fact: fact,
        ),
      );
    }

    final rows = <FlarkV3RecursiveGreenRenderableRow>[];
    FlarkV3SourceSpan? priorPhysical;
    for (final raw in rawRows) {
      final path = pathRecords.sublist(
        raw.pathStart,
        raw.pathStart + raw.pathCount,
      );
      final ownerCount = path.where((frame) => frame.isRowOwner).length;
      final owner = path.last;
      final frames = <BigInt>{};
      if (ownerCount != 1 ||
          !owner.isRowOwner ||
          owner.frameId != raw.frameId ||
          owner.kind != raw.kind ||
          path.any(
            (frame) =>
                !frames.add(frame.frameId) ||
                !_containsSpan(frame.physicalSource, raw.physicalSource),
          ) ||
          priorPhysical != null &&
              (raw.physicalSource.startUtf8 < priorPhysical.endUtf8 ||
                  raw.physicalSource.startUtf16 < priorPhysical.endUtf16)) {
        throw const FlarkV3DocumentQueryException(
          'A recursive-Green row path does not bind its exact owner.',
        );
      }
      rows.add(
        FlarkV3RecursiveGreenRenderableRow(
          globalOrdinal: raw.globalOrdinal,
          frameId: raw.frameId,
          kind: raw.kind,
          selected: raw.selected,
          inlineCapable: raw.inlineCapable,
          literal: raw.literal,
          presentationKind: raw.presentationKind,
          editCapability: raw.editCapability,
          physicalSource: raw.physicalSource,
          editableSource: raw.editableSource,
          path: path,
        ),
      );
      priorPhysical = raw.physicalSource;
    }

    final covered = metricRange(range.coveredRange);
    final requested = metricRange(range.requestedRange);
    final expectedCoverage = rows.isEmpty
        ? requested
        : FlarkV3SourceSpan(
            startUtf8: requested.startUtf8 < rows.first.physicalSource.startUtf8
                ? requested.startUtf8
                : rows.first.physicalSource.startUtf8,
            endUtf8: requested.endUtf8 > rows.last.physicalSource.endUtf8
                ? requested.endUtf8
                : rows.last.physicalSource.endUtf8,
            startUtf16:
                requested.startUtf16 < rows.first.physicalSource.startUtf16
                ? requested.startUtf16
                : rows.first.physicalSource.startUtf16,
            endUtf16: requested.endUtf16 > rows.last.physicalSource.endUtf16
                ? requested.endUtf16
                : rows.last.physicalSource.endUtf16,
          );
    if (!_sameSpan(covered, expectedCoverage)) {
      throw const FlarkV3DocumentQueryException(
        'Recursive-Green rows disagree with their minimal covered range.',
      );
    }
    return FlarkV3DecodedDocumentBlockRange.recursiveGreen(
      coveredSource: covered,
      startGlobalRowOrdinal: startGlobalRowOrdinal,
      totalGlobalRowCount: totalGlobalRowCount,
      selectedRowIndex: selectedRowIndex,
      rows: rows,
    );
  }

  static FlarkV3SourceSpan metricRange(FlarkV3MetricRange range) =>
      FlarkV3SourceSpan(
        startUtf8: range.start.bytes,
        endUtf8: range.end.bytes,
        startUtf16: range.start.utf16,
        endUtf16: range.end.utf16,
      );
}

FlarkV3InlineFacts _decodeInlineFacts(
  Uint8List bytes, {
  required FlarkV3SourceDocument sourceDocument,
  required FlarkV3SourceVersion expectedSource,
  required int expectedProfilePartition,
  required FlarkV3SourceSpan? expectedLeaf,
  required FlarkV3DocumentStructure structure,
}) {
  final reader = _M11Reader(bytes);
  reader.expectMagic(_inlineMagic, 'inline');
  reader.expectU32(_inlineSchema, 'inline schema');
  final disposition = switch (reader.variant('inline disposition')) {
    1 => FlarkV3InlineFactsDisposition.authoritative,
    2 => FlarkV3InlineFactsDisposition.unsupported,
    _ => throw const FlarkV3DocumentQueryException(
      'The host returned an unknown inline disposition.',
    ),
  };
  final profilePartition = reader.u32('inline profile partition');
  final factCount = reader.u32('inline fact count');
  final factLeaf = reader.span(
    'inline leaf',
    sourceBytes: expectedSource.metric.bytes,
    utf8ToUtf16: sourceDocument.utf8ToUtf16,
  );
  reader.expectU32(
    FlarkV3InlineFactsDecoder.recordBytes,
    'inline fact record size',
  );
  reader.expectU32(0, 'inline reserved field');
  final inlineContentSource = structure.inlineContentSource;
  final inlineLeaf =
      structure.canCarryInlineFacts &&
      inlineContentSource != null &&
      expectedLeaf != null &&
      _sameSpan(inlineContentSource, expectedLeaf) &&
      _containsSpan(expectedLeaf, factLeaf) &&
      factLeaf.startUtf8 < factLeaf.endUtf8;
  final selectedListItem =
      (structure.kind == FlarkV3DocumentStructureKind.bulletList ||
          structure.kind == FlarkV3DocumentStructureKind.orderedList) &&
      expectedLeaf == null &&
      factLeaf.startUtf8 < factLeaf.endUtf8 &&
      factLeaf.startUtf8 >= structure.source.startUtf8 &&
      factLeaf.endUtf8 <= structure.source.endUtf8 &&
      factLeaf.startUtf16 >= structure.source.startUtf16 &&
      factLeaf.endUtf16 <= structure.source.endUtf16 &&
      factLeaf.endUtf8 - factLeaf.startUtf8 <=
          FlarkV3InlineFacts.maximumWholeLeafSourceBytes;
  if (!inlineLeaf && !selectedListItem) {
    throw const FlarkV3DocumentQueryException(
      'Inline facts do not bind an exact supported inline projection.',
    );
  }
  final factBytes = factCount * FlarkV3InlineFactsDecoder.recordBytes;
  final requiredBytes = _inlineHeaderBytes + factBytes;
  if (bytes.length < requiredBytes) {
    throw const FlarkV3DocumentQueryException(
      'Inline metadata does not match its canonical fact bytes.',
    );
  }
  final encodedFacts = Uint8List.sublistView(
    bytes,
    _inlineHeaderBytes,
    requiredBytes,
  );
  final encodedValues = Uint8List.sublistView(bytes, requiredBytes);
  final inlineValues = encodedValues.isEmpty
      ? null
      : FlarkV3InlineValuesPayload(
          sourceVersion: expectedSource,
          profilePartition: profilePartition,
          source: factLeaf,
          encodedBytes: encodedValues,
        );
  try {
    return FlarkV3InlineFactsDecoder.decode(
      sourceDocument: sourceDocument,
      expectedSource: expectedSource,
      factSource: expectedSource,
      expectedProfilePartition: expectedProfilePartition,
      profilePartition: profilePartition,
      // The enclosing structural projection authorizes where an inline leaf
      // may live. The parser-authored fact leaf is the exact source authority
      // for the inline projection itself (for example, a Paragraph excludes
      // its terminal line ending).
      expectedLeaf: factLeaf,
      factLeaf: factLeaf,
      disposition: disposition,
      factCount: factCount,
      encodedFacts: encodedFacts,
      inlineValues: inlineValues,
    );
  } on FlarkV3InlineFactsDecodeException catch (error) {
    throw FlarkV3DocumentQueryException(error.message);
  }
}

({FlarkV3DocumentStructure structure, FlarkV3DocumentProjection projection})
_decodeStructuralPair(
  Uint8List greenBytes,
  Uint8List projectionBytes, {
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
}) {
  final decodedGreen = _decodeGreen(
    greenBytes,
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  final decodedProjection = _decodeProjection(
    projectionBytes,
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  final green = decodedGreen.structure;
  final projection = decodedProjection.projection;
  if (decodedGreen.wireVariant != decodedProjection.wireVariant ||
      green.kind != projection.kind ||
      !_sameSpan(green.source, projection.source) ||
      (switch (green.kind) {
        FlarkV3DocumentStructureKind.paragraph ||
        FlarkV3DocumentStructureKind.fencedCode ||
        FlarkV3DocumentStructureKind.heading ||
        FlarkV3DocumentStructureKind.thematicBreak ||
        FlarkV3DocumentStructureKind.indentedCode ||
        FlarkV3DocumentStructureKind.blockQuote ||
        FlarkV3DocumentStructureKind.bulletList ||
        FlarkV3DocumentStructureKind.orderedList => !_sameSpan(
          green.visibleSource,
          projection.projectedSource,
        ),
        _ => false,
      }) ||
      (green.indentedCode != null &&
          projection.runCount != green.indentedCode!.lineCount) ||
      (green.blockQuote != null &&
          projection.runCount != green.blockQuote!.lineCount) ||
      (green.bulletList != null &&
          projection.runCount != green.bulletList!.itemCount) ||
      (green.orderedList != null &&
          projection.runCount != green.orderedList!.itemCount)) {
    throw const FlarkV3DocumentQueryException(
      'Green and projection records do not describe one atomic root.',
    );
  }
  return (structure: green, projection: projection);
}

({FlarkV3DocumentStructure structure, int wireVariant}) _decodeGreen(
  Uint8List bytes, {
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
}) {
  final reader = _M11Reader(bytes);
  reader.expectMagic(_greenMagic, 'Green');
  reader.expectU32(_roleSchema, 'Green schema');
  final wireVariant = reader.variant('Green');
  final kind = _kind(wireVariant);
  final source = reader.span(
    'Green source',
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  final visible = reader.span(
    'Green visible source',
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  if (kind == FlarkV3DocumentStructureKind.fencedCode) {
    final metadata = reader.u64AsU32('fenced code metadata');
    final openingStart = reader.u32('opening fence start');
    final openingEnd = reader.u32('opening fence end');
    final infoStart = reader.u32('raw info start');
    final infoEnd = reader.u32('raw info end');
    final closingStart = reader.u32('closing fence start');
    final closingEnd = reader.u32('closing fence end');
    reader.expectEnd('Green');

    final marker = switch (metadata & 0xff) {
      0x60 => FlarkV3CodeFenceMarker.backtick,
      0x7e => FlarkV3CodeFenceMarker.tilde,
      _ => throw const FlarkV3DocumentQueryException(
        'A fenced-code Green record carried an invalid marker.',
      ),
    };
    final openingIndent = (metadata >> 8) & 0xff;
    final closed = metadata & _fenceClosedFlag != 0;
    if (metadata & ~_fenceMetadataMask != 0 || openingIndent > 3) {
      throw const FlarkV3DocumentQueryException(
        'A fenced-code Green record carried invalid metadata.',
      );
    }
    final openingMarker = _absoluteSpan(
      openingStart,
      openingEnd,
      name: 'opening fence',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final rawInfo = _absoluteSpan(
      infoStart,
      infoEnd,
      name: 'raw fence info',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final closingMarker =
        closingStart == _fenceAbsentCut && closingEnd == _fenceAbsentCut
        ? null
        : _absoluteSpan(
            closingStart,
            closingEnd,
            name: 'closing fence',
            sourceBytes: sourceBytes,
            utf8ToUtf16: utf8ToUtf16,
          );
    if (openingMarker.startUtf8 < source.startUtf8 ||
        openingMarker.endUtf8 - openingMarker.startUtf8 < 3 ||
        rawInfo.startUtf8 != openingMarker.endUtf8 ||
        rawInfo.endUtf8 > source.endUtf8 ||
        rawInfo.endUtf8 > visible.startUtf8 ||
        visible.startUtf8 < source.startUtf8 ||
        visible.endUtf8 > source.endUtf8 ||
        visible.startUtf8 - rawInfo.endUtf8 > 2 ||
        closed != (closingMarker != null) ||
        (closingMarker == null
            ? visible.endUtf8 != source.endUtf8
            : closingMarker.startUtf8 < source.startUtf8 ||
                  visible.endUtf8 > closingMarker.startUtf8 ||
                  closingMarker.startUtf8 - visible.endUtf8 > 3 ||
                  closingMarker.endUtf8 - closingMarker.startUtf8 <
                      openingMarker.endUtf8 - openingMarker.startUtf8 ||
                  closingMarker.endUtf8 > source.endUtf8)) {
      throw const FlarkV3DocumentQueryException(
        'A fenced-code Green record carried inconsistent source geometry.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        fencedCode: FlarkV3FencedCodeFacts(
          marker: marker,
          openingIndent: openingIndent,
          openingMarker: openingMarker,
          rawInfoSource: rawInfo,
          bodySource: visible,
          closingMarker: closingMarker,
        ),
      ),
    );
  }
  if (wireVariant == _atxHeadingVariant) {
    final metadata = reader.u64AsU32('ATX heading metadata');
    final openingStart = reader.u32('ATX opening marker start');
    final openingEnd = reader.u32('ATX opening marker end');
    final closingStart = reader.u32('ATX closing marker start');
    final closingEnd = reader.u32('ATX closing marker end');
    final lineEndingStart = reader.u32('ATX line ending start');
    final lineEndingEnd = reader.u32('ATX line ending end');
    reader.expectEnd('Green');

    final level = metadata & 0xff;
    final hasClosingMarker = metadata & _atxHasClosingMarkerFlag != 0;
    final openingIndent = (metadata >> _atxOpeningIndentShift) & 0x3;
    final hasBofBom = metadata & _atxHasBofBomFlag != 0;
    if (metadata & ~_atxMetadataMask != 0 || level < 1 || level > 6) {
      throw const FlarkV3DocumentQueryException(
        'An ATX-heading Green record carried invalid metadata.',
      );
    }
    final openingMarker = _absoluteSpan(
      openingStart,
      openingEnd,
      name: 'ATX opening marker',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final closingMarker =
        closingStart == _structuredAbsentCut &&
            closingEnd == _structuredAbsentCut
        ? null
        : _absoluteSpan(
            closingStart,
            closingEnd,
            name: 'ATX closing marker',
            sourceBytes: sourceBytes,
            utf8ToUtf16: utf8ToUtf16,
          );
    final lineEnding = _absoluteSpan(
      lineEndingStart,
      lineEndingEnd,
      name: 'ATX line ending',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    if (openingMarker.startUtf8 !=
            source.startUtf8 + openingIndent + (hasBofBom ? 3 : 0) ||
        (hasBofBom && source.startUtf8 != 0) ||
        openingMarker.endUtf8 - openingMarker.startUtf8 != level ||
        openingMarker.endUtf8 > visible.startUtf8 ||
        visible.startUtf8 < source.startUtf8 ||
        visible.endUtf8 > lineEnding.startUtf8 ||
        lineEnding.endUtf8 != source.endUtf8 ||
        lineEnding.endUtf8 - lineEnding.startUtf8 > 2 ||
        hasClosingMarker != (closingMarker != null) ||
        (closingMarker != null &&
            (closingMarker.startUtf8 < visible.endUtf8 ||
                closingMarker.startUtf8 == closingMarker.endUtf8 ||
                closingMarker.endUtf8 > lineEnding.startUtf8))) {
      throw const FlarkV3DocumentQueryException(
        'An ATX-heading Green record carried inconsistent source geometry.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        heading: FlarkV3AtxHeadingFacts(
          level: level,
          openingMarker: openingMarker,
          contentSource: visible,
          closingMarker: closingMarker,
        ),
      ),
    );
  }
  if (wireVariant == _setextHeadingVariant) {
    final metadata = reader.u64AsU32('Setext heading metadata');
    final underlineStart = reader.u32('Setext underline marker start');
    final underlineEnd = reader.u32('Setext underline marker end');
    final lineEndingStart = reader.u32('Setext underline line ending start');
    final lineEndingEnd = reader.u32('Setext underline line ending end');
    final definitions = reader.u64AsU32('reference definition count');
    reader.expectEnd('Green');

    final level = metadata & 0xff;
    final openingIndent =
        (metadata >> _setextOpeningIndentShift) & _setextOpeningIndentMask;
    if (metadata & ~_setextMetadataMask != 0 || level < 1 || level > 2) {
      throw const FlarkV3DocumentQueryException(
        'A Setext-heading Green record carried invalid metadata.',
      );
    }
    final underlineMarker = _absoluteSpan(
      underlineStart,
      underlineEnd,
      name: 'Setext underline marker',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final underlineLineEnding = _absoluteSpan(
      lineEndingStart,
      lineEndingEnd,
      name: 'Setext underline line ending',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    if (underlineMarker.startUtf8 < openingIndent) {
      throw const FlarkV3DocumentQueryException(
        'A Setext-heading Green record carried inconsistent source geometry.',
      );
    }
    final contentLineEndingEnd = underlineMarker.startUtf8 - openingIndent;
    final contentLineEnding = _absoluteSpan(
      visible.endUtf8,
      contentLineEndingEnd,
      name: 'Setext content line ending',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    if (visible.startUtf8 < source.startUtf8 ||
        visible.startUtf8 >= visible.endUtf8 ||
        contentLineEnding.endUtf8 + openingIndent !=
            underlineMarker.startUtf8 ||
        contentLineEnding.endUtf8 - contentLineEnding.startUtf8 < 1 ||
        contentLineEnding.endUtf8 - contentLineEnding.startUtf8 > 2 ||
        underlineMarker.startUtf8 >= underlineMarker.endUtf8 ||
        underlineMarker.endUtf8 > underlineLineEnding.startUtf8 ||
        underlineLineEnding.startUtf8 > underlineLineEnding.endUtf8 ||
        underlineLineEnding.endUtf8 != source.endUtf8 ||
        underlineLineEnding.endUtf8 - underlineLineEnding.startUtf8 > 2) {
      throw const FlarkV3DocumentQueryException(
        'A Setext-heading Green record carried inconsistent source geometry.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: definitions,
        heading: FlarkV3SetextHeadingFacts(
          level: level,
          contentSource: visible,
          openingIndent: openingIndent,
          contentLineEnding: contentLineEnding,
          underlineMarker: underlineMarker,
          underlineLineEnding: underlineLineEnding,
        ),
      ),
    );
  }
  if (wireVariant == _thematicBreakVariant) {
    final metadata = reader.u64AsU32('thematic-break metadata');
    final markerStart = reader.u32('thematic-break marker envelope start');
    final markerEnd = reader.u32('thematic-break marker envelope end');
    final lineEndingStart = reader.u32('thematic-break line ending start');
    final lineEndingEnd = reader.u32('thematic-break line ending end');
    final markerCount = reader.u64AsU32('thematic-break marker count');
    reader.expectEnd('Green');

    final marker = switch (metadata & 0xff) {
      0x2a => FlarkV3ThematicBreakMarker.asterisk,
      0x2d => FlarkV3ThematicBreakMarker.hyphen,
      0x5f => FlarkV3ThematicBreakMarker.underscore,
      _ => throw const FlarkV3DocumentQueryException(
        'A thematic-break Green record carried an invalid marker.',
      ),
    };
    final openingIndent = (metadata >> _thematicBreakOpeningIndentShift) & 0x3;
    final hasBofBom = metadata & _thematicBreakHasBofBomFlag != 0;
    if (metadata & ~_thematicBreakMetadataMask != 0) {
      throw const FlarkV3DocumentQueryException(
        'A thematic-break Green record carried invalid metadata.',
      );
    }
    final markerEnvelope = _absoluteSpan(
      markerStart,
      markerEnd,
      name: 'thematic-break marker envelope',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    final lineEnding = _absoluteSpan(
      lineEndingStart,
      lineEndingEnd,
      name: 'thematic-break line ending',
      sourceBytes: sourceBytes,
      utf8ToUtf16: utf8ToUtf16,
    );
    if (visible.startUtf8 != source.startUtf8 ||
        visible.endUtf8 != source.startUtf8 ||
        markerEnvelope.startUtf8 !=
            source.startUtf8 + openingIndent + (hasBofBom ? 3 : 0) ||
        hasBofBom && source.startUtf8 != 0 ||
        markerEnvelope.startUtf8 >= markerEnvelope.endUtf8 ||
        markerEnvelope.endUtf8 > lineEnding.startUtf8 ||
        markerCount < 3 ||
        markerCount > markerEnvelope.endUtf8 - markerEnvelope.startUtf8 ||
        lineEnding.startUtf8 > lineEnding.endUtf8 ||
        lineEnding.endUtf8 != source.endUtf8 ||
        lineEnding.endUtf8 - lineEnding.startUtf8 > 2) {
      throw const FlarkV3DocumentQueryException(
        'A thematic-break Green record carried inconsistent source geometry.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        thematicBreak: FlarkV3ThematicBreakFacts(
          marker: marker,
          markerCount: markerCount,
          openingIndent: openingIndent,
          hasBofBom: hasBofBom,
          markerEnvelope: markerEnvelope,
          lineEnding: lineEnding,
        ),
      ),
    );
  }
  if (wireVariant == _indentedCodeVariant) {
    final metadata = reader.u64AsU32('indented-code metadata');
    final lineCount = reader.u32('indented-code line count');
    final projectedUtf8Length = reader.u32(
      'indented-code projected UTF-8 length',
    );
    final projectedUtf16Length = reader.u32(
      'indented-code projected UTF-16 length',
    );
    final terminalLineEndingBytes = reader.u32(
      'indented-code terminal line ending width',
    );
    final reserved = reader.u64AsU32('indented-code reserved field');
    reader.expectEnd('Green');

    final deindentColumns = metadata & 0xff;
    final hasBofBom = metadata & _indentedCodeHasBofBomFlag != 0;
    if (metadata & ~_indentedCodeMetadataMask != 0 ||
        deindentColumns != 4 ||
        lineCount == 0 ||
        projectedUtf8Length > source.endUtf8 - source.startUtf8 ||
        projectedUtf16Length > source.endUtf16 - source.startUtf16 ||
        terminalLineEndingBytes > 2 ||
        reserved != 0 ||
        hasBofBom && source.startUtf8 != 0 ||
        visible.startUtf8 != source.startUtf8 ||
        visible.endUtf8 != source.startUtf8) {
      throw const FlarkV3DocumentQueryException(
        'An indented-code Green record carried inconsistent projection facts.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        indentedCode: FlarkV3IndentedCodeFacts(
          deindentColumns: deindentColumns,
          hasBofBom: hasBofBom,
          lineCount: lineCount,
          projectedUtf8Length: projectedUtf8Length,
          projectedUtf16Length: projectedUtf16Length,
          terminalLineEndingBytes: terminalLineEndingBytes,
        ),
      ),
    );
  }
  if (wireVariant == _blockQuoteVariant) {
    final disposition = reader.u64AsU32('block-quote disposition');
    final lineCount = reader.u32('block-quote line count');
    final childFirstLine = reader.u32('block-quote child first line');
    final childLineCount = reader.u32('block-quote child line count');
    final projectedUtf8Length = reader.u32(
      'block-quote projected UTF-8 length',
    );
    final projectedUtf16Length = reader.u32(
      'block-quote projected UTF-16 length',
    );
    final reserved = reader.u32('block-quote reserved field');
    reader.expectEnd('Green');

    final sourceUtf8Length = source.endUtf8 - source.startUtf8;
    final sourceUtf16Length = source.endUtf16 - source.startUtf16;
    if (disposition != _blockQuoteExactSingleParagraphDisposition ||
        lineCount == 0 ||
        childFirstLine != 0 ||
        childLineCount != lineCount ||
        projectedUtf8Length == 0 ||
        projectedUtf8Length > sourceUtf8Length ||
        projectedUtf16Length == 0 ||
        projectedUtf16Length > sourceUtf16Length ||
        reserved != 0 ||
        sourceUtf8Length == 0 ||
        visible.startUtf8 != source.startUtf8 ||
        visible.endUtf8 != source.startUtf8) {
      throw const FlarkV3DocumentQueryException(
        'A block-quote Green record carried inconsistent path facts.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        blockQuote: FlarkV3BlockQuoteFacts(
          lineCount: lineCount,
          childFirstLine: childFirstLine,
          childLineCount: childLineCount,
          projectedUtf8Length: projectedUtf8Length,
          projectedUtf16Length: projectedUtf16Length,
        ),
      ),
    );
  }
  if (wireVariant == _bulletListVariant) {
    final metadata = reader.u64AsU32('bullet-list metadata');
    final itemCount = reader.u32('bullet-list item count');
    final encodedTerminalEmptyStart = reader.u32(
      'bullet-list terminal-empty relative start',
    );
    final paragraphCount = reader.u32('bullet-list paragraph count');
    final projectedUtf8Length = reader.u32(
      'bullet-list projected UTF-8 length',
    );
    final projectedUtf16Length = reader.u32(
      'bullet-list projected UTF-16 length',
    );
    final reserved = reader.u32('bullet-list reserved field');
    reader.expectEnd('Green');

    final disposition = metadata & 0xff;
    final marker = switch ((metadata >> _bulletListMarkerShift) & 0xff) {
      0x2d => FlarkV3BulletListMarker.hyphen,
      0x2b => FlarkV3BulletListMarker.plus,
      0x2a => FlarkV3BulletListMarker.asterisk,
      _ => throw const FlarkV3DocumentQueryException(
        'A bullet-list Green record carried an invalid marker.',
      ),
    };
    final tight = metadata & _bulletListTightFlag != 0;
    final terminalEmptyRelativeStartUtf8 =
        encodedTerminalEmptyStart == _bulletListAbsentTerminalEmpty
        ? null
        : encodedTerminalEmptyStart;
    final sourceUtf8Length = source.endUtf8 - source.startUtf8;
    final sourceUtf16Length = source.endUtf16 - source.startUtf16;
    final terminalShapeIsValid = terminalEmptyRelativeStartUtf8 == null
        ? paragraphCount == itemCount
        : paragraphCount + 1 == itemCount &&
              terminalEmptyRelativeStartUtf8 < sourceUtf8Length;
    if (metadata & ~_bulletListMetadataMask != 0 ||
        disposition != _bulletListExactDisposition ||
        !tight ||
        itemCount == 0 ||
        !terminalShapeIsValid ||
        projectedUtf8Length > sourceUtf8Length ||
        projectedUtf16Length > sourceUtf16Length ||
        (projectedUtf8Length == 0) != (projectedUtf16Length == 0) ||
        (projectedUtf8Length == 0 &&
            (itemCount != 1 ||
                paragraphCount != 0 ||
                terminalEmptyRelativeStartUtf8 != 0)) ||
        reserved != 0 ||
        sourceUtf8Length == 0 ||
        visible.startUtf8 != source.startUtf8 ||
        visible.endUtf8 != source.startUtf8) {
      throw const FlarkV3DocumentQueryException(
        'A bullet-list Green record carried inconsistent projection facts.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        bulletList: FlarkV3BulletListFacts(
          marker: marker,
          itemCount: itemCount,
          terminalEmptyRelativeStartUtf8: terminalEmptyRelativeStartUtf8,
          paragraphCount: paragraphCount,
          projectedUtf8Length: projectedUtf8Length,
          projectedUtf16Length: projectedUtf16Length,
        ),
      ),
    );
  }
  if (wireVariant == _orderedListVariant) {
    final metadata = reader.u64AsU32('ordered-list metadata');
    final itemCount = reader.u32('ordered-list item count');
    final encodedTerminalEmptyStart = reader.u32(
      'ordered-list terminal-empty relative start',
    );
    final paragraphCount = reader.u32('ordered-list paragraph count');
    final projectedUtf8Length = reader.u32(
      'ordered-list projected UTF-8 length',
    );
    final projectedUtf16Length = reader.u32(
      'ordered-list projected UTF-16 length',
    );
    final start = reader.u32('ordered-list start');
    reader.expectEnd('Green');

    final disposition = metadata & 0xff;
    final delimiter = switch ((metadata >> _orderedListDelimiterShift) & 0xff) {
      0x2e => FlarkV3OrderedListDelimiter.period,
      0x29 => FlarkV3OrderedListDelimiter.parenthesis,
      _ => throw const FlarkV3DocumentQueryException(
        'An ordered-list Green record carried an invalid delimiter.',
      ),
    };
    final tight = metadata & _orderedListTightFlag != 0;
    final terminalEmptyRelativeStartUtf8 =
        encodedTerminalEmptyStart == _orderedListAbsentTerminalEmpty
        ? null
        : encodedTerminalEmptyStart;
    final sourceUtf8Length = source.endUtf8 - source.startUtf8;
    final sourceUtf16Length = source.endUtf16 - source.startUtf16;
    final terminalShapeIsValid = terminalEmptyRelativeStartUtf8 == null
        ? paragraphCount == itemCount
        : paragraphCount + 1 == itemCount &&
              terminalEmptyRelativeStartUtf8 < sourceUtf8Length;
    if (metadata & ~_orderedListMetadataMask != 0 ||
        disposition != _orderedListExactDisposition ||
        !tight ||
        start > 999999999 ||
        itemCount == 0 ||
        !terminalShapeIsValid ||
        projectedUtf8Length > sourceUtf8Length ||
        projectedUtf16Length > sourceUtf16Length ||
        (projectedUtf8Length == 0) != (projectedUtf16Length == 0) ||
        (projectedUtf8Length == 0 &&
            (itemCount != 1 ||
                paragraphCount != 0 ||
                terminalEmptyRelativeStartUtf8 != 0)) ||
        sourceUtf8Length == 0 ||
        visible.startUtf8 != source.startUtf8 ||
        visible.endUtf8 != source.startUtf8) {
      throw const FlarkV3DocumentQueryException(
        'An ordered-list Green record carried inconsistent projection facts.',
      );
    }
    return (
      wireVariant: wireVariant,
      structure: FlarkV3DocumentStructure(
        kind: kind,
        source: source,
        visibleSource: visible,
        referenceDefinitionCount: 0,
        orderedList: FlarkV3OrderedListFacts(
          start: start,
          delimiter: delimiter,
          itemCount: itemCount,
          terminalEmptyRelativeStartUtf8: terminalEmptyRelativeStartUtf8,
          paragraphCount: paragraphCount,
          projectedUtf8Length: projectedUtf8Length,
          projectedUtf16Length: projectedUtf16Length,
        ),
      ),
    );
  }
  final definitions = reader.u64AsU32('reference definition count');
  final reasonTag = reader.u32('unknown reason');
  final detail0 = reader.u32('unknown reason detail');
  final detail1 = reader.u64AsU32('unknown reason detail');
  final detail2 = reader.u64AsU32('unknown reason detail');
  reader.expectEnd('Green');

  final unknownReason = switch ((kind, reasonTag)) {
    (FlarkV3DocumentStructureKind.unknown, 1)
        when detail0 == 0 && detail1 == 0 && detail2 == 0 =>
      FlarkV3DocumentUnknownReason.blankBoundary,
    (FlarkV3DocumentStructureKind.unknown, 2)
        when detail1 == 0 && detail2 == 0 =>
      _validateUnsupportedOpener(detail0),
    (FlarkV3DocumentStructureKind.unknown, _) =>
      throw const FlarkV3DocumentQueryException(
        'Green carried an unknown M1.1 fallback reason.',
      ),
    (_, 0) when detail0 == 0 && detail1 == 0 && detail2 == 0 => null,
    _ => throw const FlarkV3DocumentQueryException(
      'A supported Green root carried fallback-only details.',
    ),
  };
  if (kind == FlarkV3DocumentStructureKind.unknown && definitions != 0) {
    throw const FlarkV3DocumentQueryException(
      'An Unknown Green root cannot claim reference definitions.',
    );
  }
  return (
    wireVariant: wireVariant,
    structure: FlarkV3DocumentStructure(
      kind: kind,
      source: source,
      visibleSource: visible,
      referenceDefinitionCount: definitions,
      unknownReason: unknownReason,
    ),
  );
}

/// Package-internal decoded range bytes before the runtime wraps continuation.
final class FlarkV3DecodedDocumentBlockRange {
  FlarkV3DecodedDocumentBlockRange({
    required List<FlarkV3DocumentStructuralBlock> blocks,
    required this.coveredSource,
  }) : blocks = List<FlarkV3DocumentStructuralBlock>.unmodifiable(blocks),
       startGlobalRowOrdinal = null,
       totalGlobalRowCount = null,
       selectedRowIndex = null,
       recursiveGreenRows = null;

  FlarkV3DecodedDocumentBlockRange.recursiveGreen({
    required this.coveredSource,
    required BigInt this.startGlobalRowOrdinal,
    required BigInt this.totalGlobalRowCount,
    required this.selectedRowIndex,
    required List<FlarkV3RecursiveGreenRenderableRow> rows,
  }) : blocks = const <FlarkV3DocumentStructuralBlock>[],
       recursiveGreenRows =
           List<FlarkV3RecursiveGreenRenderableRow>.unmodifiable(rows);

  final List<FlarkV3DocumentStructuralBlock> blocks;
  final FlarkV3SourceSpan coveredSource;
  final BigInt? startGlobalRowOrdinal;
  final BigInt? totalGlobalRowCount;
  final int? selectedRowIndex;
  final List<FlarkV3RecursiveGreenRenderableRow>? recursiveGreenRows;
}

final class _DecodedRecursiveGreenRow {
  const _DecodedRecursiveGreenRow({
    required this.globalOrdinal,
    required this.frameId,
    required this.kind,
    required this.selected,
    required this.inlineCapable,
    required this.literal,
    required this.pathStart,
    required this.pathCount,
    required this.presentationKind,
    required this.editCapability,
    required this.physicalSource,
    required this.editableSource,
  });

  final BigInt globalOrdinal;
  final BigInt frameId;
  final FlarkV3RecursiveGreenKind kind;
  final bool selected;
  final bool inlineCapable;
  final bool literal;
  final int pathStart;
  final int pathCount;
  final FlarkV3RecursiveGreenRowPresentationKind presentationKind;
  final FlarkV3RecursiveGreenRowEditCapability editCapability;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan? editableSource;
}

({FlarkV3DocumentProjection projection, int wireVariant}) _decodeProjection(
  Uint8List bytes, {
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
}) {
  final reader = _M11Reader(bytes);
  reader.expectMagic(_projectionMagic, 'Projection');
  reader.expectU32(_roleSchema, 'Projection schema');
  final wireVariant = reader.variant('Projection');
  final kind = _kind(wireVariant);
  final source = reader.span(
    'Projection source',
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  final projected = reader.span(
    'Projection visible source',
    sourceBytes: sourceBytes,
    utf8ToUtf16: utf8ToUtf16,
  );
  final runs = reader.u64AsU32('projection run count');
  reader.expectEnd('Projection');
  final runsAreValid = switch (kind) {
    FlarkV3DocumentStructureKind.empty ||
    FlarkV3DocumentStructureKind.thematicBreak => runs == 0,
    FlarkV3DocumentStructureKind.indentedCode ||
    FlarkV3DocumentStructureKind.blockQuote ||
    FlarkV3DocumentStructureKind.bulletList ||
    FlarkV3DocumentStructureKind.orderedList => runs > 0,
    _ => runs == 1,
  };
  if (!runsAreValid) {
    throw const FlarkV3DocumentQueryException(
      'Projection run count does not match its M1.1 root kind.',
    );
  }
  return (
    wireVariant: wireVariant,
    projection: FlarkV3DocumentProjection(
      kind: kind,
      source: source,
      projectedSource: projected,
      runCount: runs,
    ),
  );
}

FlarkV3DocumentPointPath _decodeBlockQuotePointPathV4(
  Uint8List bytes, {
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
  required FlarkV3DocumentStructure structure,
  required FlarkV3DocumentProjection projection,
}) {
  if (bytes.lengthInBytes !=
      _blockQuotePointPathNodeCount * _documentPointPathV4NodeRecordBytes) {
    throw const FlarkV3DocumentQueryException(
      'Viewport schema 4 requires one exact block-quote point path.',
    );
  }

  final data = ByteData.sublistView(bytes);
  final nodes = <FlarkV3DocumentPointPathNode>[];
  for (var index = 0; index < _blockQuotePointPathNodeCount; index += 1) {
    final offset = index * _documentPointPathV4NodeRecordBytes;
    nodes.add(
      _decodePointPathNode(
        wireKind: data.getUint8(offset),
        allowListKinds: false,
        flags: data.getUint8(offset + 1),
        depth: data.getUint16(offset + 2, Endian.little),
        encodedParent: data.getUint32(offset + 4, Endian.little),
        sourceStartUtf8: data.getUint32(offset + 8, Endian.little),
        sourceEndUtf8: data.getUint32(offset + 12, Endian.little),
        authoredStartUtf16: data.getUint32(offset + 16, Endian.little),
        authoredEndUtf16: data.getUint32(offset + 20, Endian.little),
        firstRun: data.getUint32(offset + 24, Endian.little),
        runCount: data.getUint32(offset + 28, Endian.little),
        projectedUtf8Length: data.getUint32(offset + 32, Endian.little),
        projectedUtf16Length: data.getUint32(offset + 36, Endian.little),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
      ),
    );
  }
  final path = _validatePointPathTopology(nodes);
  final facts = structure.blockQuote;
  final ancestor = path.root;
  final leaf = path.selectedLeaf;
  if (structure.kind != FlarkV3DocumentStructureKind.blockQuote ||
      projection.kind != FlarkV3DocumentStructureKind.blockQuote ||
      facts == null ||
      ancestor.kind != FlarkV3DocumentPointPathNodeKind.blockQuote ||
      ancestor.isNoncontiguous ||
      !_sameSpan(ancestor.source, structure.source) ||
      ancestor.firstRun != 0 ||
      ancestor.runCount != facts.lineCount ||
      ancestor.runCount != projection.runCount ||
      ancestor.projectedUtf8Length != facts.projectedUtf8Length ||
      ancestor.projectedUtf16Length != facts.projectedUtf16Length ||
      leaf.kind != FlarkV3DocumentPointPathNodeKind.paragraph ||
      !leaf.isNoncontiguous ||
      !_sameSpan(leaf.source, structure.source) ||
      leaf.firstRun != facts.childFirstLine ||
      leaf.runCount != facts.childLineCount ||
      leaf.projectedUtf8Length != facts.projectedUtf8Length ||
      leaf.projectedUtf16Length != facts.projectedUtf16Length) {
    throw const FlarkV3DocumentQueryException(
      'The block-quote point path disagrees with its structural summary.',
    );
  }
  return path;
}

FlarkV3DocumentPointPath _decodePointPathV5(
  Uint8List bytes, {
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
  required int nodeCount,
}) {
  if (nodeCount <= 0 ||
      bytes.lengthInBytes != nodeCount * _documentPointPathV5NodeRecordBytes) {
    throw const FlarkV3DocumentQueryException(
      'Viewport schema 5 carries an invalid point-path table.',
    );
  }

  final data = ByteData.sublistView(bytes);
  final nodes = <FlarkV3DocumentPointPathNode>[];
  for (var index = 0; index < nodeCount; index += 1) {
    final offset = index * _documentPointPathV5NodeRecordBytes;
    nodes.add(
      _decodePointPathNode(
        wireKind: data.getUint8(offset),
        allowListKinds: true,
        flags: data.getUint8(offset + 1),
        depth: data.getUint16(offset + 2, Endian.little),
        encodedParent: data.getUint32(offset + 4, Endian.little),
        sourceStartUtf8: data.getUint32(offset + 8, Endian.little),
        sourceEndUtf8: data.getUint32(offset + 12, Endian.little),
        authoredStartUtf16: null,
        authoredEndUtf16: null,
        firstRun: data.getUint32(offset + 16, Endian.little),
        runCount: data.getUint32(offset + 20, Endian.little),
        projectedUtf8Length: data.getUint32(offset + 24, Endian.little),
        projectedUtf16Length: data.getUint32(offset + 28, Endian.little),
        sourceBytes: sourceBytes,
        utf8ToUtf16: utf8ToUtf16,
      ),
    );
  }
  return _validatePointPathTopology(nodes);
}

FlarkV3DocumentPointPathNode _decodePointPathNode({
  required int wireKind,
  required bool allowListKinds,
  required int flags,
  required int depth,
  required int encodedParent,
  required int sourceStartUtf8,
  required int sourceEndUtf8,
  required int? authoredStartUtf16,
  required int? authoredEndUtf16,
  required int firstRun,
  required int runCount,
  required int projectedUtf8Length,
  required int projectedUtf16Length,
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
}) {
  final kind = switch (wireKind) {
    _pointPathBlockQuoteKind => FlarkV3DocumentPointPathNodeKind.blockQuote,
    _pointPathParagraphKind => FlarkV3DocumentPointPathNodeKind.paragraph,
    _pointPathListKind when allowListKinds =>
      FlarkV3DocumentPointPathNodeKind.list,
    _pointPathListItemKind when allowListKinds =>
      FlarkV3DocumentPointPathNodeKind.listItem,
    _ => throw const FlarkV3DocumentQueryException(
      'A point-path node has an unknown structure kind.',
    ),
  };
  if (flags & ~_pointPathFlagMask != 0) {
    throw const FlarkV3DocumentQueryException(
      'A point-path node contains unknown flags.',
    );
  }
  if (sourceStartUtf8 >= sourceEndUtf8 ||
      sourceEndUtf8 > sourceBytes ||
      runCount == 0 ||
      (projectedUtf8Length == 0) != (projectedUtf16Length == 0)) {
    throw const FlarkV3DocumentQueryException(
      'A point-path node carries invalid source or projection geometry.',
    );
  }
  late final int sourceStartUtf16;
  late final int sourceEndUtf16;
  try {
    sourceStartUtf16 = utf8ToUtf16(sourceStartUtf8);
    sourceEndUtf16 = utf8ToUtf16(sourceEndUtf8);
  } on Object {
    throw const FlarkV3DocumentQueryException(
      'A point-path node is not aligned to exact source boundaries.',
    );
  }
  if ((authoredStartUtf16 != null && authoredStartUtf16 != sourceStartUtf16) ||
      (authoredEndUtf16 != null && authoredEndUtf16 != sourceEndUtf16)) {
    throw const FlarkV3DocumentQueryException(
      'A point-path node disagrees across UTF-8 and UTF-16 coordinates.',
    );
  }
  final parentIndex = encodedParent == _pointPathRootParent
      ? null
      : encodedParent;
  return FlarkV3DocumentPointPathNode(
    kind: kind,
    source: FlarkV3SourceSpan(
      startUtf8: sourceStartUtf8,
      endUtf8: sourceEndUtf8,
      startUtf16: sourceStartUtf16,
      endUtf16: sourceEndUtf16,
    ),
    depth: depth,
    parentIndex: parentIndex,
    firstRun: firstRun,
    runCount: runCount,
    projectedUtf8Length: projectedUtf8Length,
    projectedUtf16Length: projectedUtf16Length,
    isNoncontiguous: flags & _pointPathNoncontiguousFlag != 0,
    isSelected: flags & _pointPathSelectedFlag != 0,
  );
}

FlarkV3DocumentPointPath _validatePointPathTopology(
  List<FlarkV3DocumentPointPathNode> nodes,
) {
  if (nodes.isEmpty) {
    throw const FlarkV3DocumentQueryException('A point path cannot be empty.');
  }
  for (var index = 0; index < nodes.length; index += 1) {
    final node = nodes[index];
    final expectedParentIndex = index == 0 ? null : index - 1;
    if (node.depth != index ||
        node.parentIndex != expectedParentIndex ||
        node.isSelected != (index == nodes.length - 1)) {
      throw const FlarkV3DocumentQueryException(
        'Point-path nodes do not form one selected outer-to-inner ancestry.',
      );
    }
    if (index == 0) continue;
    final parent = nodes[index - 1];
    if (!_containsSpan(parent.source, node.source) ||
        !_containsRunSlice(parent, node) ||
        node.projectedUtf8Length > parent.projectedUtf8Length ||
        node.projectedUtf16Length > parent.projectedUtf16Length) {
      throw const FlarkV3DocumentQueryException(
        'A point-path child escapes its parser-authored parent envelope.',
      );
    }
  }
  return FlarkV3DocumentPointPath._(List.unmodifiable(nodes));
}

bool _containsSpan(FlarkV3SourceSpan parent, FlarkV3SourceSpan child) =>
    parent.startUtf8 <= child.startUtf8 &&
    child.endUtf8 <= parent.endUtf8 &&
    parent.startUtf16 <= child.startUtf16 &&
    child.endUtf16 <= parent.endUtf16;

bool _containsRunSlice(
  FlarkV3DocumentPointPathNode parent,
  FlarkV3DocumentPointPathNode child,
) =>
    parent.firstRun <= child.firstRun &&
    child.firstRun + child.runCount <= parent.firstRun + parent.runCount;

FlarkV3DocumentStructureKind _kind(int variant) => switch (variant) {
  0 => FlarkV3DocumentStructureKind.empty,
  1 => FlarkV3DocumentStructureKind.paragraph,
  2 => FlarkV3DocumentStructureKind.unknown,
  3 => FlarkV3DocumentStructureKind.fencedCode,
  _atxHeadingVariant ||
  _setextHeadingVariant => FlarkV3DocumentStructureKind.heading,
  _thematicBreakVariant => FlarkV3DocumentStructureKind.thematicBreak,
  _indentedCodeVariant => FlarkV3DocumentStructureKind.indentedCode,
  _blockQuoteVariant => FlarkV3DocumentStructureKind.blockQuote,
  _bulletListVariant => FlarkV3DocumentStructureKind.bulletList,
  _orderedListVariant => FlarkV3DocumentStructureKind.orderedList,
  _ => throw const FlarkV3DocumentQueryException(
    'The host returned an unknown M1.1 root variant.',
  ),
};

FlarkV3SourceSpan _absoluteSpan(
  int start,
  int end, {
  required String name,
  required int sourceBytes,
  required int Function(int utf8Offset) utf8ToUtf16,
}) {
  if (start > end || end > sourceBytes) {
    throw FlarkV3DocumentQueryException('$name is outside exact source.');
  }
  return FlarkV3SourceSpan(
    startUtf8: start,
    endUtf8: end,
    startUtf16: utf8ToUtf16(start),
    endUtf16: utf8ToUtf16(end),
  );
}

FlarkV3DocumentUnknownReason _validateUnsupportedOpener(int value) =>
    switch (value) {
      >= 1 && <= 9 => FlarkV3DocumentUnknownReason.unsupportedSource,
      _ => throw const FlarkV3DocumentQueryException(
        'The host returned an unknown unsupported block opener.',
      ),
    };

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _sourceSpansOverlap(
  FlarkV3SourceSpan source,
  FlarkV3MetricRange requested,
) =>
    source.startUtf8 < requested.end.bytes &&
    source.startUtf16 < requested.end.utf16 &&
    requested.start.bytes < source.endUtf8 &&
    requested.start.utf16 < source.endUtf16;

FlarkV3RecursiveGreenKind _recursiveGreenKind(int id) {
  for (final kind in FlarkV3RecursiveGreenKind.values) {
    if (kind.id == id) return kind;
  }
  throw const FlarkV3DocumentQueryException(
    'A recursive-Green row uses an unknown final kind.',
  );
}

FlarkV3RecursiveGreenRowPresentationKind _recursiveGreenRowPresentationKind(
  int value,
) => switch (value) {
  1 => FlarkV3RecursiveGreenRowPresentationKind.inline,
  2 => FlarkV3RecursiveGreenRowPresentationKind.fencedCode,
  3 => FlarkV3RecursiveGreenRowPresentationKind.indentedCode,
  4 => FlarkV3RecursiveGreenRowPresentationKind.html,
  5 => FlarkV3RecursiveGreenRowPresentationKind.thematicBreak,
  _ => throw const FlarkV3DocumentQueryException(
    'A recursive-Green row uses an unknown presentation kind.',
  ),
};

FlarkV3RecursiveGreenRowEditCapability _recursiveGreenRowEditCapability(
  int value,
) => switch (value) {
  1 => FlarkV3RecursiveGreenRowEditCapability.contiguous,
  2 => FlarkV3RecursiveGreenRowEditCapability.projectedReserved,
  3 => FlarkV3RecursiveGreenRowEditCapability.unavailable,
  _ => throw const FlarkV3DocumentQueryException(
    'A recursive-Green row uses an unknown edit capability.',
  ),
};

bool _isZeroSpan(FlarkV3SourceSpan span) =>
    span.startUtf8 == 0 &&
    span.endUtf8 == 0 &&
    span.startUtf16 == 0 &&
    span.endUtf16 == 0;

FlarkV3SourceSpan _recursiveGreenMetricSpan(
  _M11Reader reader,
  String name, {
  required FlarkV3SourceDocument sourceDocument,
  required FlarkV3SourceVersion expectedSource,
}) {
  final startUtf8 = reader.u32('$name start UTF-8');
  final startUtf16 = reader.u32('$name start UTF-16');
  final endUtf8 = reader.u32('$name end UTF-8');
  final endUtf16 = reader.u32('$name end UTF-16');
  if (endUtf8 < startUtf8 ||
      endUtf16 < startUtf16 ||
      endUtf8 > expectedSource.metric.bytes ||
      endUtf16 > expectedSource.metric.utf16 ||
      sourceDocument.utf8ToUtf16(startUtf8) != startUtf16 ||
      sourceDocument.utf8ToUtf16(endUtf8) != endUtf16) {
    throw FlarkV3DocumentQueryException('$name is outside exact source.');
  }
  return FlarkV3SourceSpan(
    startUtf8: startUtf8,
    endUtf8: endUtf8,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
  );
}

FlarkV3RecursiveGreenPathFact? _decodeRecursiveGreenPathFact({
  required int factKind,
  required FlarkV3RecursiveGreenKind kind,
  required List<int> arguments,
}) {
  Never invalid() => throw const FlarkV3DocumentQueryException(
    'A recursive-Green normalized path fact is invalid.',
  );
  final a0 = arguments[0];
  final a1 = arguments[1];
  final a2 = arguments[2];
  final a3 = arguments[3];
  switch (factKind) {
    case 0:
      if (arguments.any((value) => value != 0)) invalid();
      return null;
    case 1:
      if (kind != FlarkV3RecursiveGreenKind.list || a3 > 1) invalid();
      final style = switch (a0) {
        1 => FlarkV3RecursiveGreenListStyle.bullet,
        2 => FlarkV3RecursiveGreenListStyle.ordered,
        _ => invalid(),
      };
      if (style == FlarkV3RecursiveGreenListStyle.bullet) {
        final marker = switch (a1) {
          0x2d => FlarkV3BulletListMarker.hyphen,
          0x2b => FlarkV3BulletListMarker.plus,
          0x2a => FlarkV3BulletListMarker.asterisk,
          _ => invalid(),
        };
        return FlarkV3RecursiveGreenListPathFact(
          style: style,
          bulletMarker: marker,
          start: a2,
          tight: a3 == 1,
        );
      }
      final delimiter = switch (a1) {
        0x2e => FlarkV3OrderedListDelimiter.period,
        0x29 => FlarkV3OrderedListDelimiter.parenthesis,
        _ => invalid(),
      };
      if (a2 > 999999999) invalid();
      return FlarkV3RecursiveGreenListPathFact(
        style: style,
        orderedDelimiter: delimiter,
        start: a2,
        tight: a3 == 1,
      );
    case 2:
      if (kind != FlarkV3RecursiveGreenKind.item || a2 != 0 || a3 != 0) {
        invalid();
      }
      return FlarkV3RecursiveGreenItemPathFact(markerOffset: a0, padding: a1);
    case 3:
      if (kind != FlarkV3RecursiveGreenKind.heading ||
          a0 < 1 ||
          a0 > 6 ||
          a2 != 0 ||
          a3 != 0) {
        invalid();
      }
      final style = switch (a1) {
        0 => FlarkV3RecursiveGreenHeadingStyle.atx,
        1 => FlarkV3RecursiveGreenHeadingStyle.setext,
        _ => invalid(),
      };
      return FlarkV3RecursiveGreenHeadingPathFact(level: a0, style: style);
    case 4:
      if (kind != FlarkV3RecursiveGreenKind.fencedCode || a1 > 3) invalid();
      final marker = switch (a0) {
        0x60 => FlarkV3CodeFenceMarker.backtick,
        0x7e => FlarkV3CodeFenceMarker.tilde,
        _ => invalid(),
      };
      return FlarkV3RecursiveGreenCodePathFact(
        marker: marker,
        fenceOffsetColumns: a1,
        minimumClosingLength: BigInt.from(a2) | (BigInt.from(a3) << 32),
      );
    case 5:
      if (kind != FlarkV3RecursiveGreenKind.htmlBlock ||
          a0 == 0 ||
          a1 != 0 ||
          a2 != 0 ||
          a3 != 0) {
        invalid();
      }
      return FlarkV3RecursiveGreenHtmlPathFact(blockType: a0);
    default:
      invalid();
  }
}

bool _recursiveGreenPresentationMatchesKind(
  FlarkV3RecursiveGreenRowPresentationKind presentation,
  FlarkV3RecursiveGreenKind kind,
) => switch (presentation) {
  FlarkV3RecursiveGreenRowPresentationKind.inline =>
    kind == FlarkV3RecursiveGreenKind.paragraph ||
        kind == FlarkV3RecursiveGreenKind.heading,
  FlarkV3RecursiveGreenRowPresentationKind.fencedCode =>
    kind == FlarkV3RecursiveGreenKind.fencedCode,
  FlarkV3RecursiveGreenRowPresentationKind.indentedCode =>
    kind == FlarkV3RecursiveGreenKind.indentedCode,
  FlarkV3RecursiveGreenRowPresentationKind.html =>
    kind == FlarkV3RecursiveGreenKind.htmlBlock,
  FlarkV3RecursiveGreenRowPresentationKind.thematicBreak =>
    kind == FlarkV3RecursiveGreenKind.thematicBreak,
};

final class _M11Reader {
  _M11Reader(this.bytes) : data = ByteData.sublistView(bytes);

  final Uint8List bytes;
  final ByteData data;
  int offset = 0;

  void expectMagic(List<int> expected, String name) {
    _require(expected.length, name);
    for (var index = 0; index < expected.length; index += 1) {
      if (bytes[offset + index] != expected[index]) {
        throw FlarkV3DocumentQueryException('$name magic is invalid.');
      }
    }
    offset += expected.length;
  }

  void expectU32(int expected, String name) {
    if (u32(name) != expected) {
      throw FlarkV3DocumentQueryException('$name is unsupported.');
    }
  }

  void expectU8(int expected, String name) {
    if (u8(name) != expected) {
      throw FlarkV3DocumentQueryException('$name is unsupported.');
    }
  }

  int u8(String name) {
    _require(1, name);
    final value = data.getUint8(offset);
    offset += 1;
    return value;
  }

  int u16(String name) {
    _require(2, name);
    final value = data.getUint16(offset, Endian.little);
    offset += 2;
    return value;
  }

  int variant(String name) {
    _require(4, name);
    final value = bytes[offset];
    if (bytes[offset + 1] != 0 ||
        bytes[offset + 2] != 0 ||
        bytes[offset + 3] != 0) {
      throw FlarkV3DocumentQueryException('$name reserved bytes are nonzero.');
    }
    offset += 4;
    return value;
  }

  int u32(String name) {
    _require(4, name);
    final value = data.getUint32(offset, Endian.little);
    offset += 4;
    return value;
  }

  int u64AsU32(String name) {
    _require(8, name);
    final low = data.getUint32(offset, Endian.little);
    final high = data.getUint32(offset + 4, Endian.little);
    offset += 8;
    if (high != 0) {
      throw FlarkV3DocumentQueryException('$name exceeds the v1 range.');
    }
    return low;
  }

  BigInt u64(String name) {
    _require(8, name);
    final low = data.getUint32(offset, Endian.little);
    final high = data.getUint32(offset + 4, Endian.little);
    offset += 8;
    return BigInt.from(low) | (BigInt.from(high) << 32);
  }

  FlarkV3SourceSpan span(
    String name, {
    required int sourceBytes,
    required int Function(int utf8Offset) utf8ToUtf16,
  }) {
    final start = u64AsU32('$name start');
    final end = u64AsU32('$name end');
    if (start > end || end > sourceBytes) {
      throw FlarkV3DocumentQueryException('$name is outside exact source.');
    }
    return FlarkV3SourceSpan(
      startUtf8: start,
      endUtf8: end,
      startUtf16: utf8ToUtf16(start),
      endUtf16: utf8ToUtf16(end),
    );
  }

  void expectEnd(String name) {
    if (offset != bytes.length) {
      throw FlarkV3DocumentQueryException('$name has trailing bytes.');
    }
  }

  void _require(int count, String name) {
    if (offset + count > bytes.length) {
      throw FlarkV3DocumentQueryException('$name is truncated.');
    }
  }
}

const int _roleSchema = 1;
const int _fenceClosedFlag = 1 << 16;
const int _fenceMetadataMask = 0x1ffff;
const int _fenceAbsentCut = _structuredAbsentCut;
const int _atxHasClosingMarkerFlag = 1 << 8;
const int _atxOpeningIndentShift = 9;
const int _atxHasBofBomFlag = 1 << 11;
const int _atxMetadataMask = 0xfff;
const int _atxHeadingVariant = 4;
const int _setextHeadingVariant = 5;
const int _setextOpeningIndentShift = 8;
const int _setextOpeningIndentMask = 0x3;
const int _setextMetadataMask = 0x3ff;
const int _thematicBreakVariant = 6;
const int _thematicBreakOpeningIndentShift = 8;
const int _thematicBreakHasBofBomFlag = 1 << 10;
const int _thematicBreakMetadataMask = 0x7ff;
const int _indentedCodeVariant = 7;
const int _indentedCodeHasBofBomFlag = 1 << 8;
const int _indentedCodeMetadataMask = 0x1ff;
const int _blockQuoteVariant = 8;
const int _blockQuoteExactSingleParagraphDisposition = 1;
const int _bulletListVariant = 9;
const int _bulletListExactDisposition = 1;
const int _bulletListMarkerShift = 8;
const int _bulletListTightFlag = 1 << 16;
const int _bulletListMetadataMask = 0x1ffff;
const int _bulletListAbsentTerminalEmpty = 0xffffffff;
const int _orderedListVariant = 10;
const int _orderedListExactDisposition = 1;
const int _orderedListDelimiterShift = 8;
const int _orderedListTightFlag = 1 << 16;
const int _orderedListMetadataMask = 0x1ffff;
const int _orderedListAbsentTerminalEmpty = 0xffffffff;
const int _structuredAbsentCut = 0xffffffff;
const int _viewportSchemaV1 = 1;
const int _viewportSchemaV3 = 3;
const int _viewportSchemaV4 = 4;
const int _viewportSchemaV5 = 5;
const int _viewportSchemaV6 = 6;
const int _viewportSchemaV7 = 7;
const int _viewportSchemaV8 = 8;
const int _viewportSchemaV9 = 9;
const int _recursiveGreenViewportHeaderBytes = 112;
const int _recursiveGreenAncestorRecordBytes = 16;
const int _recursiveGreenKindRegistrySchema = 1;
const int _recursiveGreenCoverageSchema = 1;
const int _recursiveGreenLogicalAtomSchema = 1;
const int _recursiveGreenAncestorOwnerFlag = 1;
const int _rangeSchema = 1;
const int _rangeHeaderBytes = 32;
const int _rangeRecordBytes = 160;
const int _recursiveGreenRowRangeSchema = 11;
const int _recursiveGreenRowRangeHeaderBytes = 96;
const int _recursiveGreenRowRecordBytes = 64;
const int _recursiveGreenPathRecordBytes = 48;
const int _recursiveGreenRowFactRegistrySchema = 1;
const int _recursiveGreenRowRangeCompleteFlag = 1;
const int _recursiveGreenNoSelectedRow = 0xffffffff;
const int _recursiveGreenRowSelectedFlag = 1;
const int _recursiveGreenRowInlineCapableFlag = 1 << 1;
const int _recursiveGreenRowLiteralFlag = 1 << 2;
const int _recursiveGreenRowFlagMask =
    _recursiveGreenRowSelectedFlag |
    _recursiveGreenRowInlineCapableFlag |
    _recursiveGreenRowLiteralFlag;
const int _recursiveGreenPathOwnerFlag = 1;
const int _recursiveGreenPathContainerFlag = 1 << 1;
const int _recursiveGreenPathOpenFactFlag = 1 << 2;
const int _recursiveGreenPathCloseFactFlag = 1 << 3;
const int _recursiveGreenPathFlagMask =
    _recursiveGreenPathOwnerFlag |
    _recursiveGreenPathContainerFlag |
    _recursiveGreenPathOpenFactFlag |
    _recursiveGreenPathCloseFactFlag;
const int _rangeCompleteFlag = 1;
const int _greenRecordBytes = 80;
const int _projectionRecordBytes = 56;
const int _viewportHeaderV1Bytes = 20;
const int _viewportHeaderV3Bytes = 28;
const int _viewportHeaderV4Bytes = 32;
const int _viewportHeaderV5Bytes = 32;
const int _viewportHeaderV6Bytes = 32;
const int _viewportHeaderV7Bytes = 32;
const int _viewportHeaderV8Bytes = 24;
const int _leafProjectionPayloadNone = 0;
const int _leafProjectionPayloadInline = 1;
const int _leafProjectionPayloadIndentedCode = 2;
const int _leafProjectionPayloadBlockQuote = 3;
const int _leafProjectionPayloadList = 4;
const int _leafProjectionPayloadListItem = 5;
const int _leafProjectionPayloadOrderedListItem = 6;
const int _documentPointPathV4NodeRecordBytes = 40;
const int _documentPointPathV5NodeRecordBytes = 32;
const int _blockQuotePointPathNodeCount = 2;
const int _pointPathBlockQuoteKind = 1;
const int _pointPathParagraphKind = 2;
const int _pointPathListKind = 3;
const int _pointPathListItemKind = 4;
const int _pointPathNoncontiguousFlag = 1;
const int _pointPathSelectedFlag = 1 << 1;
const int _pointPathFlagMask =
    _pointPathNoncontiguousFlag | _pointPathSelectedFlag;
const int _pointPathRootParent = 0xffffffff;
const int _inlineSchema = 2;
const int _inlineHeaderBytes = 48;
const List<int> _viewportMagic = <int>[70, 76, 75, 86, 80, 48, 48, 49];
const List<int> _rangeMagic = <int>[70, 76, 75, 86, 82, 48, 48, 49];
const List<int> _greenMagic = <int>[70, 76, 75, 71, 82, 48, 48, 49];
const List<int> _projectionMagic = <int>[70, 76, 75, 80, 82, 48, 48, 49];
const List<int> _inlineMagic = <int>[70, 76, 75, 73, 78, 48, 48, 50];
