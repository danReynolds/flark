import 'dart:math' as math;

import 'package:flark/flark_adapter.dart';
import 'package:flutter/foundation.dart' show Listenable, setEquals;
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'flark_v3_block_chrome.dart';
import 'flark_v3_flutter_live_controller.dart';
import 'flark_v3_inline_image.dart';
import 'flark_v3_inline_editing_presentation.dart';
import 'flark_v3_live_editor_prototype.dart';
import 'flark_v3_visible_block_coordinator.dart';

/// Maximum number of parser-authored block presentations admitted by one
/// Flutter viewport window.
///
/// A larger viewport must be split before it reaches the widget tree. This is
/// intentionally lower than the Dart structural materializer's 256-block hard
/// cap: one surface window includes visible blocks plus bounded overscan.
const int flarkV3MaximumMountedViewportPresentations = 96;

/// Source-compatible Flutter spelling for the core materializer's exact
/// parser authority. This is an alias, not a second identity model.
typedef FlarkV3ViewportPresentationIdentity =
    FlarkV3MaterializedViewportIdentity;

/// One disjoint display run authored from parser-certified projection facts.
///
/// Coordinates are relative to the containing block's marker-free
/// [FlarkV3ParserAuthoredBlockPresentation.displayText]. Semantic styles are
/// parser facts, not classifications inferred by Flutter.
@immutable
final class FlarkV3PassiveInlineRun {
  FlarkV3PassiveInlineRun({
    required this.startUtf16,
    required this.endUtf16,
    required Iterable<FlarkV3InlineFactKind> styles,
    this.linkAnnotation,
  }) : styles = Set<FlarkV3InlineFactKind>.unmodifiable(styles) {
    if (startUtf16 < 0 || endUtf16 <= startUtf16) {
      throw RangeError('A passive inline run must be non-empty and ordered.');
    }
    if (this.styles.contains(FlarkV3InlineFactKind.autolinkUri) ||
        this.styles.contains(FlarkV3InlineFactKind.autolinkEmail)) {
      throw ArgumentError(
        'Autolinks must use the typed passive link annotation.',
      );
    }
  }

  final int startUtf16;
  final int endUtf16;
  final Set<FlarkV3InlineFactKind> styles;

  /// Exact current-source activation authority, when this run is a link.
  ///
  /// Flutter never parses or normalizes this target. Active editing retains
  /// only non-actionable link paint; this full annotation is kept exclusively
  /// on passive parser-authored rows.
  final FlarkV3InlineLinkAnnotation? linkAnnotation;
}

/// One passive image replacing its marker-free alt range.
///
/// [startUtf16] and [endUtf16] are display coordinates and may be equal for
/// an empty alt label. [outerLink] is independently parser-certified; the
/// image destination itself never becomes an activation target.
@immutable
final class FlarkV3PassiveInlineImage {
  FlarkV3PassiveInlineImage({
    required this.startUtf16,
    required this.endUtf16,
    required this.annotation,
    this.outerLink,
  }) {
    if (startUtf16 < 0 || endUtf16 < startUtf16) {
      throw RangeError('A passive inline image has invalid display geometry.');
    }
  }

  final int startUtf16;
  final int endUtf16;
  final FlarkV3InlineImageAnnotation annotation;
  final FlarkV3InlineLinkAnnotation? outerLink;
}

/// Whether the parser supplied a complete passive presentation for a block.
enum FlarkV3PassivePresentationDisposition { authoritative, unsupported }

/// Normalized parser-authored presentation for one passive structural block.
///
/// This is deliberately a rendering model, not a Markdown model. A core Dart
/// adapter joins exact structural range facts with one schema-8 aggregate
/// entry and constructs this value from existing typed projection decoders.
/// Flutter consumes only the resulting text, runs, and structural kind.
@immutable
final class FlarkV3ParserAuthoredBlockPresentation {
  /// Adapts an already-materialized core block to Flutter paint data.
  ///
  /// The core [FlarkV3ViewportPageMaterializer] has already performed the
  /// schema-8/structural join and every source projection. This factory does
  /// no payload decoding, source scanning, or marker recognition.
  factory FlarkV3ParserAuthoredBlockPresentation.fromMaterialized(
    FlarkV3MaterializedViewportBlock block, {
    double estimatedExtent = 44,
  }) {
    if (block case FlarkV3AuthoritativeViewportBlock(:final displayText)) {
      final runs = switch (block) {
        FlarkV3InlineViewportBlock(:final displayRuns) => [
          for (final run in displayRuns)
            FlarkV3PassiveInlineRun(
              startUtf16: run.displayStartUtf16,
              endUtf16: run.displayEndUtf16,
              styles: run.semanticStyles,
              linkAnnotation: run.linkAnnotation,
            ),
        ],
        _ when displayText.isEmpty => const <FlarkV3PassiveInlineRun>[],
        _ => [
          FlarkV3PassiveInlineRun(
            startUtf16: 0,
            endUtf16: displayText.length,
            styles: const <FlarkV3InlineFactKind>[],
          ),
        ],
      };
      final images = switch (block) {
        FlarkV3InlineViewportBlock() => _passiveImagesFromMaterialized(block),
        _ => const <FlarkV3PassiveInlineImage>[],
      };
      return FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: block.identity,
        ordinal: block.ordinal,
        physicalSource: block.physicalSource,
        visibleSource: block.visibleSource,
        kind: block.kind,
        displayText: displayText,
        runs: runs,
        images: images,
        headingLevel: block.headingLevel,
        estimatedExtent: estimatedExtent,
      );
    }
    final fallback = block as FlarkV3SourceFallbackViewportBlock;
    return FlarkV3ParserAuthoredBlockPresentation.unsupported(
      identity: block.identity,
      ordinal: block.ordinal,
      physicalSource: block.physicalSource,
      visibleSource: block.visibleSource,
      kind: block.kind,
      fallbackReason: fallback.reason,
      unsupportedReason: fallback.unsupportedReason,
      headingLevel: block.headingLevel,
      estimatedExtent: estimatedExtent,
    );
  }

  /// Adapts one ACK/frame-bound recursive-Green row to shared passive paint.
  factory FlarkV3ParserAuthoredBlockPresentation.fromRecursiveGreenMaterialized(
    FlarkV3MaterializedRecursiveGreenRow materialized, {
    double estimatedExtent = 44,
  }) {
    final row = materialized.row;
    final visibleSource = row.editableSource ?? row.physicalSource;
    final physicalSource = row.presentationPhysicalSource;
    final ordinal = _recursiveGreenOrdinalAsInt(row.globalOrdinal);
    final kind = _recursiveGreenDocumentKind(row.kind);
    final headingLevel = _recursiveGreenHeadingLevel(row);
    if (materialized.isAuthoritative) {
      final projection = materialized.inlineProjection;
      final runs = projection == null
          ? materialized.displayText.isEmpty
                ? const <FlarkV3PassiveInlineRun>[]
                : <FlarkV3PassiveInlineRun>[
                    FlarkV3PassiveInlineRun(
                      startUtf16: 0,
                      endUtf16: materialized.displayText.length,
                      styles: const <FlarkV3InlineFactKind>{},
                    ),
                  ]
          : <FlarkV3PassiveInlineRun>[
              for (final run in projection.runs)
                FlarkV3PassiveInlineRun(
                  startUtf16: run.displayStartUtf16,
                  endUtf16: run.displayEndUtf16,
                  styles: run.semanticStyles,
                  linkAnnotation: run.linkAnnotation,
                ),
            ];
      final images = projection == null || materialized.inlineFacts == null
          ? const <FlarkV3PassiveInlineImage>[]
          : _passiveImagesFromInlineAuthority(
              materialized.inlineFacts!,
              projection,
            );
      final inputLease = projection != null
          ? FlarkV3ProjectedInputLease.fromInlineProjection(projection)
          : row.kind.isTerminalEmptyItem
          ? FlarkV3ProjectedInputLease.fromSourceProjection(
              FlarkV3SourceProjection.fromSource(
                sourceStartUtf16: visibleSource.startUtf16,
                sourceText: '',
                pieces: const <FlarkV3SourceProjectionPiece>[],
                certifiedSourceVersion: materialized.identity.sourceVersion,
              ),
            )
          : null;
      return FlarkV3ParserAuthoredBlockPresentation.authoritative(
        identity: materialized.identity,
        ordinal: ordinal,
        physicalSource: physicalSource,
        visibleSource: visibleSource,
        kind: kind,
        displayText: materialized.displayText,
        runs: runs,
        images: images,
        headingLevel: headingLevel,
        estimatedExtent: estimatedExtent,
        recursiveGreenStructuralAck: materialized.structuralAck,
        recursiveGreenRow: row,
        recursiveGreenInputLease: inputLease,
      );
    }
    return FlarkV3ParserAuthoredBlockPresentation.unsupported(
      identity: materialized.identity,
      ordinal: ordinal,
      physicalSource: physicalSource,
      visibleSource: visibleSource,
      kind: kind,
      fallbackReason: materialized.fallbackReason,
      headingLevel: headingLevel,
      estimatedExtent: estimatedExtent,
      recursiveGreenStructuralAck: materialized.structuralAck,
      recursiveGreenRow: row,
    );
  }

  FlarkV3ParserAuthoredBlockPresentation.authoritative({
    required this.identity,
    required this.ordinal,
    required this.physicalSource,
    required this.visibleSource,
    required this.kind,
    required this.displayText,
    required Iterable<FlarkV3PassiveInlineRun> runs,
    Iterable<FlarkV3PassiveInlineImage> images =
        const <FlarkV3PassiveInlineImage>[],
    this.headingLevel,
    this.estimatedExtent = 44,
    this.recursiveGreenStructuralAck,
    this.recursiveGreenRow,
    this.recursiveGreenInputLease,
  }) : disposition = FlarkV3PassivePresentationDisposition.authoritative,
       fallbackReason = null,
       unsupportedReason = null,
       runs = List<FlarkV3PassiveInlineRun>.unmodifiable(
         _coalesceEquivalentPassiveRuns(runs),
       ),
       images = List<FlarkV3PassiveInlineImage>.unmodifiable(images) {
    _validate();
  }

  FlarkV3ParserAuthoredBlockPresentation.unsupported({
    required this.identity,
    required this.ordinal,
    required this.physicalSource,
    required this.visibleSource,
    required this.kind,
    required this.fallbackReason,
    this.unsupportedReason,
    this.headingLevel,
    this.estimatedExtent = 44,
    this.recursiveGreenStructuralAck,
    this.recursiveGreenRow,
  }) : disposition = FlarkV3PassivePresentationDisposition.unsupported,
       displayText = '',
       runs = const <FlarkV3PassiveInlineRun>[],
       images = const <FlarkV3PassiveInlineImage>[],
       recursiveGreenInputLease = null {
    _validate();
    if (unsupportedReason != null && unsupportedReason! <= 0) {
      throw RangeError('An unsupported presentation needs an exact reason.');
    }
  }

  final FlarkV3ViewportPresentationIdentity identity;
  final int ordinal;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan visibleSource;
  final FlarkV3DocumentStructureKind kind;
  final FlarkV3PassivePresentationDisposition disposition;
  final String displayText;
  final List<FlarkV3PassiveInlineRun> runs;
  final List<FlarkV3PassiveInlineImage> images;
  final int? headingLevel;
  final Object? fallbackReason;
  final int? unsupportedReason;
  final FlarkV3StructuralAck? recursiveGreenStructuralAck;
  final FlarkV3RecursiveGreenRenderableRow? recursiveGreenRow;
  final FlarkV3ProjectedInputLease? recursiveGreenInputLease;

  /// Parser/materializer-provided layout estimate used only for offscreen
  /// scroll geometry. Mounted blocks retain their natural Flutter height.
  final double estimatedExtent;

  bool get isAuthoritative =>
      disposition == FlarkV3PassivePresentationDisposition.authoritative;

  void _validate() {
    if (ordinal < 0 ||
        estimatedExtent <= 0 ||
        !estimatedExtent.isFinite ||
        physicalSource.startUtf8 > visibleSource.startUtf8 ||
        physicalSource.endUtf8 < visibleSource.endUtf8 ||
        physicalSource.startUtf16 > visibleSource.startUtf16 ||
        physicalSource.endUtf16 < visibleSource.endUtf16) {
      throw RangeError('A passive block presentation has invalid geometry.');
    }
    if ((kind == FlarkV3DocumentStructureKind.heading) !=
        (headingLevel != null)) {
      throw ArgumentError(
        'Only a heading presentation may carry a heading level.',
      );
    }
    if (headingLevel != null && (headingLevel! < 1 || headingLevel! > 6)) {
      throw RangeError.range(headingLevel!, 1, 6, 'headingLevel');
    }
    final green = recursiveGreenRow;
    final greenInputLease = recursiveGreenInputLease;
    if ((green == null) != (recursiveGreenStructuralAck == null) ||
        green != null &&
            (green.globalOrdinal != BigInt.from(ordinal) ||
                !_sameSpan(green.presentationPhysicalSource, physicalSource) ||
                !_sameSpan(
                  green.editableSource ?? green.physicalSource,
                  visibleSource,
                ) ||
                _recursiveGreenDocumentKind(green.kind) != kind)) {
      throw ArgumentError(
        'Recursive-Green row authority does not match passive paint.',
      );
    }
    if (greenInputLease != null &&
        (green == null ||
            !isAuthoritative ||
            !(green.kind.isInlineBearing || green.kind.isTerminalEmptyItem) ||
            greenInputLease.certifiedSourceVersion != identity.sourceVersion ||
            greenInputLease.sourceStartUtf16 != visibleSource.startUtf16 ||
            greenInputLease.sourceEndUtf16 != visibleSource.endUtf16 ||
            greenInputLease.displayText != displayText)) {
      throw ArgumentError(
        'Recursive-Green input authority does not match passive paint.',
      );
    }
    if (!isAuthoritative) {
      if (displayText.isNotEmpty || runs.isNotEmpty || images.isNotEmpty) {
        throw ArgumentError(
          'An unsupported passive block cannot expose partial display text.',
        );
      }
      return;
    }
    if (displayText.isEmpty) {
      if (runs.isNotEmpty) {
        throw ArgumentError('An empty passive display cannot carry runs.');
      }
    } else {
      var cursor = 0;
      for (final run in runs) {
        if (run.startUtf16 != cursor || run.endUtf16 > displayText.length) {
          throw ArgumentError(
            'Passive runs must exactly and consecutively tile display text.',
          );
        }
        cursor = run.endUtf16;
      }
      if (cursor != displayText.length) {
        throw ArgumentError(
          'Passive runs must exactly and consecutively tile display text.',
        );
      }
    }
    var previousEnd = 0;
    var previousSourceStart = -1;
    for (final image in images) {
      if (image.startUtf16 < previousEnd ||
          image.endUtf16 > displayText.length ||
          image.annotation.source.startUtf16 <= previousSourceStart) {
        throw ArgumentError(
          'Passive images must be disjoint and remain in parser source order.',
        );
      }
      previousEnd = image.endUtf16;
      previousSourceStart = image.annotation.source.startUtf16;
    }
  }
}

List<FlarkV3PassiveInlineImage> _passiveImagesFromMaterialized(
  FlarkV3InlineViewportBlock block,
) => _passiveImagesFromInlineAuthority(block.facts, block.projection);

List<FlarkV3PassiveInlineImage> _passiveImagesFromInlineAuthority(
  FlarkV3InlineFacts facts,
  FlarkV3InlineProjection projection,
) {
  final imageFacts = [
    for (final fact in facts.facts)
      if (fact.imageAnnotation != null) fact,
  ];
  final output = <FlarkV3PassiveInlineImage>[];
  for (final imageFact in imageFacts) {
    final image = imageFact.imageAnnotation!;
    final nestedInsideImage = imageFacts.any(
      (candidate) =>
          !identical(candidate, imageFact) &&
          candidate.content.startUtf16 <= image.source.startUtf16 &&
          candidate.content.endUtf16 >= image.source.endUtf16,
    );
    if (nestedInsideImage) continue;

    FlarkV3InlineLinkAnnotation? outerLink;
    var enclosingLength = 1 << 62;
    for (final candidate in facts.facts) {
      final link = candidate.linkAnnotation;
      if (link == null ||
          candidate.content.startUtf16 > image.source.startUtf16 ||
          candidate.content.endUtf16 < image.source.endUtf16) {
        continue;
      }
      final length = candidate.content.endUtf16 - candidate.content.startUtf16;
      if (length < enclosingLength) {
        outerLink = link;
        enclosingLength = length;
      }
    }
    output.add(
      FlarkV3PassiveInlineImage(
        startUtf16: projection.sourceToDisplayOffset(image.content.startUtf16),
        endUtf16: projection.sourceToDisplayOffset(image.content.endUtf16),
        annotation: image,
        outerLink: outerLink,
      ),
    );
  }
  return output;
}

int _recursiveGreenOrdinalAsInt(BigInt ordinal) {
  if (ordinal < BigInt.zero || ordinal > BigInt.from(0xffffffff)) {
    throw RangeError('A Flutter row ordinal must fit the v1 product range.');
  }
  return ordinal.toInt();
}

FlarkV3DocumentStructureKind _recursiveGreenDocumentKind(
  FlarkV3RecursiveGreenKind kind,
) => switch (kind) {
  FlarkV3RecursiveGreenKind.paragraph => FlarkV3DocumentStructureKind.paragraph,
  FlarkV3RecursiveGreenKind.fencedCode =>
    FlarkV3DocumentStructureKind.fencedCode,
  FlarkV3RecursiveGreenKind.indentedCode =>
    FlarkV3DocumentStructureKind.indentedCode,
  FlarkV3RecursiveGreenKind.heading => FlarkV3DocumentStructureKind.heading,
  FlarkV3RecursiveGreenKind.thematicBreak =>
    FlarkV3DocumentStructureKind.thematicBreak,
  FlarkV3RecursiveGreenKind.terminalEmptyItem =>
    FlarkV3DocumentStructureKind.empty,
  _ => FlarkV3DocumentStructureKind.unknown,
};

int? _recursiveGreenHeadingLevel(FlarkV3RecursiveGreenRenderableRow row) {
  if (row.kind != FlarkV3RecursiveGreenKind.heading) return null;
  for (final frame in row.path.reversed) {
    final fact = frame.fact;
    if (fact is FlarkV3RecursiveGreenHeadingPathFact) return fact.level;
  }
  throw ArgumentError('A recursive-Green Heading row has no Heading fact.');
}

Iterable<FlarkV3PassiveInlineRun> _coalesceEquivalentPassiveRuns(
  Iterable<FlarkV3PassiveInlineRun> runs,
) sync* {
  FlarkV3PassiveInlineRun? pending;
  for (final run in runs) {
    final previous = pending;
    if (previous != null &&
        previous.endUtf16 == run.startUtf16 &&
        identical(previous.linkAnnotation, run.linkAnnotation) &&
        setEquals(previous.styles, run.styles)) {
      pending = FlarkV3PassiveInlineRun(
        startUtf16: previous.startUtf16,
        endUtf16: run.endUtf16,
        styles: previous.styles,
        linkAnnotation: previous.linkAnnotation,
      );
      continue;
    }
    if (previous != null) yield previous;
    pending = run;
  }
  if (pending != null) yield pending;
}

/// One bounded ordinal request derived from Flutter scroll geometry.
@immutable
final class FlarkV3ViewportWindowDemand {
  FlarkV3ViewportWindowDemand({
    required this.centerOrdinal,
    required this.maximumBlocks,
  }) {
    if (centerOrdinal < 0 ||
        maximumBlocks <= 0 ||
        maximumBlocks > flarkV3MaximumMountedViewportPresentations) {
      throw RangeError('Viewport window demand exceeds the Flutter cap.');
    }
  }

  final int centerOrdinal;
  final int maximumBlocks;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ViewportWindowDemand &&
      other.centerOrdinal == centerOrdinal &&
      other.maximumBlocks == maximumBlocks;

  @override
  int get hashCode => Object.hash(centerOrdinal, maximumBlocks);
}

/// Revision-bound viewport state supplied to the Flutter surface.
sealed class FlarkV3ViewportSurfaceSnapshot {
  const FlarkV3ViewportSurfaceSnapshot({
    required this.totalBlockCount,
    required this.activeOrdinal,
    required this.estimatedBlockExtent,
  }) : assert(totalBlockCount > 0),
       assert(activeOrdinal >= 0),
       assert(activeOrdinal < totalBlockCount),
       assert(estimatedBlockExtent > 0);

  final int totalBlockCount;
  final int activeOrdinal;
  final double estimatedBlockExtent;
}

/// Exact normalized page ready to join with the structural coordinator.
final class FlarkV3ExactViewportSurfaceSnapshot
    extends FlarkV3ViewportSurfaceSnapshot {
  factory FlarkV3ExactViewportSurfaceSnapshot.fromMaterialization({
    required int totalBlockCount,
    required int activeOrdinal,
    required double estimatedBlockExtent,
    required FlarkV3ExactViewportPageMaterialization materialization,
  }) => FlarkV3ExactViewportSurfaceSnapshot(
    totalBlockCount: totalBlockCount,
    activeOrdinal: activeOrdinal,
    estimatedBlockExtent: estimatedBlockExtent,
    identity: materialization.identity,
    blocks: [
      for (final block in materialization.blocks)
        FlarkV3ParserAuthoredBlockPresentation.fromMaterialized(
          block,
          estimatedExtent: estimatedBlockExtent,
        ),
    ],
  );

  factory FlarkV3ExactViewportSurfaceSnapshot.fromRecursiveGreenMaterialization({
    required int activeOrdinal,
    required double estimatedBlockExtent,
    required FlarkV3ExactRecursiveGreenViewportPageMaterialization
    materialization,
  }) {
    final totalBlockCount = _recursiveGreenOrdinalAsInt(
      materialization.totalGlobalRowCount,
    );
    if (activeOrdinal < 0 ||
        activeOrdinal >= totalBlockCount ||
        !materialization.rows.any(
          (materialized) =>
              materialized.row.globalOrdinal == BigInt.from(activeOrdinal),
        )) {
      throw ArgumentError(
        'A live recursive-Green viewport needs one mounted active row.',
      );
    }
    return FlarkV3ExactViewportSurfaceSnapshot(
      totalBlockCount: totalBlockCount,
      activeOrdinal: activeOrdinal,
      estimatedBlockExtent: estimatedBlockExtent,
      identity: materialization.identity,
      blocks: [
        for (final row in materialization.rows)
          FlarkV3ParserAuthoredBlockPresentation.fromRecursiveGreenMaterialized(
            row,
            estimatedExtent: estimatedBlockExtent,
          ),
      ],
    );
  }

  FlarkV3ExactViewportSurfaceSnapshot({
    required super.totalBlockCount,
    required super.activeOrdinal,
    required super.estimatedBlockExtent,
    required this.identity,
    required Iterable<FlarkV3ParserAuthoredBlockPresentation> blocks,
  }) : blocks = List<FlarkV3ParserAuthoredBlockPresentation>.unmodifiable(
         blocks,
       ) {
    if (this.blocks.isEmpty ||
        this.blocks.length > flarkV3MaximumMountedViewportPresentations) {
      throw RangeError(
        'An exact Flutter viewport page must contain 1 through '
        '$flarkV3MaximumMountedViewportPresentations blocks.',
      );
    }
    var expectedOrdinal = this.blocks.first.ordinal;
    for (final block in this.blocks) {
      if (block.identity != identity ||
          block.ordinal != expectedOrdinal ||
          block.ordinal >= totalBlockCount) {
        throw ArgumentError(
          'Viewport presentations must be one consecutive exact page.',
        );
      }
      expectedOrdinal += 1;
    }
  }

  final FlarkV3ViewportPresentationIdentity identity;
  final List<FlarkV3ParserAuthoredBlockPresentation> blocks;

  int get firstOrdinal => blocks.first.ordinal;
  int get lastOrdinal => blocks.last.ordinal;
  bool containsOrdinal(int ordinal) =>
      ordinal >= firstOrdinal && ordinal <= lastOrdinal;
}

/// Fail-closed viewport state used while authority is pending or unavailable.
///
/// It deliberately carries no source text. Flutter can retain scroll and
/// active-input geometry without painting stale or locally interpreted
/// Markdown.
final class FlarkV3SourceGapViewportSurfaceSnapshot
    extends FlarkV3ViewportSurfaceSnapshot {
  const FlarkV3SourceGapViewportSurfaceSnapshot({
    required super.totalBlockCount,
    required super.activeOrdinal,
    required super.estimatedBlockExtent,
    required this.reason,
  });

  final Object reason;
}

/// Parser-facing window source borrowed by the virtualized Flutter surface.
///
/// The production implementation owns the schema-8 + structural join and
/// routes window demands through [FlarkV3FlutterVisibleBlockCoordinator].
/// Fakes may supply normalized values in focused widget tests; the surface
/// itself never parses source, strips markers, or creates passive controllers.
abstract interface class FlarkV3ViewportPresentationSource
    implements Listenable {
  FlarkV3ViewportSurfaceSnapshot get snapshot;

  void requestWindow(FlarkV3ViewportWindowDemand demand);

  /// Activates an already parser-identified block using the long-lived live
  /// controller owned by the production adapter.
  void activateOrdinal(int ordinal);
}

typedef FlarkV3SourceGapBuilder =
    Widget Function(
      BuildContext context,
      FlarkV3ViewportSurfaceSnapshot snapshot,
    );

/// Read-only inspection and command seam for tests and product diagnostics.
///
/// It never exposes source bytes or mutates Markdown. Commands delegate to the
/// parser-facing [FlarkV3ViewportPresentationSource].
final class FlarkV3VirtualizedLiveSurfaceController {
  _FlarkV3VirtualizedLiveSurfaceState? _state;

  int get mountedPresentationCount =>
      _state?._mountedPresentationOrdinals.length ?? 0;

  Set<int> get mountedPresentationOrdinals => Set<int>.unmodifiable(
    _state?._mountedPresentationOrdinals ?? const <int>{},
  );

  int passiveBuildCount(int ordinal) =>
      _state?._passiveBuildCounts[ordinal] ?? 0;

  FlarkV3ViewportSurfaceSnapshot? get snapshot =>
      _state?.widget.presentationSource.snapshot;

  void revealAndActivateOrdinal(int ordinal) {
    final state = _state;
    if (state == null) {
      throw StateError('The virtualized surface is not mounted.');
    }
    state._revealAndActivateOrdinal(ordinal);
  }
}

typedef FlarkV3ActiveEditorBuilder = Widget Function(BuildContext context);

@visibleForTesting
typedef FlarkV3ActivePresentationReadiness =
    bool Function(FlarkV3ParserAuthoredBlockPresentation target);

/// Virtualized multi-block v3 editing surface.
///
/// Exactly one [FlarkV3LiveEditorPrototype] lives in a stable overlay branch.
/// A composited transform follows the active row without reparenting that
/// editor or its [EditableTextState]. Passive rows are controller-free
/// structural widgets built only from parser-authored normalized
/// presentations. Offscreen document geometry is represented by two spacers,
/// so a 4,096-block document never builds 4,096 widgets.
final class FlarkV3VirtualizedLiveSurface extends StatefulWidget {
  FlarkV3VirtualizedLiveSurface({
    super.key,
    required FlarkV3FlutterLiveController liveController,
    required this.visibleBlockCoordinator,
    required this.presentationSource,
    required FlarkV3PaintLayerBuilder paintLayerBuilder,
    this.controller,
    this.scrollController,
    this.focusNode,
    this.editableKey,
    this.style = const TextStyle(fontSize: 16, height: 1.35),
    this.codeStyle = const TextStyle(fontFamily: 'monospace'),
    this.blockSpacing = 12,
    this.horizontalPadding = 16,
    this.cacheExtent = 720,
    this.windowBlockCount = FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
    this.sourceGapBuilder = _defaultSourceGapBuilder,
    this.onLinkActivated,
    this.inlineImageBuilder,
  }) : paintLayerBuilder = paintLayerBuilder,
       _activePresentationProgress = liveController,
       _activePresentationReadiness = ((target) {
         return liveController.isExactCurrentPresentationFor(
           targetSourceVersion: target.identity.sourceVersion,
           targetPhysicalSource: target.physicalSource,
           targetKind: target.kind,
           targetDisplayText: target.displayText,
           targetRecursiveGreenAck: target.recursiveGreenStructuralAck,
           targetRecursiveGreenRow: target.recursiveGreenRow,
         );
       }),
       _activeEditorBuilder = ((context) {
         return FlarkV3LiveEditorPrototype(
           controller: liveController,
           paintLayerBuilder: paintLayerBuilder,
           editableKey: editableKey,
           focusNode: focusNode,
           style: style,
           fencedCodeStyle: codeStyle,
         );
       }),
       assert(blockSpacing >= 0),
       assert(horizontalPadding >= 0),
       assert(cacheExtent >= 0),
       assert(windowBlockCount > 0),
       assert(windowBlockCount <= flarkV3MaximumMountedViewportPresentations);

  /// Focused widget-test seam for the virtualization topology.
  ///
  /// Production adapters must use the default constructor, which always hosts
  /// [FlarkV3LiveEditorPrototype]. This seam exists so the surface can be
  /// tested without constructing a parser host for every layout invariant.
  @visibleForTesting
  const FlarkV3VirtualizedLiveSurface.withActiveEditorBuilder({
    super.key,
    required FlarkV3ActiveEditorBuilder activeEditorBuilder,
    required this.visibleBlockCoordinator,
    required this.presentationSource,
    this.controller,
    this.scrollController,
    this.focusNode,
    this.style = const TextStyle(fontSize: 16, height: 1.35),
    this.codeStyle = const TextStyle(fontFamily: 'monospace'),
    this.blockSpacing = 12,
    this.horizontalPadding = 16,
    this.cacheExtent = 720,
    this.windowBlockCount = FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
    this.sourceGapBuilder = _defaultSourceGapBuilder,
    this.onLinkActivated,
    this.inlineImageBuilder,
    Listenable? activePresentationProgress,
    FlarkV3ActivePresentationReadiness? activePresentationReadiness,
  }) : _activeEditorBuilder = activeEditorBuilder,
       _activePresentationProgress = activePresentationProgress,
       _activePresentationReadiness = activePresentationReadiness,
       paintLayerBuilder = null,
       editableKey = null,
       assert(blockSpacing >= 0),
       assert(horizontalPadding >= 0),
       assert(cacheExtent >= 0),
       assert(windowBlockCount > 0),
       assert(
         (activePresentationProgress == null) ==
             (activePresentationReadiness == null),
       ),
       assert(windowBlockCount <= flarkV3MaximumMountedViewportPresentations);

  final FlarkV3FlutterVisibleBlockCoordinator visibleBlockCoordinator;
  final FlarkV3ViewportPresentationSource presentationSource;
  final FlarkV3PaintLayerBuilder? paintLayerBuilder;
  final FlarkV3VirtualizedLiveSurfaceController? controller;
  final ScrollController? scrollController;
  final FocusNode? focusNode;
  final Key? editableKey;
  final TextStyle style;
  final TextStyle codeStyle;
  final double blockSpacing;
  final double horizontalPadding;
  final double cacheExtent;
  final int windowBlockCount;
  final FlarkV3SourceGapBuilder sourceGapBuilder;

  /// Receives exact parser-authored link activation from passive rows only.
  ///
  /// The annotation carries the parser-semantic destination, not an
  /// HTML-escaped or platform-normalized URI. The callback owns validation
  /// and context-appropriate encoding before navigation.
  ///
  /// When null, links remain styled but their taps retain ordinary row
  /// activation and enter editing. The active [EditableText] never invokes
  /// this callback.
  final ValueChanged<FlarkV3InlineLinkAnnotation>? onLinkActivated;

  /// Resolves parser-certified passive images into application visuals.
  ///
  /// The builder receives marker-free alt text and parser-cooked values. When
  /// null, Flark paints a deterministic labelled fallback and performs no I/O.
  final FlarkV3InlineImageBuilder? inlineImageBuilder;
  final FlarkV3ActiveEditorBuilder _activeEditorBuilder;
  final Listenable? _activePresentationProgress;
  final FlarkV3ActivePresentationReadiness? _activePresentationReadiness;

  @override
  State<FlarkV3VirtualizedLiveSurface> createState() =>
      _FlarkV3VirtualizedLiveSurfaceState();
}

final class _FlarkV3VirtualizedLiveSurfaceState
    extends State<FlarkV3VirtualizedLiveSurface> {
  final Map<int, LayerLink> _rowLinks = <int, LayerLink>{};
  final Map<int, GlobalKey> _rowKeys = <int, GlobalKey>{};
  final Map<int, _CachedPassiveRow> _passiveRows = <int, _CachedPassiveRow>{};
  final Set<int> _mountedPresentationOrdinals = <int>{};
  final Map<int, int> _passiveBuildCounts = <int, int>{};
  List<FlarkV3ParserAuthoredBlockPresentation>? _lastJoinedPage;

  ScrollController? _ownedScrollController;
  FlarkV3ViewportWindowDemand? _lastWindowDemand;
  int? _pendingRevealOrdinal;
  int? _lastPositionedActiveOrdinal;
  _StagedActivation? _stagedActivation;
  double _activeExtent = 44;
  bool _positionScheduled = false;
  bool _focusScheduled = false;

  ScrollController get _scrollController =>
      widget.scrollController ?? _ownedScrollController!;

  @override
  void initState() {
    super.initState();
    if (widget.scrollController == null) {
      _ownedScrollController = ScrollController();
    }
    widget.presentationSource.addListener(_handleAuthorityChange);
    widget.visibleBlockCoordinator.addListener(_handleAuthorityChange);
    widget._activePresentationProgress?.addListener(
      _handleActivePresentationProgress,
    );
    _attachDebugController(widget.controller);
    _scheduleActivePosition();
  }

  @override
  void didUpdateWidget(FlarkV3VirtualizedLiveSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.presentationSource, widget.presentationSource)) {
      oldWidget.presentationSource.removeListener(_handleAuthorityChange);
      widget.presentationSource.addListener(_handleAuthorityChange);
      _clearPresentationCache();
    }
    if (!identical(
      oldWidget.visibleBlockCoordinator,
      widget.visibleBlockCoordinator,
    )) {
      oldWidget.visibleBlockCoordinator.removeListener(_handleAuthorityChange);
      widget.visibleBlockCoordinator.addListener(_handleAuthorityChange);
      _clearPresentationCache();
    }
    if (!identical(
      oldWidget._activePresentationProgress,
      widget._activePresentationProgress,
    )) {
      oldWidget._activePresentationProgress?.removeListener(
        _handleActivePresentationProgress,
      );
      widget._activePresentationProgress?.addListener(
        _handleActivePresentationProgress,
      );
      _stagedActivation = null;
    }
    if (!identical(oldWidget.scrollController, widget.scrollController)) {
      _ownedScrollController?.dispose();
      _ownedScrollController = widget.scrollController == null
          ? ScrollController()
          : null;
    }
    if (!identical(oldWidget.controller, widget.controller)) {
      _detachDebugController(oldWidget.controller);
      _attachDebugController(widget.controller);
    }
    _scheduleActivePosition();
  }

  void _attachDebugController(
    FlarkV3VirtualizedLiveSurfaceController? controller,
  ) {
    final previous = controller?._state;
    if (previous != null && !identical(previous, this)) {
      throw StateError(
        'A virtualized surface controller cannot attach to two surfaces.',
      );
    }
    controller?._state = this;
  }

  void _detachDebugController(
    FlarkV3VirtualizedLiveSurfaceController? controller,
  ) {
    if (identical(controller?._state, this)) controller?._state = null;
  }

  void _handleAuthorityChange() {
    if (!mounted) return;
    final activationCompleted = _completeStagedActivationIfReady();
    setState(() {});
    if (activationCompleted) _requestEditorFocus();
    _scheduleActivePosition();
  }

  void _handleActivePresentationProgress() {
    if (!mounted || _stagedActivation == null) return;
    final activationCompleted = _completeStagedActivationIfReady();
    setState(() {});
    if (activationCompleted) {
      _requestEditorFocus();
      _scheduleActivePosition();
    }
  }

  void _clearPresentationCache() {
    _passiveRows.clear();
    _rowKeys.clear();
    _rowLinks.clear();
    _lastJoinedPage = null;
  }

  @override
  void dispose() {
    widget.presentationSource.removeListener(_handleAuthorityChange);
    widget.visibleBlockCoordinator.removeListener(_handleAuthorityChange);
    widget._activePresentationProgress?.removeListener(
      _handleActivePresentationProgress,
    );
    _detachDebugController(widget.controller);
    _ownedScrollController?.dispose();
    super.dispose();
  }

  void _scheduleActivePosition() {
    if (_positionScheduled) return;
    _positionScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _positionScheduled = false;
      if (!mounted) return;
      final snapshot = widget.presentationSource.snapshot;
      final target = _pendingRevealOrdinal ?? snapshot.activeOrdinal;
      if (_pendingRevealOrdinal == null &&
          _lastPositionedActiveOrdinal == target) {
        return;
      }
      final context = _rowKeys[target]?.currentContext;
      if (context != null) {
        if (_rowIntersectsViewport(context)) {
          _lastPositionedActiveOrdinal = target;
          if (_pendingRevealOrdinal == target) _pendingRevealOrdinal = null;
          return;
        }
        Scrollable.ensureVisible(
          context,
          alignment: 0.5,
          duration: Duration.zero,
        );
        _lastPositionedActiveOrdinal = target;
        if (_pendingRevealOrdinal == target) _pendingRevealOrdinal = null;
        return;
      }
      if (!_scrollController.hasClients) return;
      final estimate = snapshot.estimatedBlockExtent;
      final targetOffset =
          (target * estimate -
                  _scrollController.position.viewportDimension * 0.5)
              .clamp(0.0, _scrollController.position.maxScrollExtent);
      _scrollController.jumpTo(targetOffset);
      _lastPositionedActiveOrdinal = target;
    });
  }

  bool _rowIntersectsViewport(BuildContext rowContext) {
    final viewport = context.findRenderObject();
    final row = rowContext.findRenderObject();
    if (viewport is! RenderBox ||
        row is! RenderBox ||
        !viewport.hasSize ||
        !row.attached ||
        !row.hasSize) {
      return false;
    }
    final rowRect =
        row.localToGlobal(Offset.zero, ancestor: viewport) & row.size;
    return rowRect.overlaps(Offset.zero & viewport.size);
  }

  void _revealAndActivateOrdinal(int ordinal) {
    final snapshot = widget.presentationSource.snapshot;
    if (ordinal < 0 || ordinal >= snapshot.totalBlockCount) {
      throw RangeError.index(ordinal, snapshot, 'ordinal');
    }
    final activatingDifferentOrdinal = ordinal != snapshot.activeOrdinal;
    _pendingRevealOrdinal = ordinal;
    widget.presentationSource.requestWindow(
      FlarkV3ViewportWindowDemand(
        centerOrdinal: ordinal,
        maximumBlocks: widget.windowBlockCount,
      ),
    );
    final target = _exactPresentationForOrdinal(ordinal);
    if (activatingDifferentOrdinal &&
        widget._activePresentationReadiness != null &&
        (target == null || target.isAuthoritative)) {
      _stagedActivation = _StagedActivation(ordinal);
      _adoptStagedTargetExtent(ordinal);
    }
    try {
      widget.presentationSource.activateOrdinal(ordinal);
    } catch (_) {
      if (_stagedActivation?.ordinal == ordinal) _stagedActivation = null;
      rethrow;
    }
    final activationCompleted = _completeStagedActivationIfReady();
    if (_stagedActivation == null || activationCompleted) {
      _requestEditorFocus();
    }
    _scheduleActivePosition();
  }

  FlarkV3ParserAuthoredBlockPresentation? _exactPresentationForOrdinal(
    int ordinal,
  ) {
    final snapshot = widget.presentationSource.snapshot;
    if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot) return null;
    for (final block in snapshot.blocks) {
      if (block.ordinal == ordinal) return block;
    }
    return null;
  }

  bool _completeStagedActivationIfReady() {
    final staged = _stagedActivation;
    final readiness = widget._activePresentationReadiness;
    if (staged == null || readiness == null) return false;
    final snapshot = widget.presentationSource.snapshot;
    if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot ||
        snapshot.activeOrdinal != staged.ordinal) {
      return false;
    }
    final target = _exactPresentationForOrdinal(staged.ordinal);
    if (target == null) return false;
    if (!target.isAuthoritative || readiness(target)) {
      _stagedActivation = null;
      return true;
    }
    return false;
  }

  void _adoptStagedTargetExtent(int ordinal) {
    final renderObject = _rowKeys[ordinal]?.currentContext?.findRenderObject();
    if (renderObject is! RenderBox ||
        !renderObject.attached ||
        !renderObject.hasSize) {
      return;
    }
    final extent = renderObject.size.height - widget.blockSpacing;
    if (extent > 0 && extent.isFinite) _activeExtent = extent;
  }

  void _requestEditorFocus() {
    if (widget.focusNode?.hasFocus == true || _focusScheduled) return;
    _focusScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        _focusScheduled = false;
        return;
      }
      // Active positioning also completes after the activation frame. Give
      // its zero-duration scroll one frame to composite before asking the web
      // text client to reveal the caret at the new row.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _focusScheduled = false;
        if (!mounted) return;
        final focusNode = widget.focusNode;
        if (focusNode != null) {
          if (focusNode.canRequestFocus && !focusNode.hasFocus) {
            focusNode.requestFocus();
          }
          return;
        }
        final editableKey = widget.editableKey;
        if (editableKey is GlobalKey<EditableTextState>) {
          editableKey.currentState?.requestKeyboard();
        }
      });
      WidgetsBinding.instance.scheduleFrame();
    });
  }

  bool _handleScrollNotification(ScrollNotification notification) {
    if (notification is! ScrollUpdateNotification &&
        notification is! ScrollEndNotification) {
      return false;
    }
    final snapshot = widget.presentationSource.snapshot;
    final centerOrdinal =
        _nearestMountedOrdinalToViewportCenter() ??
        ((notification.metrics.pixels +
                    notification.metrics.viewportDimension * 0.5) /
                snapshot.estimatedBlockExtent)
            .floor()
            .clamp(0, snapshot.totalBlockCount - 1);
    final demand = FlarkV3ViewportWindowDemand(
      centerOrdinal: centerOrdinal,
      maximumBlocks: widget.windowBlockCount,
    );
    if (demand != _lastWindowDemand) {
      _lastWindowDemand = demand;
      widget.presentationSource.requestWindow(demand);
    }
    return false;
  }

  int? _nearestMountedOrdinalToViewportCenter() {
    final surface = context.findRenderObject();
    if (surface is! RenderBox || !surface.hasSize) return null;
    final viewportCenter = surface.size.height * 0.5;
    int? nearestOrdinal;
    var nearestDistance = double.infinity;
    for (final entry in _rowKeys.entries) {
      final row = entry.value.currentContext?.findRenderObject();
      if (row is! RenderBox || !row.attached || !row.hasSize) continue;
      final top = row.localToGlobal(Offset.zero, ancestor: surface).dy;
      final distance = (top + row.size.height * 0.5 - viewportCenter).abs();
      if (distance < nearestDistance) {
        nearestDistance = distance;
        nearestOrdinal = entry.key;
      }
    }
    return nearestOrdinal;
  }

  @override
  Widget build(BuildContext context) {
    final snapshot = widget.presentationSource.snapshot;
    _pruneCaches(snapshot);
    final exactJoined = _joinExactPage(snapshot);
    if (exactJoined != null) _lastJoinedPage = exactJoined;
    final retained = _lastJoinedPage;
    final joined =
        exactJoined ??
        (retained != null &&
                retained.any((block) => block.ordinal == snapshot.activeOrdinal)
            ? retained
            : null);
    final activeLink = _rowLinks.putIfAbsent(
      snapshot.activeOrdinal,
      LayerLink.new,
    );

    return LayoutBuilder(
      builder: (context, constraints) {
        return Stack(
          clipBehavior: Clip.none,
          children: [
            Positioned.fill(
              child: NotificationListener<ScrollNotification>(
                onNotification: _handleScrollNotification,
                child: CustomScrollView(
                  controller: _scrollController,
                  scrollCacheExtent: ScrollCacheExtent.pixels(
                    widget.cacheExtent,
                  ),
                  slivers: _buildSlivers(
                    snapshot,
                    joined,
                    passiveAuthorityAvailable: exactJoined != null,
                  ),
                ),
              ),
            ),
            Positioned(
              left: 0,
              top: 0,
              child: CompositedTransformFollower(
                link: activeLink,
                showWhenUnlinked: false,
                targetAnchor: Alignment.topLeft,
                followerAnchor: Alignment.topLeft,
                child: SizedBox(
                  width: constraints.maxWidth,
                  child: Padding(
                    padding: EdgeInsets.symmetric(
                      horizontal: widget.horizontalPadding,
                    ),
                    child: _ActiveExtentReporter(
                      onExtent: _adoptActiveExtent,
                      child: _ActiveVisualGate(
                        hidden: _stagedActivation != null,
                        child: widget._activeEditorBuilder(context),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        );
      },
    );
  }

  List<Widget> _buildSlivers(
    FlarkV3ViewportSurfaceSnapshot snapshot,
    List<FlarkV3ParserAuthoredBlockPresentation>? joined, {
    required bool passiveAuthorityAvailable,
  }) {
    if (joined == null) {
      final activeOffset =
          snapshot.activeOrdinal * snapshot.estimatedBlockExtent;
      final trailingBlocks =
          snapshot.totalBlockCount - snapshot.activeOrdinal - 1;
      return [
        SliverToBoxAdapter(child: SizedBox(height: activeOffset)),
        SliverToBoxAdapter(
          child: CompositedTransformTarget(
            link: _rowLinks.putIfAbsent(snapshot.activeOrdinal, LayerLink.new),
            child: SizedBox(
              key: _rowKeys.putIfAbsent(snapshot.activeOrdinal, GlobalKey.new),
              height: _activeExtent + widget.blockSpacing,
              child: widget.sourceGapBuilder(context, snapshot),
            ),
          ),
        ),
        SliverToBoxAdapter(
          child: SizedBox(
            height: trailingBlocks * snapshot.estimatedBlockExtent,
          ),
        ),
      ];
    }

    final firstOrdinal = joined.first.ordinal;
    final lastOrdinal = joined.last.ordinal;
    final leadingExtent = firstOrdinal * snapshot.estimatedBlockExtent;
    final trailingExtent =
        (snapshot.totalBlockCount - lastOrdinal - 1) *
        snapshot.estimatedBlockExtent;
    final activeOrdinal = snapshot.activeOrdinal;
    final stagedActiveOrdinal = _stagedActivation?.ordinal;

    return [
      SliverToBoxAdapter(child: SizedBox(height: leadingExtent)),
      SliverList.builder(
        itemCount: joined.length,
        itemBuilder: (context, index) {
          final presentation = joined[index];
          final ordinal = presentation.ordinal;
          final rowKey = _rowKeys.putIfAbsent(ordinal, GlobalKey.new);
          if (ordinal == activeOrdinal) {
            if (ordinal == stagedActiveOrdinal) {
              return KeyedSubtree(
                key: rowKey,
                child: CompositedTransformTarget(
                  link: _rowLinks.putIfAbsent(ordinal, LayerLink.new),
                  child: _PassiveAuthorityGate(
                    authorityAvailable: false,
                    child: _cachedPassiveRow(presentation),
                  ),
                ),
              );
            }
            return KeyedSubtree(
              key: rowKey,
              child: CompositedTransformTarget(
                link: _rowLinks.putIfAbsent(ordinal, LayerLink.new),
                child: SizedBox(height: _activeExtent + widget.blockSpacing),
              ),
            );
          }
          return KeyedSubtree(
            key: rowKey,
            child: CompositedTransformTarget(
              link: _rowLinks.putIfAbsent(ordinal, LayerLink.new),
              child: _PassiveAuthorityGate(
                authorityAvailable: passiveAuthorityAvailable,
                child: _cachedPassiveRow(presentation),
              ),
            ),
          );
        },
      ),
      SliverToBoxAdapter(child: SizedBox(height: trailingExtent)),
    ];
  }

  List<FlarkV3ParserAuthoredBlockPresentation>? _joinExactPage(
    FlarkV3ViewportSurfaceSnapshot snapshot,
  ) {
    if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot) return null;
    if (snapshot.blocks.every((block) => block.recursiveGreenRow != null)) {
      final ack = snapshot.blocks.first.recursiveGreenStructuralAck;
      if (ack == null ||
          snapshot.blocks.any(
            (block) =>
                block.identity != snapshot.identity ||
                block.recursiveGreenStructuralAck != ack,
          )) {
        return null;
      }
      return snapshot.blocks;
    }
    final structural = widget.visibleBlockCoordinator.exactValue;
    final phase = widget.visibleBlockCoordinator.phase;
    if (structural == null ||
        phase != FlarkV3FlutterVisibleBlockPhase.exact &&
            phase != FlarkV3FlutterVisibleBlockPhase.truncated ||
        structural.demand.sourceRevision !=
            snapshot.identity.sourceVersion.revision ||
        structural.demand.structureGeneration !=
            snapshot.identity.structureGeneration) {
      return null;
    }
    final structures = {
      for (final block in structural.blocks) block.ordinal: block,
    };
    for (final block in snapshot.blocks) {
      final structure = structures[block.ordinal];
      if (structure == null ||
          structure.structure.kind != block.kind ||
          !_sameSpan(structure.structure.source, block.physicalSource)) {
        return null;
      }
    }
    return snapshot.blocks;
  }

  Widget _cachedPassiveRow(
    FlarkV3ParserAuthoredBlockPresentation presentation,
  ) {
    final cached = _passiveRows[presentation.ordinal];
    if (cached != null &&
        _samePassivePaint(cached.presentation, presentation) &&
        cached.style == widget.style &&
        cached.codeStyle == widget.codeStyle &&
        cached.blockSpacing == widget.blockSpacing &&
        cached.horizontalPadding == widget.horizontalPadding &&
        cached.onLinkActivated == widget.onLinkActivated &&
        cached.inlineImageBuilder == widget.inlineImageBuilder) {
      if (!identical(cached.presentation, presentation)) {
        _passiveRows[presentation.ordinal] = _CachedPassiveRow(
          presentation: presentation,
          style: cached.style,
          codeStyle: cached.codeStyle,
          blockSpacing: cached.blockSpacing,
          horizontalPadding: cached.horizontalPadding,
          onLinkActivated: cached.onLinkActivated,
          inlineImageBuilder: cached.inlineImageBuilder,
          widget: cached.widget,
        );
      }
      return cached.widget;
    }
    final row = _FlarkV3PassivePresentationRow(
      key: ValueKey<Object>((presentation.identity, presentation.ordinal)),
      presentation: presentation,
      style: widget.style,
      codeStyle: widget.codeStyle,
      blockSpacing: widget.blockSpacing,
      horizontalPadding: widget.horizontalPadding,
      onTap: () => _revealAndActivateOrdinal(presentation.ordinal),
      onLinkActivated: widget.onLinkActivated == null
          ? null
          : (annotation) =>
                _activateExactPassiveLink(presentation.ordinal, annotation),
      inlineImageBuilder: widget.inlineImageBuilder,
      onMount: () {
        _mountedPresentationOrdinals.add(presentation.ordinal);
      },
      onUnmount: () {
        _mountedPresentationOrdinals.remove(presentation.ordinal);
      },
      onBuild: () {
        _passiveBuildCounts.update(
          presentation.ordinal,
          (count) => count + 1,
          ifAbsent: () => 1,
        );
      },
    );
    _passiveRows[presentation.ordinal] = _CachedPassiveRow(
      presentation: presentation,
      style: widget.style,
      codeStyle: widget.codeStyle,
      blockSpacing: widget.blockSpacing,
      horizontalPadding: widget.horizontalPadding,
      onLinkActivated: widget.onLinkActivated,
      inlineImageBuilder: widget.inlineImageBuilder,
      widget: row,
    );
    return row;
  }

  void _activateExactPassiveLink(
    int ordinal,
    FlarkV3InlineLinkAnnotation requested,
  ) {
    final callback = widget.onLinkActivated;
    final snapshot = widget.presentationSource.snapshot;
    if (callback == null ||
        snapshot is! FlarkV3ExactViewportSurfaceSnapshot ||
        snapshot.activeOrdinal == ordinal) {
      return;
    }
    final joined = _joinExactPage(snapshot);
    if (joined == null) return;
    FlarkV3ParserAuthoredBlockPresentation? current;
    for (final presentation in joined) {
      if (presentation.ordinal == ordinal) {
        current = presentation;
        break;
      }
    }
    if (current == null || !current.isAuthoritative) return;
    for (final run in current.runs) {
      final annotation = run.linkAnnotation;
      if (_sameLinkAnnotation(annotation, requested)) {
        callback(annotation!);
        return;
      }
    }
    for (final image in current.images) {
      final annotation = image.outerLink;
      if (_sameLinkAnnotation(annotation, requested)) {
        callback(annotation!);
        return;
      }
    }
  }

  void _pruneCaches(FlarkV3ViewportSurfaceSnapshot snapshot) {
    final retained = switch (snapshot) {
      FlarkV3ExactViewportSurfaceSnapshot(:final blocks) => {
        for (final block in blocks) block.ordinal,
        snapshot.activeOrdinal,
      },
      // A source gap paints no passive rows, so retaining the previous
      // bounded page cannot expose stale Markdown. Keeping at most the prior
      // 96 widgets lets an unchanged unrelated row survive the ordinary
      // edit -> pending -> exact authority transition without rebuilding.
      FlarkV3SourceGapViewportSurfaceSnapshot() => {
        ..._passiveRows.keys,
        snapshot.activeOrdinal,
      },
    };
    _passiveRows.removeWhere((ordinal, _) => !retained.contains(ordinal));
    _rowKeys.removeWhere((ordinal, _) => !retained.contains(ordinal));
    _rowLinks.removeWhere((ordinal, _) => !retained.contains(ordinal));
  }

  void _adoptActiveExtent(double extent) {
    if (_stagedActivation != null) return;
    final bounded = math.max(1.0, extent);
    if ((bounded - _activeExtent).abs() < 0.5 || !mounted) return;
    setState(() => _activeExtent = bounded);
  }
}

final class _StagedActivation {
  const _StagedActivation(this.ordinal);

  final int ordinal;
}

/// Keeps the one platform text client mounted and laid out while a passive row
/// covers an activation handoff.
final class _ActiveVisualGate extends StatelessWidget {
  const _ActiveVisualGate({required this.hidden, required this.child});

  final bool hidden;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ExcludeSemantics(
      excluding: hidden,
      child: IgnorePointer(
        ignoring: hidden,
        child: Opacity(
          key: const Key('flark-v3-active-visual-gate'),
          opacity: hidden ? 0 : 1,
          child: child,
        ),
      ),
    );
  }
}

final class _CachedPassiveRow {
  const _CachedPassiveRow({
    required this.presentation,
    required this.style,
    required this.codeStyle,
    required this.blockSpacing,
    required this.horizontalPadding,
    required this.onLinkActivated,
    required this.inlineImageBuilder,
    required this.widget,
  });

  final FlarkV3ParserAuthoredBlockPresentation presentation;
  final TextStyle style;
  final TextStyle codeStyle;
  final double blockSpacing;
  final double horizontalPadding;
  final ValueChanged<FlarkV3InlineLinkAnnotation>? onLinkActivated;
  final FlarkV3InlineImageBuilder? inlineImageBuilder;
  final Widget widget;
}

/// Keeps the last exact bounded passive row painted while current authority
/// catches up, without hit-testing or exposing stale semantics.
///
/// The stable topology is important: an unrelated row retains its element,
/// pixels, geometry, and RichText layout cache across the
/// edit -> gap -> exact transition. Authority-sensitive behavior remains
/// fail-closed until the replacement snapshot is exact.
final class _PassiveAuthorityGate extends StatelessWidget {
  const _PassiveAuthorityGate({
    required this.authorityAvailable,
    required this.child,
  });

  final bool authorityAvailable;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      ignoring: !authorityAvailable,
      child: ExcludeSemantics(excluding: !authorityAvailable, child: child),
    );
  }
}

final class _FlarkV3PassivePresentationRow extends StatefulWidget {
  const _FlarkV3PassivePresentationRow({
    super.key,
    required this.presentation,
    required this.style,
    required this.codeStyle,
    required this.blockSpacing,
    required this.horizontalPadding,
    required this.onTap,
    required this.onLinkActivated,
    required this.inlineImageBuilder,
    required this.onMount,
    required this.onUnmount,
    required this.onBuild,
  });

  final FlarkV3ParserAuthoredBlockPresentation presentation;
  final TextStyle style;
  final TextStyle codeStyle;
  final double blockSpacing;
  final double horizontalPadding;
  final VoidCallback onTap;
  final ValueChanged<FlarkV3InlineLinkAnnotation>? onLinkActivated;
  final FlarkV3InlineImageBuilder? inlineImageBuilder;
  final VoidCallback onMount;
  final VoidCallback onUnmount;
  final VoidCallback onBuild;

  @override
  State<_FlarkV3PassivePresentationRow> createState() =>
      _FlarkV3PassivePresentationRowState();
}

final class _FlarkV3PassivePresentationRowState
    extends State<_FlarkV3PassivePresentationRow> {
  List<TapGestureRecognizer?> _linkRecognizers =
      const <TapGestureRecognizer?>[];

  @override
  void initState() {
    super.initState();
    widget.onMount();
    _replaceLinkRecognizers();
  }

  @override
  void didUpdateWidget(_FlarkV3PassivePresentationRow oldWidget) {
    super.didUpdateWidget(oldWidget);
    _replaceLinkRecognizers();
  }

  @override
  void dispose() {
    _disposeLinkRecognizers();
    widget.onUnmount();
    super.dispose();
  }

  void _replaceLinkRecognizers() {
    _disposeLinkRecognizers();
    final callback = widget.onLinkActivated;
    _linkRecognizers = [
      for (final run in widget.presentation.runs)
        _linkRecognizer(run.linkAnnotation, callback),
    ];
  }

  TapGestureRecognizer? _linkRecognizer(
    FlarkV3InlineLinkAnnotation? annotation,
    ValueChanged<FlarkV3InlineLinkAnnotation>? callback,
  ) {
    if (annotation == null || callback == null) return null;
    return TapGestureRecognizer(debugOwner: this)
      ..onTap = () => callback(annotation);
  }

  void _disposeLinkRecognizers() {
    for (final recognizer in _linkRecognizers) {
      recognizer?.dispose();
    }
    _linkRecognizers = const <TapGestureRecognizer?>[];
  }

  @override
  Widget build(BuildContext context) {
    widget.onBuild();
    final presentation = widget.presentation;
    final isBlankBoundary =
        presentation.isAuthoritative &&
        presentation.kind == FlarkV3DocumentStructureKind.unknown &&
        presentation.displayText.isEmpty;
    final child = presentation.isAuthoritative
        ? _authoritativeBlock(context, presentation)
        : _unsupportedBlock(context);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: widget.onTap,
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          widget.horizontalPadding,
          0,
          widget.horizontalPadding,
          isBlankBoundary ? 0 : widget.blockSpacing,
        ),
        child: child,
      ),
    );
  }

  Widget _authoritativeBlock(
    BuildContext context,
    FlarkV3ParserAuthoredBlockPresentation presentation,
  ) {
    if (presentation.kind == FlarkV3DocumentStructureKind.unknown &&
        presentation.displayText.isEmpty) {
      return SizedBox.shrink(
        key: ValueKey<Object>((
          'flark-v3-blank-boundary',
          presentation.ordinal,
        )),
      );
    }
    if (presentation.kind == FlarkV3DocumentStructureKind.thematicBreak) {
      return Semantics(
        label: 'Thematic break',
        child: const SizedBox(
          height: 24,
          child: Center(
            child: DecoratedBox(
              decoration: BoxDecoration(color: Color(0x33000000)),
              child: SizedBox(width: double.infinity, height: 1),
            ),
          ),
        ),
      );
    }
    final baseStyle = switch (presentation.kind) {
      FlarkV3DocumentStructureKind.fencedCode ||
      FlarkV3DocumentStructureKind.indentedCode => widget.style.merge(
        widget.codeStyle,
      ),
      FlarkV3DocumentStructureKind.heading => _headingStyle(
        widget.style,
        presentation.headingLevel!,
      ),
      _ => widget.style,
    };
    final richText = RichText(
      key: ValueKey<Object>(('flark-v3-passive-text', presentation.ordinal)),
      text: TextSpan(
        style: baseStyle,
        children: _passiveSpans(context, presentation, baseStyle),
      ),
    );
    final greenRow = presentation.recursiveGreenRow;
    if (greenRow != null) {
      final leaf = switch (presentation.kind) {
        FlarkV3DocumentStructureKind.fencedCode ||
        FlarkV3DocumentStructureKind.indentedCode => FlarkV3CodeBlockChrome(
          key: ValueKey<Object>((
            'flark-v3-passive-code-block-chrome',
            presentation.ordinal,
          )),
          active: true,
          child: richText,
        ),
        _ => richText,
      };
      return FlarkV3RecursiveGreenContainerChrome(
        key: ValueKey<Object>((
          'flark-v3-passive-green-chrome',
          presentation.ordinal,
        )),
        path: greenRow.path,
        textStyle: baseStyle,
        child: leaf,
      );
    }
    return switch (presentation.kind) {
      FlarkV3DocumentStructureKind.fencedCode ||
      FlarkV3DocumentStructureKind.indentedCode => FlarkV3CodeBlockChrome(
        key: ValueKey<Object>((
          'flark-v3-passive-code-block-chrome',
          presentation.ordinal,
        )),
        active: true,
        child: richText,
      ),
      FlarkV3DocumentStructureKind.blockQuote => DecoratedBox(
        decoration: const BoxDecoration(
          border: Border(left: BorderSide(color: Color(0xFFCBD5E1), width: 3)),
        ),
        child: Padding(
          padding: const EdgeInsets.only(left: 12),
          child: richText,
        ),
      ),
      FlarkV3DocumentStructureKind.bulletList ||
      FlarkV3DocumentStructureKind.orderedList => Padding(
        padding: const EdgeInsets.only(left: 20),
        child: richText,
      ),
      _ => richText,
    };
  }

  List<InlineSpan> _passiveSpans(
    BuildContext context,
    FlarkV3ParserAuthoredBlockPresentation presentation,
    TextStyle baseStyle,
  ) {
    final spans = <InlineSpan>[];
    var runIndex = 0;
    var imageIndex = 0;
    var cursor = 0;

    while (cursor < presentation.displayText.length ||
        imageIndex < presentation.images.length) {
      final image = imageIndex < presentation.images.length
          ? presentation.images[imageIndex]
          : null;
      if (image != null && image.startUtf16 == cursor) {
        spans.add(_passiveImageSpan(context, presentation, image, baseStyle));
        imageIndex += 1;
        cursor = image.endUtf16;
        while (runIndex < presentation.runs.length &&
            presentation.runs[runIndex].endUtf16 <= cursor) {
          runIndex += 1;
        }
        continue;
      }
      if (runIndex >= presentation.runs.length) {
        throw StateError('Passive image geometry escaped the display runs.');
      }
      final run = presentation.runs[runIndex];
      if (cursor < run.startUtf16 || cursor >= run.endUtf16) {
        throw StateError(
          'Passive span cursor escaped its parser-authored run.',
        );
      }
      final end = image != null && image.startUtf16 < run.endUtf16
          ? image.startUtf16
          : run.endUtf16;
      if (end <= cursor) {
        throw StateError('Passive image geometry does not make progress.');
      }
      final recognizer = _linkRecognizers[runIndex];
      spans.add(
        TextSpan(
          semanticsIdentifier: recognizer == null
              ? null
              : _passiveLinkSemanticsIdentifier(presentation.ordinal, cursor),
          text: presentation.displayText.substring(cursor, end),
          style: _inlineStyle(
            baseStyle,
            run.styles,
            isLink: run.linkAnnotation != null,
          ),
          recognizer: recognizer,
        ),
      );
      cursor = end;
      if (cursor == run.endUtf16) runIndex += 1;
    }
    return spans;
  }

  InlineSpan _passiveImageSpan(
    BuildContext context,
    FlarkV3ParserAuthoredBlockPresentation presentation,
    FlarkV3PassiveInlineImage image,
    TextStyle baseStyle,
  ) {
    final alt = presentation.displayText.substring(
      image.startUtf16,
      image.endUtf16,
    );
    final link = image.outerLink;
    final callback = widget.onLinkActivated;
    final activate = link == null || callback == null
        ? null
        : () => callback(link);
    final spec = FlarkV3InlineImageSpec(
      annotation: image.annotation,
      alt: alt,
      outerLink: link,
      constraints: FlarkV3InlineImage.inlineConstraints,
    );
    Widget child = FlarkV3InlineImage(
      key: ValueKey<Object>((
        'flark-v3-passive-image',
        presentation.ordinal,
        image.annotation.source.startUtf16,
      )),
      spec: spec,
      builder: widget.inlineImageBuilder,
      style: baseStyle,
    );
    if (activate != null) {
      child = GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: activate,
        child: child,
      );
    }
    return WidgetSpan(
      alignment: PlaceholderAlignment.middle,
      child: Semantics(
        container: true,
        excludeSemantics: true,
        image: true,
        link: activate != null,
        identifier: _passiveImageSemanticsIdentifier(
          presentation.ordinal,
          image.annotation.source.startUtf16,
        ),
        label: alt.isEmpty ? 'Image' : alt,
        value: image.annotation.destination,
        onTap: activate,
        child: child,
      ),
    );
  }

  Widget _unsupportedBlock(BuildContext context) => Semantics(
    label: 'Markdown block unavailable',
    child: SizedBox(
      height: widget.presentation.estimatedExtent,
      child: const Align(
        alignment: Alignment.centerLeft,
        child: DecoratedBox(
          decoration: BoxDecoration(color: Color(0x0A000000)),
          child: SizedBox(width: 120, height: 12),
        ),
      ),
    ),
  );
}

final class _ActiveExtentReporter extends SingleChildRenderObjectWidget {
  const _ActiveExtentReporter({required this.onExtent, required super.child});

  final ValueChanged<double> onExtent;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _ActiveExtentRenderObject(onExtent);

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _ActiveExtentRenderObject renderObject,
  ) {
    renderObject.onExtent = onExtent;
  }
}

final class _ActiveExtentRenderObject extends RenderProxyBox {
  _ActiveExtentRenderObject(this.onExtent);

  ValueChanged<double> onExtent;
  double? _reportedExtent;

  @override
  void performLayout() {
    super.performLayout();
    final next = size.height;
    if (_reportedExtent == next) return;
    _reportedExtent = next;
    WidgetsBinding.instance.addPostFrameCallback((_) => onExtent(next));
  }
}

Widget _defaultSourceGapBuilder(
  BuildContext context,
  FlarkV3ViewportSurfaceSnapshot snapshot,
) => Semantics(
  liveRegion: true,
  label: 'Markdown rendering is catching up',
  child: const SizedBox(
    height: 36,
    child: Align(
      alignment: Alignment.centerLeft,
      child: DecoratedBox(
        decoration: BoxDecoration(color: Color(0x0A000000)),
        child: SizedBox(width: 180, height: 12),
      ),
    ),
  ),
);

TextStyle _headingStyle(TextStyle base, int level) {
  final scale = switch (level) {
    1 => 2.0,
    2 => 1.5,
    3 => 1.25,
    4 => 1.0,
    5 => 0.875,
    6 => 0.85,
    _ => throw RangeError.range(level, 1, 6, 'level'),
  };
  return base.copyWith(
    fontSize: (base.fontSize ?? 16) * scale,
    fontWeight: FontWeight.w700,
  );
}

TextStyle _inlineStyle(
  TextStyle base,
  Set<FlarkV3InlineFactKind> styles, {
  required bool isLink,
}) {
  var result = base;
  for (final style in styles) {
    result = switch (style) {
      FlarkV3InlineFactKind.emphasis => result.copyWith(
        fontStyle: FontStyle.italic,
      ),
      FlarkV3InlineFactKind.strong => result.copyWith(
        fontWeight: FontWeight.w700,
      ),
      FlarkV3InlineFactKind.code => result.copyWith(
        fontFamily: 'monospace',
        backgroundColor: const Color(0x12000000),
      ),
      FlarkV3InlineFactKind.strikethrough => result.copyWith(
        decoration: result.decoration == null
            ? TextDecoration.lineThrough
            : TextDecoration.combine([
                result.decoration!,
                TextDecoration.lineThrough,
              ]),
      ),
      FlarkV3InlineFactKind.autolinkUri ||
      FlarkV3InlineFactKind.autolinkEmail ||
      FlarkV3InlineFactKind.escapedPunctuation ||
      FlarkV3InlineFactKind.hardLineBreak ||
      FlarkV3InlineFactKind.characterReference ||
      FlarkV3InlineFactKind.directLink ||
      FlarkV3InlineFactKind.directImage ||
      FlarkV3InlineFactKind.referenceLink ||
      FlarkV3InlineFactKind.referenceImage => result,
    };
  }
  if (isLink) {
    result = _withInlineLinkStyle(result);
  }
  return result;
}

TextStyle _withInlineLinkStyle(TextStyle style) =>
    _withInlineDecoration(style, TextDecoration.underline);

TextStyle _withInlineDecoration(TextStyle style, TextDecoration decoration) =>
    style.copyWith(
      decoration: style.decoration == null
          ? decoration
          : TextDecoration.combine([style.decoration!, decoration]),
    );

String _passiveLinkSemanticsIdentifier(int ordinal, int startUtf16) =>
    'flark-v3-passive-link-$ordinal-$startUtf16';

String _passiveImageSemanticsIdentifier(int ordinal, int sourceStartUtf16) =>
    'flark-v3-passive-image-$ordinal-$sourceStartUtf16';

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _samePassivePaint(
  FlarkV3ParserAuthoredBlockPresentation left,
  FlarkV3ParserAuthoredBlockPresentation right,
) {
  if (left.ordinal != right.ordinal ||
      left.kind != right.kind ||
      left.disposition != right.disposition ||
      left.displayText != right.displayText ||
      left.headingLevel != right.headingLevel ||
      left.fallbackReason != right.fallbackReason ||
      left.unsupportedReason != right.unsupportedReason ||
      left.estimatedExtent != right.estimatedExtent ||
      !_sameRecursiveGreenPaint(
        left.recursiveGreenRow,
        right.recursiveGreenRow,
      ) ||
      left.runs.length != right.runs.length ||
      left.images.length != right.images.length) {
    return false;
  }
  for (var index = 0; index < left.runs.length; index += 1) {
    final leftRun = left.runs[index];
    final rightRun = right.runs[index];
    if (leftRun.startUtf16 != rightRun.startUtf16 ||
        leftRun.endUtf16 != rightRun.endUtf16 ||
        !setEquals(leftRun.styles, rightRun.styles) ||
        !_sameLinkAnnotation(leftRun.linkAnnotation, rightRun.linkAnnotation)) {
      return false;
    }
  }
  for (var index = 0; index < left.images.length; index += 1) {
    final leftImage = left.images[index];
    final rightImage = right.images[index];
    if (leftImage.startUtf16 != rightImage.startUtf16 ||
        leftImage.endUtf16 != rightImage.endUtf16 ||
        !_sameImageAnnotation(leftImage.annotation, rightImage.annotation) ||
        !_sameLinkAnnotation(leftImage.outerLink, rightImage.outerLink)) {
      return false;
    }
  }
  return true;
}

bool _sameRecursiveGreenPaint(
  FlarkV3RecursiveGreenRenderableRow? left,
  FlarkV3RecursiveGreenRenderableRow? right,
) {
  if (identical(left, right)) return true;
  if (left == null || right == null || left.path.length != right.path.length) {
    return false;
  }
  for (var index = 0; index < left.path.length; index += 1) {
    final leftFrame = left.path[index];
    final rightFrame = right.path[index];
    if (leftFrame.kind != rightFrame.kind ||
        leftFrame.isRowOwner != rightFrame.isRowOwner ||
        leftFrame.isContainer != rightFrame.isContainer ||
        leftFrame.hasOpenFact != rightFrame.hasOpenFact ||
        leftFrame.hasCloseFact != rightFrame.hasCloseFact ||
        !_sameRecursiveGreenPathFact(leftFrame.fact, rightFrame.fact)) {
      return false;
    }
  }
  return true;
}

bool _sameRecursiveGreenPathFact(
  FlarkV3RecursiveGreenPathFact? left,
  FlarkV3RecursiveGreenPathFact? right,
) => switch ((left, right)) {
  (null, null) => true,
  (
    FlarkV3RecursiveGreenListPathFact left,
    FlarkV3RecursiveGreenListPathFact right,
  ) =>
    left.style == right.style &&
        left.bulletMarker == right.bulletMarker &&
        left.orderedDelimiter == right.orderedDelimiter &&
        left.start == right.start &&
        left.tight == right.tight,
  (
    FlarkV3RecursiveGreenItemPathFact left,
    FlarkV3RecursiveGreenItemPathFact right,
  ) =>
    left.markerOffset == right.markerOffset && left.padding == right.padding,
  (
    FlarkV3RecursiveGreenHeadingPathFact left,
    FlarkV3RecursiveGreenHeadingPathFact right,
  ) =>
    left.level == right.level && left.style == right.style,
  (
    FlarkV3RecursiveGreenCodePathFact left,
    FlarkV3RecursiveGreenCodePathFact right,
  ) =>
    left.marker == right.marker &&
        left.fenceOffsetColumns == right.fenceOffsetColumns &&
        left.minimumClosingLength == right.minimumClosingLength,
  (
    FlarkV3RecursiveGreenHtmlPathFact left,
    FlarkV3RecursiveGreenHtmlPathFact right,
  ) =>
    left.blockType == right.blockType,
  _ => false,
};

bool _sameLinkAnnotation(
  FlarkV3InlineLinkAnnotation? left,
  FlarkV3InlineLinkAnnotation? right,
) {
  if (identical(left, right)) return true;
  if (left == null || right == null) return false;
  return left.kind == right.kind &&
      left.targetRecipe == right.targetRecipe &&
      left.destination == right.destination &&
      _sameSpan(left.source, right.source) &&
      _sameSpan(left.content, right.content) &&
      _sameSpan(left.destinationSource, right.destinationSource) &&
      left.title == right.title &&
      _sameNullableSpan(left.titleSource, right.titleSource);
}

bool _sameImageAnnotation(
  FlarkV3InlineImageAnnotation left,
  FlarkV3InlineImageAnnotation right,
) =>
    identical(left, right) ||
    left.destination == right.destination &&
        _sameSpan(left.source, right.source) &&
        _sameSpan(left.content, right.content) &&
        _sameSpan(left.destinationSource, right.destinationSource) &&
        left.title == right.title &&
        _sameNullableSpan(left.titleSource, right.titleSource);

bool _sameNullableSpan(FlarkV3SourceSpan? left, FlarkV3SourceSpan? right) =>
    identical(left, right) ||
    left != null && right != null && _sameSpan(left, right);
