import '../host/flark_v3_host_protocol.dart';
import '../runtime/public/flark_v3_document_query.dart';
import '../runtime/public/flark_v3_inline_facts.dart';
import '../source/flark_v3_source_document.dart';
import 'flark_v3_source_projection.dart';

/// Whether parser-certified inline markers remain in the display text.
///
/// This is deliberately an explicit presentation policy. The projection never
/// recognizes Markdown itself: [hideCertifiedMarkers] removes only the exact
/// opener and closer ranges carried by [FlarkV3InlineFacts].
enum FlarkV3InlineMarkerPolicy { allMarkersVisible, hideCertifiedMarkers }

/// Which exact source caret wins when hidden source shares one display caret.
///
/// A chain of adjacent certified markers collapses to one display offset.
/// [upstream] selects the chain's smallest source offset and [downstream]
/// selects its largest source offset. Away from a hidden-marker boundary both
/// affinities produce the same exact source offset.
enum FlarkV3InlineProjectionAffinity { upstream, downstream }

/// One immutable, source-backed display run.
///
/// Source coordinates are absolute document UTF-16 offsets. Display
/// coordinates are UTF-16 offsets relative to this projection's bounded leaf.
/// [semanticFacts] is the complete parser-preorder semantic stack, outermost
/// first. [semanticStyles] contains only style facts, while [linkAnnotation]
/// exposes parser-certified link semantics separately. Retaining complete
/// facts preserves certified projection recipes while [text] remains backed by
/// the exact source span or by an explicit parser-authored replacement piece.
final class FlarkV3InlineDisplayRun {
  const FlarkV3InlineDisplayRun._({
    required this.text,
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.displayStartUtf16,
    required this.displayEndUtf16,
    required this.semanticStack,
  });

  final String text;
  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final int displayStartUtf16;
  final int displayEndUtf16;

  /// Structurally shared semantic authority for this run.
  ///
  /// Capturing this stack is O(1), independent of nesting depth.
  final FlarkV3InlineSemanticStack semanticStack;

  /// Lazily materialized parser-preorder facts, outermost first.
  List<FlarkV3InlineFact> get semanticFacts => semanticStack.factsOuterToInner;

  /// Style fact kinds only, in the same outer-to-inner order.
  List<FlarkV3InlineFactKind> get semanticStyles =>
      semanticStack.kindsOuterToInner;

  /// The innermost parser-certified link active for this run, in O(1).
  ///
  /// A link nested inside an image alt label is intentionally suppressed. Its
  /// fact remains in [semanticFacts], but it must not become an action for the
  /// image's accessible label.
  FlarkV3InlineLinkAnnotation? get linkAnnotation =>
      semanticStack.linkAnnotation;

  /// The innermost parser-certified image active for this run, in O(1).
  FlarkV3InlineImageAnnotation? get imageAnnotation =>
      semanticStack.imageAnnotation;
}

/// Immutable persistent stack of complete parser-certified inline facts.
///
/// Each push allocates one node and each pop restores the existing parent.
/// Runs therefore share their semantic prefixes rather than copying a list at
/// every boundary. [factsInnerToOuter] is allocation-free apart from iterator
/// state; outer-to-inner list views are materialized only if a consumer asks.
final class FlarkV3InlineSemanticStack {
  const FlarkV3InlineSemanticStack._empty()
    : _fact = null,
      _parent = null,
      depth = 0,
      linkAnnotation = null,
      imageAnnotation = null,
      _insideImageAlt = false;

  FlarkV3InlineSemanticStack._push(
    FlarkV3InlineFact fact,
    FlarkV3InlineSemanticStack parent,
  ) : _fact = fact,
      _parent = parent,
      depth = parent.depth + 1,
      linkAnnotation = parent._insideImageAlt
          ? parent.linkAnnotation
          : fact.linkAnnotation ?? parent.linkAnnotation,
      imageAnnotation = fact.imageAnnotation ?? parent.imageAnnotation,
      _insideImageAlt =
          parent._insideImageAlt ||
          fact.kind == FlarkV3InlineFactKind.directImage ||
          fact.kind == FlarkV3InlineFactKind.referenceImage;

  static const empty = FlarkV3InlineSemanticStack._empty();

  final FlarkV3InlineFact? _fact;
  final FlarkV3InlineSemanticStack? _parent;

  final int depth;

  /// The innermost parser-certified link active at this stack point.
  ///
  /// Retaining this cached value avoids any scan proportional to semantic
  /// depth and applies the image-alt action-suppression rule on push.
  final FlarkV3InlineLinkAnnotation? linkAnnotation;

  /// The innermost parser-certified image active at this stack point.
  final FlarkV3InlineImageAnnotation? imageAnnotation;

  final bool _insideImageAlt;

  bool get isEmpty => depth == 0;

  Iterable<FlarkV3InlineFact> get factsInnerToOuter sync* {
    var cursor = this;
    while (!cursor.isEmpty) {
      yield cursor._fact!;
      cursor = cursor._parent!;
    }
  }

  List<FlarkV3InlineFact> get factsOuterToInner =>
      List<FlarkV3InlineFact>.unmodifiable(factsInnerToOuter.toList().reversed);

  List<FlarkV3InlineFactKind> get kindsOuterToInner =>
      List<FlarkV3InlineFactKind>.unmodifiable(
        factsOuterToInner
            .where((fact) => _isSemanticStyleKind(fact.kind))
            .map((fact) => fact.kind),
      );
}

/// Deterministic construction-work receipt for the bounded Dart seam.
///
/// [styleBoundaryComparisons] counts only the ordered start/end sweep. It is
/// bounded by fact events plus boundary visits rather than their product.
/// [semanticStackNodesAllocated] is at most the fact count; each emitted run
/// stores one O(1) [runStackReferencesStored] reference. The possibly
/// quadratic [logicalSemanticDepthSum] is a diagnostic only and never causes
/// that many fact references to be materialized during construction.
final class FlarkV3InlineProjectionWorkReceipt {
  const FlarkV3InlineProjectionWorkReceipt._({
    required this.markerSortComparisons,
    required this.boundaryPointsVisited,
    required this.boundaryIntervalsVisited,
    required this.styleBoundaryComparisons,
    required this.factStartEventsApplied,
    required this.factEndEventsApplied,
    required this.semanticStackNodesAllocated,
    required this.runStackReferencesStored,
    required this.logicalSemanticDepthSum,
    required this.sourceLeafReads,
    required this.sourceSlices,
  });

  final int markerSortComparisons;
  final int boundaryPointsVisited;
  final int boundaryIntervalsVisited;
  final int styleBoundaryComparisons;
  final int factStartEventsApplied;
  final int factEndEventsApplied;
  final int semanticStackNodesAllocated;
  final int runStackReferencesStored;
  final int logicalSemanticDepthSum;
  final int sourceLeafReads;
  final int sourceSlices;
}

/// A source-authority or invariant failure while constructing a projection.
///
/// This is not a Markdown parse error. The caller must discard the result and
/// retain an exact source-painted fallback.
final class FlarkV3InlineProjectionException implements Exception {
  const FlarkV3InlineProjectionException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3InlineProjectionException($message)';
}

/// One exact UTF-16 range carried by parser-certified delimiter topology.
///
/// This range deliberately has no Markdown semantics of its own. It remains
/// valid after provisional edits because it is shifted mechanically from the
/// parser-authored fact that minted it.
final class FlarkV3InlineUtf16Range {
  const FlarkV3InlineUtf16Range(this.startUtf16, this.endUtf16)
    : assert(startUtf16 >= 0),
      assert(endUtf16 >= startUtf16);

  final int startUtf16;
  final int endUtf16;

  int get lengthUtf16 => endUtf16 - startUtf16;
  bool get isCollapsed => startUtf16 == endUtf16;

  bool contains(FlarkV3InlineUtf16Range other) =>
      startUtf16 <= other.startUtf16 && other.endUtf16 <= endUtf16;

  bool intersects(int start, int end) =>
      !isCollapsed && startUtf16 < end && endUtf16 > start;

  FlarkV3InlineUtf16Range shift(int delta) =>
      FlarkV3InlineUtf16Range(startUtf16 + delta, endUtf16 + delta);
}

/// One parser-certified opening/content/closing delimiter pair.
///
/// [id] is the parser-preorder ordinal within this bounded leaf. [parentId]
/// records exact nesting and is null for a top-level pair. Dart never derives
/// a pair from source characters.
final class FlarkV3InlineDelimiterPair {
  const FlarkV3InlineDelimiterPair._({
    required this.id,
    required this.parentId,
    required this.kind,
    required this.source,
    required this.content,
    required this.opener,
    required this.closer,
  });

  final int id;
  final int? parentId;
  final FlarkV3InlineFactKind kind;
  final FlarkV3InlineUtf16Range source;
  final FlarkV3InlineUtf16Range content;
  final FlarkV3InlineUtf16Range opener;
  final FlarkV3InlineUtf16Range closer;

  FlarkV3InlineDelimiterPair _shift(int delta) => FlarkV3InlineDelimiterPair._(
    id: id,
    parentId: parentId,
    kind: kind,
    source: source.shift(delta),
    content: content.shift(delta),
    opener: opener.shift(delta),
    closer: closer.shift(delta),
  );

  FlarkV3InlineDelimiterPair _editInsideContent(int delta) =>
      FlarkV3InlineDelimiterPair._(
        id: id,
        parentId: parentId,
        kind: kind,
        source: FlarkV3InlineUtf16Range(
          source.startUtf16,
          source.endUtf16 + delta,
        ),
        content: FlarkV3InlineUtf16Range(
          content.startUtf16,
          content.endUtf16 + delta,
        ),
        opener: opener,
        closer: closer.shift(delta),
      );
}

/// Exact source deletion approved by parser-certified delimiter topology.
///
/// The range may be wider than the visible-text deletion because complete
/// opener/closer pairs whose content becomes empty are included atomically.
final class FlarkV3InlineDeletionPlan {
  FlarkV3InlineDeletionPlan._({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required List<int> removedPairIds,
  }) : removedPairIds = List<int>.unmodifiable(removedPairIds);

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final List<int> removedPairIds;

  bool get removesDelimiterPairs => removedPairIds.isNotEmpty;
}

/// One source edit normalized against parser-certified inline edit topology.
///
/// Ordinary paired delimiters retain their existing behavior: only a deletion
/// can expand over a pair it would orphan. Parser-certified atomic facts are
/// distinct source/display constructs, so every replacement intersecting their
/// visible content consumes the complete fact and an insertion at a collapsed
/// display edge is moved outside the source range.
final class FlarkV3InlineEditPlan {
  FlarkV3InlineEditPlan._({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.replacement,
    required Map<int, FlarkV3InlineFactKind> removedAtomicFacts,
    required List<int> removedPairedDelimiterFactIds,
    required Set<FlarkV3InlineFactKind> authorizedAtomicBoundaryKinds,
  }) : _removedAtomicFacts = Map<int, FlarkV3InlineFactKind>.unmodifiable(
         removedAtomicFacts,
       ),
       removedPairedDelimiterFactIds = List<int>.unmodifiable(
         removedPairedDelimiterFactIds,
       ),
       _authorizedAtomicBoundaryKinds = Set<FlarkV3InlineFactKind>.unmodifiable(
         authorizedAtomicBoundaryKinds,
       );

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final String replacement;
  final Map<int, FlarkV3InlineFactKind> _removedAtomicFacts;
  final List<int> removedPairedDelimiterFactIds;
  final Set<FlarkV3InlineFactKind> _authorizedAtomicBoundaryKinds;

  /// Parser fact ids for every atomic construct consumed by this edit.
  List<int> get removedAtomicFactIds =>
      List<int>.unmodifiable(_removedAtomicFacts.keys);

  /// Compatibility view for callers that distinguish escaped punctuation.
  List<int> get removedEscapedPunctuationFactIds =>
      _removedAtomicIdsOfKind(FlarkV3InlineFactKind.escapedPunctuation);

  List<int> get removedHardLineBreakFactIds =>
      _removedAtomicIdsOfKind(FlarkV3InlineFactKind.hardLineBreak);

  bool get removesAtomicInlineAtoms => _removedAtomicFacts.isNotEmpty;

  bool get removesEscapedPunctuationAtoms =>
      removedEscapedPunctuationFactIds.isNotEmpty;
  bool get removesHardLineBreakAtoms => removedHardLineBreakFactIds.isNotEmpty;
  bool get removesPairedDelimiters => removedPairedDelimiterFactIds.isNotEmpty;
  bool get removesCertifiedConstructs =>
      removesAtomicInlineAtoms || removesPairedDelimiters;

  /// Whether this insertion lands on a parser-certified atomic-fact boundary.
  ///
  /// That boundary can lie inside an adjacent merged hidden-marker chain, so a
  /// source-projection policy needs this typed authorization to distinguish the
  /// safe boundary from an arbitrary insertion inside hidden source.
  bool get authorizesAtomicBoundaryInsertion =>
      sourceStartUtf16 == sourceEndUtf16 &&
      replacement.isNotEmpty &&
      _authorizedAtomicBoundaryKinds.isNotEmpty;

  bool get authorizesEscapedPunctuationBoundaryInsertion =>
      authorizesAtomicBoundaryInsertion &&
      _authorizedAtomicBoundaryKinds.contains(
        FlarkV3InlineFactKind.escapedPunctuation,
      );

  bool get authorizesHardLineBreakBoundaryInsertion =>
      authorizesAtomicBoundaryInsertion &&
      _authorizedAtomicBoundaryKinds.contains(
        FlarkV3InlineFactKind.hardLineBreak,
      );

  FlarkV3SourceEdit get sourceEdit => FlarkV3SourceEdit(
    startUtf16: sourceStartUtf16,
    endUtf16: sourceEndUtf16,
    replacement: replacement,
  );

  List<int> _removedAtomicIdsOfKind(FlarkV3InlineFactKind kind) =>
      List<int>.unmodifiable([
        for (final entry in _removedAtomicFacts.entries)
          if (entry.value == kind) entry.key,
      ]);
}

/// Bounded paired-delimiter topology retained from exact parser facts.
///
/// This is editing metadata, not a Markdown recognizer. It can expand a
/// visible deletion over pairs that the deletion would orphan and can shift
/// the remaining topology mechanically while an authoritative reparse is
/// pending.
final class FlarkV3InlineDelimiterTopology {
  FlarkV3InlineDelimiterTopology._({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required List<FlarkV3InlineDelimiterPair> pairs,
  }) : pairs = List<FlarkV3InlineDelimiterPair>.unmodifiable(pairs);

  factory FlarkV3InlineDelimiterTopology._fromFacts(FlarkV3InlineFacts facts) {
    final pairs = <FlarkV3InlineDelimiterPair>[];
    final open = <FlarkV3InlineDelimiterPair>[];
    for (var index = 0; index < facts.facts.length; index += 1) {
      final fact = facts.facts[index];
      if (fact.linkAnnotation != null ||
          fact.imageAnnotation != null ||
          fact.kind == FlarkV3InlineFactKind.characterReference) {
        continue;
      }
      while (open.isNotEmpty &&
          fact.source.startUtf16 >= open.last.source.endUtf16) {
        open.removeLast();
      }
      final pair = FlarkV3InlineDelimiterPair._(
        id: index,
        parentId: open.isEmpty ? null : open.last.id,
        kind: fact.kind,
        source: _inlineRange(fact.source),
        content: _inlineRange(fact.content),
        opener: _inlineRange(fact.opener),
        closer: _inlineRange(fact.closer),
      );
      pairs.add(pair);
      open.add(pair);
    }
    return FlarkV3InlineDelimiterTopology._(
      sourceStartUtf16: facts.source.startUtf16,
      sourceEndUtf16: facts.source.endUtf16,
      pairs: pairs,
    );
  }

  final int sourceStartUtf16;
  final int sourceEndUtf16;
  final List<FlarkV3InlineDelimiterPair> pairs;

  bool get isEmpty => pairs.isEmpty;

  /// Expands one non-empty source deletion over every newly orphaned pair.
  ///
  /// A range that crosses only part of a hidden marker is rejected. Callers
  /// may then fail closed to literal source rather than splitting a parser-
  /// certified delimiter.
  FlarkV3InlineDeletionPlan planDeletion(
    int sourceStartUtf16,
    int sourceEndUtf16,
  ) {
    _validateRange(sourceStartUtf16, sourceEndUtf16);
    if (sourceStartUtf16 == sourceEndUtf16) {
      return FlarkV3InlineDeletionPlan._(
        sourceStartUtf16: sourceStartUtf16,
        sourceEndUtf16: sourceEndUtf16,
        removedPairIds: const [],
      );
    }

    var start = sourceStartUtf16;
    var end = sourceEndUtf16;
    final removed = <int>{};
    var changed = true;
    while (changed) {
      changed = false;
      for (final pair in pairs.reversed) {
        if (start <= pair.content.startUtf16 && pair.content.endUtf16 <= end) {
          removed.add(pair.id);
          final expandedStart = start < pair.source.startUtf16
              ? start
              : pair.source.startUtf16;
          final expandedEnd = end > pair.source.endUtf16
              ? end
              : pair.source.endUtf16;
          if (expandedStart != start || expandedEnd != end) {
            start = expandedStart;
            end = expandedEnd;
            changed = true;
          }
        }
      }
    }

    for (final pair in pairs) {
      final markerIntersection =
          pair.opener.intersects(start, end) ||
          pair.closer.intersects(start, end);
      if (!markerIntersection) continue;
      if (start <= pair.source.startUtf16 && pair.source.endUtf16 <= end) {
        removed.add(pair.id);
        continue;
      }
      throw const FlarkV3InlineProjectionException(
        'Deletion crosses only part of a certified delimiter pair.',
      );
    }

    final removedIds = removed.toList()..sort();
    return FlarkV3InlineDeletionPlan._(
      sourceStartUtf16: start,
      sourceEndUtf16: end,
      removedPairIds: removedIds,
    );
  }

  /// Normalizes one projected source edit without recognizing Markdown.
  ///
  /// The caller maps display coordinates through the certified projection,
  /// then passes that exact source edit here before mutation. Atomic constructs
  /// are identified solely by their parser-authored fact kind and geometry and
  /// are always normalized atomically. [cleanupOrphanedPairs] may defer ordinary
  /// empty-pair expansion across a platform delete-then-insert batch without
  /// weakening that atomic guarantee. No source character is inspected.
  FlarkV3InlineEditPlan planEdit(
    FlarkV3SourceEdit edit, {
    bool cleanupOrphanedPairs = true,
  }) {
    _validateRange(edit.startUtf16, edit.endUtf16);
    var start = edit.startUtf16;
    var end = edit.endUtf16;
    final removed = <int>{};
    final isInsertion = start == end;

    for (final pair in pairs) {
      if (!_isAtomicInlineKind(pair.kind)) continue;
      if (isInsertion) {
        if (pair.source.startUtf16 < start && start < pair.source.endUtf16) {
          start = pair.source.startUtf16;
          end = start;
          break;
        }
        continue;
      }
      if (!pair.source.intersects(start, end)) continue;
      if (pair.source.startUtf16 < start) start = pair.source.startUtf16;
      if (pair.source.endUtf16 > end) end = pair.source.endUtf16;
      removed.add(pair.id);
    }

    if (cleanupOrphanedPairs && edit.replacement.isEmpty && start < end) {
      final deletion = planDeletion(start, end);
      start = deletion.sourceStartUtf16;
      end = deletion.sourceEndUtf16;
      removed.addAll(deletion.removedPairIds);
    }

    final removedAtomicFacts = <int, FlarkV3InlineFactKind>{};
    final removedPairedDelimiters = <int>[];
    for (final pair in pairs) {
      if (!removed.contains(pair.id)) continue;
      if (_isAtomicInlineKind(pair.kind)) {
        removedAtomicFacts[pair.id] = pair.kind;
      } else {
        removedPairedDelimiters.add(pair.id);
      }
    }
    final authorizedAtomicBoundaryKinds = <FlarkV3InlineFactKind>{
      if (isInsertion && edit.replacement.isNotEmpty)
        for (final pair in pairs)
          if (_isAtomicInlineKind(pair.kind) &&
              (start == pair.source.startUtf16 ||
                  start == pair.source.endUtf16))
            pair.kind,
    };
    return FlarkV3InlineEditPlan._(
      sourceStartUtf16: start,
      sourceEndUtf16: end,
      replacement: edit.replacement,
      removedAtomicFacts: removedAtomicFacts,
      removedPairedDelimiterFactIds: removedPairedDelimiters,
      authorizedAtomicBoundaryKinds: authorizedAtomicBoundaryKinds,
    );
  }

  /// Plans every disjoint empty-pair cleanup after deferred provisional edits.
  ///
  /// Only outermost empty pairs are returned. Nested empty pairs are already
  /// contained by their parent's deletion. The plans are source ordered and
  /// can be applied in reverse without rebasing earlier offsets.
  List<FlarkV3InlineDeletionPlan> planOrphanCleanup() {
    if (pairs.isEmpty) return const [];
    final byId = <int, FlarkV3InlineDelimiterPair>{
      for (final pair in pairs) pair.id: pair,
    };
    final output = <FlarkV3InlineDeletionPlan>[];
    for (final pair in pairs) {
      if (!pair.content.isCollapsed) continue;
      var parentId = pair.parentId;
      var nestedInEmptyParent = false;
      while (parentId != null) {
        final parent = byId[parentId];
        if (parent == null) {
          throw const FlarkV3InlineProjectionException(
            'Delimiter topology has a missing parent.',
          );
        }
        if (parent.content.isCollapsed) {
          nestedInEmptyParent = true;
          break;
        }
        parentId = parent.parentId;
      }
      if (nestedInEmptyParent) continue;
      final removed = <int>[
        for (final candidate in pairs)
          if (pair.source.contains(candidate.source)) candidate.id,
      ]..sort();
      output.add(
        FlarkV3InlineDeletionPlan._(
          sourceStartUtf16: pair.source.startUtf16,
          sourceEndUtf16: pair.source.endUtf16,
          removedPairIds: removed,
        ),
      );
    }
    return List<FlarkV3InlineDeletionPlan>.unmodifiable(output);
  }

  /// Mechanically maps this topology through one already-approved source edit.
  FlarkV3InlineDelimiterTopology afterSourceEdit(FlarkV3SourceEdit edit) {
    _validateRange(edit.startUtf16, edit.endUtf16);
    final delta = edit.replacement.length - (edit.endUtf16 - edit.startUtf16);
    final removed = <int>{};
    final isInsertion = edit.startUtf16 == edit.endUtf16;
    for (final pair in pairs) {
      if (edit.startUtf16 <= pair.source.startUtf16 &&
          pair.source.endUtf16 <= edit.endUtf16) {
        removed.add(pair.id);
        continue;
      }
      if (_isAtomicInlineKind(pair.kind) &&
          ((isInsertion &&
                  pair.source.startUtf16 < edit.startUtf16 &&
                  edit.startUtf16 < pair.source.endUtf16) ||
              (!isInsertion &&
                  pair.source.intersects(edit.startUtf16, edit.endUtf16)))) {
        throw const FlarkV3InlineProjectionException(
          'Source edit splits a certified atomic inline construct.',
        );
      }
      if (pair.opener.intersects(edit.startUtf16, edit.endUtf16) ||
          pair.closer.intersects(edit.startUtf16, edit.endUtf16)) {
        throw const FlarkV3InlineProjectionException(
          'Source edit crosses only part of a certified delimiter pair.',
        );
      }
    }

    final output = <FlarkV3InlineDelimiterPair>[];
    for (final pair in pairs) {
      if (removed.contains(pair.id)) continue;
      final parentId = pair.parentId;
      if (parentId != null && removed.contains(parentId)) {
        throw const FlarkV3InlineProjectionException(
          'Source edit retained a child of a removed delimiter pair.',
        );
      }
      if (edit.endUtf16 <= pair.source.startUtf16) {
        output.add(pair._shift(delta));
      } else if (edit.startUtf16 >= pair.source.endUtf16) {
        output.add(pair);
      } else if (pair.content.startUtf16 <= edit.startUtf16 &&
          edit.endUtf16 <= pair.content.endUtf16) {
        output.add(pair._editInsideContent(delta));
      } else {
        throw const FlarkV3InlineProjectionException(
          'Source edit escapes certified delimiter content.',
        );
      }
    }

    return FlarkV3InlineDelimiterTopology._(
      sourceStartUtf16: sourceStartUtf16,
      sourceEndUtf16: sourceEndUtf16 + delta,
      pairs: output,
    );
  }

  /// Maps this bounded inline topology through an edit to an enclosing source
  /// projection.
  ///
  /// Structural projections such as a selected list item may own source
  /// before and after the inline leaf (a hidden marker prefix and a physical
  /// line ending). Those edits have no Markdown meaning: an edit before the
  /// leaf shifts the already-certified topology, an edit after it leaves the
  /// topology untouched, and an edit inside it uses [afterSourceEdit]. An edit
  /// crossing the leaf boundary is rejected so callers fail closed instead of
  /// extending parser authority mechanically.
  FlarkV3InlineDelimiterTopology afterEnclosingSourceEdit(
    FlarkV3SourceEdit edit,
  ) {
    if (edit.startUtf16 < 0 || edit.endUtf16 < edit.startUtf16) {
      throw RangeError('Source edit has an invalid range.');
    }
    final isInsertion = edit.startUtf16 == edit.endUtf16;
    final before =
        edit.endUtf16 < sourceStartUtf16 ||
        (!isInsertion && edit.endUtf16 == sourceStartUtf16);
    if (before) {
      final delta = edit.replacement.length - (edit.endUtf16 - edit.startUtf16);
      return FlarkV3InlineDelimiterTopology._(
        sourceStartUtf16: sourceStartUtf16 + delta,
        sourceEndUtf16: sourceEndUtf16 + delta,
        pairs: [for (final pair in pairs) pair._shift(delta)],
      );
    }
    final after =
        edit.startUtf16 > sourceEndUtf16 ||
        (!isInsertion && edit.startUtf16 == sourceEndUtf16);
    if (after) return this;
    if (edit.startUtf16 >= sourceStartUtf16 &&
        edit.endUtf16 <= sourceEndUtf16) {
      return afterSourceEdit(edit);
    }
    throw const FlarkV3InlineProjectionException(
      'Source edit crosses the parser-certified inline leaf boundary.',
    );
  }

  void _validateRange(int start, int end) {
    if (start < sourceStartUtf16 || end < start || end > sourceEndUtf16) {
      throw RangeError('Source edit escapes delimiter topology.');
    }
  }
}

/// Internal Dart-only projection of one validated authoritative inline leaf.
///
/// No Markdown classification occurs here. An unsupported whole-leaf result
/// always becomes an identity projection, even when marker hiding was
/// requested, because no marker range is then certified safe to hide.
final class FlarkV3InlineProjection {
  FlarkV3InlineProjection._({
    required this.sourceStartUtf16,
    required this.sourceEndUtf16,
    required this.sourceText,
    required this.displayText,
    required List<FlarkV3InlineDisplayRun> runs,
    required List<FlarkV3InlineImageAnnotation> imageAnnotations,
    required this.sourceProjection,
    required this.delimiterTopology,
    required this.work,
  }) : runs = List<FlarkV3InlineDisplayRun>.unmodifiable(runs),
       imageAnnotations = List<FlarkV3InlineImageAnnotation>.unmodifiable(
         imageAnnotations,
       );

  factory FlarkV3InlineProjection.fromValidatedFacts({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3InlineFacts facts,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.allMarkersVisible,
  }) {
    _validateExactAuthority(sourceDocument, expectedSource, facts);
    _validateFacts(facts);

    final work = _MutableProjectionWork();
    final certifiedMarkers = _sortedCertifiedMarkers(facts, work);
    final hiddenMarkers =
        markerPolicy == FlarkV3InlineMarkerPolicy.hideCertifiedMarkers &&
            facts.disposition == FlarkV3InlineFactsDisposition.authoritative
        ? _buildHiddenMarkerChains(facts, certifiedMarkers)
        : const <_HiddenMarkerChain>[];
    // Non-collapsed marker ranges are disjoint and already sorted. A certified
    // escape has no closer marker, so its content end is merged separately as
    // a semantic boundary without pretending that a zero-width marker exists.
    final boundaries = _projectionBoundaries(facts, certifiedMarkers);
    final hardLineBreakContentEnds = <int, int>{
      for (final fact in facts.facts)
        if (fact.kind == FlarkV3InlineFactKind.hardLineBreak)
          fact.content.startUtf16: fact.content.endUtf16,
    };
    final characterReferencesByStart = <int, FlarkV3InlineFact>{
      for (final fact in facts.facts)
        if (fact.kind == FlarkV3InlineFactKind.characterReference)
          fact.source.startUtf16: fact,
    };
    final appliesAuthoritativeReplacements =
        markerPolicy == FlarkV3InlineMarkerPolicy.hideCertifiedMarkers &&
        facts.disposition == FlarkV3InlineFactsDisposition.authoritative;

    // One bounded leaf materialization prevents one source-tree descent per
    // display run. Every later slice is local UTF-16 over this <=8 KiB leaf.
    final leafText = sourceDocument.readRange(
      facts.source.startUtf16,
      facts.source.endUtf16,
    );
    final runs = <FlarkV3InlineDisplayRun>[];
    final projectionPieces = <FlarkV3SourceProjectionPiece>[];
    final display = StringBuffer();
    var activeStack = FlarkV3InlineSemanticStack.empty;
    work.sourceLeafReads = 1;
    var nextFact = 0;
    var displayOffset = 0;
    var hiddenIndex = 0;

    void advanceSemanticSweep(int boundary) {
      work.boundaryPointsVisited += 1;
      while (!activeStack.isEmpty) {
        work.styleBoundaryComparisons += 1;
        if (activeStack._fact!.content.endUtf16 > boundary) break;
        activeStack = activeStack._parent!;
        work.factEndEventsApplied += 1;
      }
      while (nextFact < facts.facts.length) {
        work.styleBoundaryComparisons += 1;
        final fact = facts.facts[nextFact];
        if (fact.content.startUtf16 > boundary) break;
        nextFact += 1;
        work.factStartEventsApplied += 1;
        if (fact.content.endUtf16 <= boundary) {
          // Empty content has coincident start/end events and never becomes
          // active for a non-empty display interval.
          work.factEndEventsApplied += 1;
        } else {
          activeStack = FlarkV3InlineSemanticStack._push(fact, activeStack);
          work.semanticStackNodesAllocated += 1;
        }
      }
    }

    for (var index = 0; index + 1 < boundaries.length; index += 1) {
      final start = boundaries[index];
      final end = boundaries[index + 1];
      work.boundaryIntervalsVisited += 1;
      advanceSemanticSweep(start);
      if (start == end) continue;
      while (hiddenIndex < hiddenMarkers.length &&
          hiddenMarkers[hiddenIndex].end <= start) {
        hiddenIndex += 1;
      }
      if (hiddenIndex < hiddenMarkers.length &&
          hiddenMarkers[hiddenIndex].start <= start &&
          end <= hiddenMarkers[hiddenIndex].end) {
        projectionPieces.add(
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: start,
            sourceEndUtf16: end,
          ),
        );
        continue;
      }

      final characterReference = characterReferencesByStart[start];
      final replacesCharacterReference =
          appliesAuthoritativeReplacements &&
          characterReference != null &&
          characterReference.source.endUtf16 == end;
      final normalizesHardLineBreak =
          appliesAuthoritativeReplacements &&
          hardLineBreakContentEnds[start] == end;
      final text = replacesCharacterReference
          ? characterReference.characterReferenceValue!
          : normalizesHardLineBreak
          ? '\n'
          : leafText.substring(
              start - facts.source.startUtf16,
              end - facts.source.startUtf16,
            );
      projectionPieces.add(
        replacesCharacterReference || normalizesHardLineBreak
            ? FlarkV3SourceProjectionPiece.replace(
                sourceStartUtf16: start,
                sourceEndUtf16: end,
                displayText: text,
              )
            : FlarkV3SourceProjectionPiece.copy(
                sourceStartUtf16: start,
                sourceEndUtf16: end,
              ),
      );
      final displayEnd = displayOffset + text.length;
      runs.add(
        FlarkV3InlineDisplayRun._(
          text: text,
          sourceStartUtf16: start,
          sourceEndUtf16: end,
          displayStartUtf16: displayOffset,
          displayEndUtf16: displayEnd,
          semanticStack: activeStack,
        ),
      );
      work
        ..runStackReferencesStored += 1
        ..logicalSemanticDepthSum += activeStack.depth
        ..sourceSlices += 1;
      display.write(text);
      displayOffset = displayEnd;
    }
    advanceSemanticSweep(boundaries.last);
    final sourceProjection = FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: facts.source.startUtf16,
      sourceText: leafText,
      pieces: projectionPieces,
      certifiedSourceVersion: expectedSource,
    );
    if (sourceProjection.displayText != display.toString()) {
      throw const FlarkV3InlineProjectionException(
        'Inline runs diverge from their exhaustive source projection.',
      );
    }

    return FlarkV3InlineProjection._(
      sourceStartUtf16: facts.source.startUtf16,
      sourceEndUtf16: facts.source.endUtf16,
      sourceText: leafText,
      displayText: sourceProjection.displayText,
      runs: runs,
      imageAnnotations: [for (final fact in facts.facts) ?fact.imageAnnotation],
      sourceProjection: sourceProjection,
      delimiterTopology: FlarkV3InlineDelimiterTopology._fromFacts(facts),
      work: work.seal(),
    );
  }

  /// Absolute UTF-16 range of the bounded source leaf.
  final int sourceStartUtf16;
  final int sourceEndUtf16;

  /// Exact bounded source snapshot from which [displayText] was projected.
  final String sourceText;

  /// Exact parser-authored projected text.
  ///
  /// Marker-free hard line breaks normalize LF, CR, and CRLF source content to
  /// one display LF while [sourceText] retains the exact physical line ending.
  final String displayText;

  final List<FlarkV3InlineDisplayRun> runs;

  /// Parser-preorder images, including images whose alt label is empty.
  ///
  /// Non-empty alt runs also expose their active annotation directly. This
  /// collection prevents an empty label from erasing the image semantic node
  /// merely because there is no non-empty text interval on which to hang it.
  final List<FlarkV3InlineImageAnnotation> imageAnnotations;

  final FlarkV3SourceProjection sourceProjection;
  final FlarkV3InlineDelimiterTopology delimiterTopology;
  final FlarkV3InlineProjectionWorkReceipt work;

  int get sourceLengthUtf16 => sourceEndUtf16 - sourceStartUtf16;
  int get displayLengthUtf16 => displayText.length;

  /// Maps an absolute source UTF-16 offset into leaf-relative display space.
  ///
  /// Hidden and replaced parser-authored pieces use the same exhaustive map
  /// exposed by [sourceProjection].
  int sourceToDisplayOffset(int sourceOffsetUtf16) =>
      sourceProjection.sourceToDisplayOffset(sourceOffsetUtf16);

  /// Maps a leaf-relative display UTF-16 offset to absolute source space.
  ///
  /// At a hidden-marker boundary, [affinity] deterministically selects the
  /// earliest or latest source caret in the collapsed adjacent-marker chain.
  /// Affinity is required because choosing the rich-editor caret behavior at
  /// an opening or closing edge belongs to the editor interaction policy, not
  /// to this source projection.
  int displayToSourceOffset(
    int displayOffsetUtf16, {
    required FlarkV3InlineProjectionAffinity affinity,
  }) {
    return sourceProjection.displayToSourceOffset(
      displayOffsetUtf16,
      affinity: switch (affinity) {
        FlarkV3InlineProjectionAffinity.upstream =>
          FlarkV3SourceProjectionAffinity.upstream,
        FlarkV3InlineProjectionAffinity.downstream =>
          FlarkV3SourceProjectionAffinity.downstream,
      },
    );
  }
}

void _validateExactAuthority(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3SourceVersion expectedSource,
  FlarkV3InlineFacts facts,
) {
  if (facts.sourceVersion != expectedSource) {
    throw const FlarkV3InlineProjectionException(
      'Inline facts do not match the caller exact source authority.',
    );
  }
  if (!sourceDocument.hasCertifiedFacts ||
      sourceDocument.revision != expectedSource.revision ||
      sourceDocument.utf8Length != expectedSource.metric.bytes ||
      sourceDocument.utf16Length != expectedSource.metric.utf16 ||
      sourceDocument.contentHash128 != expectedSource.contentHash) {
    throw const FlarkV3InlineProjectionException(
      'Projection source does not match the certified source version.',
    );
  }
}

FlarkV3InlineUtf16Range _inlineRange(FlarkV3SourceSpan span) =>
    FlarkV3InlineUtf16Range(span.startUtf16, span.endUtf16);

void _validateFacts(FlarkV3InlineFacts facts) {
  final leaf = facts.source;
  if (leaf.startUtf16 < 0 ||
      leaf.endUtf16 <= leaf.startUtf16 ||
      leaf.endUtf16 > facts.sourceVersion.metric.utf16) {
    throw const FlarkV3InlineProjectionException(
      'Inline leaf is outside its exact source.',
    );
  }
  if (facts.disposition == FlarkV3InlineFactsDisposition.unsupported &&
      facts.facts.isNotEmpty) {
    throw const FlarkV3InlineProjectionException(
      'Unsupported inline facts cannot carry semantic ranges.',
    );
  }

  final open = <FlarkV3InlineFact>[];
  var openContainerLinkCount = 0;
  var previousStart = -1;
  var previousContentStart = -1;
  for (final fact in facts.facts) {
    final escaped = fact.kind == FlarkV3InlineFactKind.escapedPunctuation;
    final hardLineBreak = fact.kind == FlarkV3InlineFactKind.hardLineBreak;
    final characterReference =
        fact.kind == FlarkV3InlineFactKind.characterReference;
    final markerlessAutolink = _usesMarkerlessAutolinkGeometry(fact);
    final openerLength = fact.opener.endUtf16 - fact.opener.startUtf16;
    final contentLength = fact.content.endUtf16 - fact.content.startUtf16;
    final openerCollapsed = fact.opener.startUtf16 == fact.opener.endUtf16;
    final closerCollapsed = fact.closer.startUtf16 == fact.closer.endUtf16;
    if (fact.source.startUtf16 < previousStart ||
        fact.content.startUtf16 < previousContentStart ||
        fact.source.startUtf16 < leaf.startUtf16 ||
        fact.source.endUtf16 > leaf.endUtf16 ||
        fact.source.startUtf16 != fact.opener.startUtf16 ||
        fact.opener.endUtf16 != fact.content.startUtf16 ||
        fact.content.endUtf16 != fact.closer.startUtf16 ||
        fact.closer.endUtf16 != fact.source.endUtf16 ||
        !_factAnnotationsAreCanonical(fact, facts.sourceVersion) ||
        (characterReference
            ? !openerCollapsed ||
                  !closerCollapsed ||
                  contentLength !=
                      fact.source.endUtf16 - fact.source.startUtf16 ||
                  fact.characterReferenceValue == null
            : markerlessAutolink
            ? !openerCollapsed ||
                  !closerCollapsed ||
                  fact.characterReferenceValue != null
            : openerCollapsed ||
                  fact.characterReferenceValue != null ||
                  (escaped
                      ? openerLength != 1 ||
                            contentLength != 1 ||
                            !closerCollapsed
                      : hardLineBreak
                      ? openerLength < 1 ||
                            (contentLength != 1 && contentLength != 2) ||
                            !closerCollapsed
                      : closerCollapsed))) {
      throw const FlarkV3InlineProjectionException(
        'Inline fact ranges are not canonical.',
      );
    }
    previousStart = fact.source.startUtf16;
    previousContentStart = fact.content.startUtf16;
    while (open.isNotEmpty &&
        fact.source.startUtf16 >= open.last.source.endUtf16) {
      final closed = open.removeLast();
      if (_isActionableContainerLinkKind(closed.kind)) {
        openContainerLinkCount -= 1;
      }
    }
    if (open.isNotEmpty) {
      final parent = open.last;
      final nestedInsideContainerLink =
          _isLinkInlineKind(fact.kind) && openContainerLinkCount > 0;
      final isCertifiedUriCharacterReference =
          parent.kind == FlarkV3InlineFactKind.autolinkUri &&
          !_usesMarkerlessAutolinkGeometry(parent) &&
          fact.kind == FlarkV3InlineFactKind.characterReference;
      if (parent.kind == FlarkV3InlineFactKind.code ||
          _isLeafInlineKind(parent.kind) ||
          ((parent.kind == FlarkV3InlineFactKind.autolinkUri ||
                  parent.kind == FlarkV3InlineFactKind.autolinkEmail) &&
              !isCertifiedUriCharacterReference) ||
          nestedInsideContainerLink ||
          fact.source.startUtf16 < parent.content.startUtf16 ||
          fact.source.endUtf16 > parent.content.endUtf16) {
        throw const FlarkV3InlineProjectionException(
          'Inline semantic ranges cross non-canonically.',
        );
      }
    }
    open.add(fact);
    if (_isActionableContainerLinkKind(fact.kind)) {
      openContainerLinkCount += 1;
    }
  }
}

bool _factAnnotationsAreCanonical(
  FlarkV3InlineFact fact,
  FlarkV3SourceVersion sourceVersion,
) {
  final link = fact.linkAnnotation;
  final image = fact.imageAnnotation;
  if (fact.kind == FlarkV3InlineFactKind.directLink) {
    return link != null &&
        image == null &&
        link.kind == FlarkV3InlineLinkKind.direct &&
        link.targetRecipe ==
            FlarkV3InlineLinkTargetRecipe.companionCookedValue &&
        _sameInlineSpan(link.source, fact.source) &&
        _sameInlineSpan(link.content, fact.content) &&
        _valueCutsAreCanonical(
          closer: fact.closer,
          destination: link.destinationSource,
          title: link.titleSource,
          titleValue: link.title,
        );
  }
  if (fact.kind == FlarkV3InlineFactKind.directImage) {
    return link == null &&
        image != null &&
        _sameInlineSpan(image.source, fact.source) &&
        _sameInlineSpan(image.content, fact.content) &&
        _valueCutsAreCanonical(
          closer: fact.closer,
          destination: image.destinationSource,
          title: image.titleSource,
          titleValue: image.title,
        );
  }
  if (fact.kind == FlarkV3InlineFactKind.referenceLink) {
    return link != null &&
        image == null &&
        link.kind == FlarkV3InlineLinkKind.reference &&
        link.targetRecipe ==
            FlarkV3InlineLinkTargetRecipe.companionCookedValue &&
        _sameInlineSpan(link.source, fact.source) &&
        _sameInlineSpan(link.content, fact.content) &&
        _referenceValueCutsAreCanonical(
          sourceVersion: sourceVersion,
          destination: link.destinationSource,
          title: link.titleSource,
          titleValue: link.title,
        );
  }
  if (fact.kind == FlarkV3InlineFactKind.referenceImage) {
    return link == null &&
        image != null &&
        _sameInlineSpan(image.source, fact.source) &&
        _sameInlineSpan(image.content, fact.content) &&
        _referenceValueCutsAreCanonical(
          sourceVersion: sourceVersion,
          destination: image.destinationSource,
          title: image.titleSource,
          titleValue: image.title,
        );
  }
  if (fact.kind == FlarkV3InlineFactKind.autolinkUri) {
    final markerless = _usesMarkerlessAutolinkGeometry(fact);
    final recipe = link?.targetRecipe;
    return link != null &&
        image == null &&
        link.kind == FlarkV3InlineLinkKind.uri &&
        (markerless
            ? recipe == FlarkV3InlineLinkTargetRecipe.exactContent ||
                  recipe ==
                      FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent
            : recipe == FlarkV3InlineLinkTargetRecipe.exactContent ||
                  recipe ==
                      FlarkV3InlineLinkTargetRecipe
                          .characterReferenceProjectedContent) &&
        _sameInlineSpan(link.source, fact.source) &&
        _sameInlineSpan(link.content, fact.content) &&
        _sameInlineSpan(link.destinationSource, fact.content) &&
        link.title == null &&
        link.titleSource == null;
  }
  if (fact.kind == FlarkV3InlineFactKind.autolinkEmail) {
    return link != null &&
        image == null &&
        link.kind == FlarkV3InlineLinkKind.email &&
        link.targetRecipe == FlarkV3InlineLinkTargetRecipe.mailtoExactContent &&
        _sameInlineSpan(link.source, fact.source) &&
        _sameInlineSpan(link.content, fact.content) &&
        _sameInlineSpan(link.destinationSource, fact.content) &&
        link.title == null &&
        link.titleSource == null;
  }
  return link == null && image == null;
}

bool _usesMarkerlessAutolinkGeometry(FlarkV3InlineFact fact) =>
    (fact.kind == FlarkV3InlineFactKind.autolinkUri ||
        fact.kind == FlarkV3InlineFactKind.autolinkEmail) &&
    _sameInlineSpan(fact.source, fact.content) &&
    fact.opener.startUtf16 == fact.opener.endUtf16 &&
    fact.closer.startUtf16 == fact.closer.endUtf16;

bool _valueCutsAreCanonical({
  required FlarkV3SourceSpan closer,
  required FlarkV3SourceSpan destination,
  required FlarkV3SourceSpan? title,
  required String? titleValue,
}) {
  if (destination.startUtf8 < closer.startUtf8 ||
      destination.endUtf8 > closer.endUtf8 ||
      destination.endUtf8 < destination.startUtf8 ||
      destination.startUtf16 < closer.startUtf16 ||
      destination.endUtf16 > closer.endUtf16 ||
      destination.endUtf16 < destination.startUtf16 ||
      (title == null) != (titleValue == null)) {
    return false;
  }
  if (title == null) return true;
  return title.startUtf8 >= destination.endUtf8 &&
      title.endUtf8 > title.startUtf8 &&
      title.endUtf8 <= closer.endUtf8 &&
      title.startUtf16 >= destination.endUtf16 &&
      title.endUtf16 > title.startUtf16 &&
      title.endUtf16 <= closer.endUtf16;
}

bool _referenceValueCutsAreCanonical({
  required FlarkV3SourceVersion sourceVersion,
  required FlarkV3SourceSpan destination,
  required FlarkV3SourceSpan? title,
  required String? titleValue,
}) {
  if (destination.startUtf8 < 0 ||
      destination.endUtf8 < destination.startUtf8 ||
      destination.endUtf8 > sourceVersion.metric.bytes ||
      destination.startUtf16 < 0 ||
      destination.endUtf16 < destination.startUtf16 ||
      destination.endUtf16 > sourceVersion.metric.utf16 ||
      (title == null) != (titleValue == null)) {
    return false;
  }
  if (title == null) return true;
  return title.startUtf8 >= destination.endUtf8 &&
      title.endUtf8 > title.startUtf8 &&
      title.endUtf8 <= sourceVersion.metric.bytes &&
      title.startUtf16 >= destination.endUtf16 &&
      title.endUtf16 > title.startUtf16 &&
      title.endUtf16 <= sourceVersion.metric.utf16;
}

bool _sameInlineSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

List<_Utf16Range> _sortedCertifiedMarkers(
  FlarkV3InlineFacts facts,
  _MutableProjectionWork work,
) {
  final markers =
      <_Utf16Range>[
        for (final fact in facts.facts)
          if (fact.kind != FlarkV3InlineFactKind.characterReference) ...[
            if (fact.opener.startUtf16 < fact.opener.endUtf16)
              _Utf16Range(fact.opener.startUtf16, fact.opener.endUtf16),
            if (fact.closer.startUtf16 < fact.closer.endUtf16)
              _Utf16Range(fact.closer.startUtf16, fact.closer.endUtf16),
          ],
      ]..sort((left, right) {
        work.markerSortComparisons += 1;
        final byStart = left.start.compareTo(right.start);
        return byStart != 0 ? byStart : left.end.compareTo(right.end);
      });

  for (var index = 1; index < markers.length; index += 1) {
    if (markers[index].start < markers[index - 1].end) {
      throw const FlarkV3InlineProjectionException(
        'Certified inline marker ranges overlap.',
      );
    }
  }
  return markers;
}

List<int> _projectionBoundaries(
  FlarkV3InlineFacts facts,
  List<_Utf16Range> sortedMarkers,
) {
  final markerEndpoints = <int>[
    for (final marker in sortedMarkers) ...[marker.start, marker.end],
  ];
  final factBoundaries = <int>[
    for (final fact in facts.facts)
      if (_isAtomicInlineKind(fact.kind))
        fact.content.endUtf16
      else if (fact.kind == FlarkV3InlineFactKind.characterReference) ...[
        fact.source.startUtf16,
        fact.source.endUtf16,
      ] else if (_usesMarkerlessAutolinkGeometry(fact)) ...[
        fact.source.startUtf16,
        fact.source.endUtf16,
      ],
  ]..sort();
  final boundaries = <int>[facts.source.startUtf16];
  var markerIndex = 0;
  var factBoundaryIndex = 0;

  void add(int boundary) {
    if (boundary > boundaries.last) boundaries.add(boundary);
  }

  while (markerIndex < markerEndpoints.length ||
      factBoundaryIndex < factBoundaries.length) {
    if (factBoundaryIndex >= factBoundaries.length ||
        (markerIndex < markerEndpoints.length &&
            markerEndpoints[markerIndex] <=
                factBoundaries[factBoundaryIndex])) {
      add(markerEndpoints[markerIndex]);
      markerIndex += 1;
    } else {
      add(factBoundaries[factBoundaryIndex]);
      factBoundaryIndex += 1;
    }
  }
  add(facts.source.endUtf16);
  return boundaries;
}

List<_HiddenMarkerChain> _buildHiddenMarkerChains(
  FlarkV3InlineFacts facts,
  List<_Utf16Range> sortedMarkers,
) {
  if (sortedMarkers.isEmpty) return const [];

  final merged = <_Utf16Range>[];
  for (final marker in sortedMarkers) {
    if (merged.isEmpty || marker.start > merged.last.end) {
      merged.add(marker);
    } else {
      merged[merged.length - 1] = _Utf16Range(merged.last.start, marker.end);
    }
  }

  final hidden = <_HiddenMarkerChain>[];
  for (final marker in merged) {
    hidden.add(_HiddenMarkerChain(start: marker.start, end: marker.end));
  }
  return hidden;
}

bool _isSemanticStyleKind(FlarkV3InlineFactKind kind) => switch (kind) {
  FlarkV3InlineFactKind.emphasis ||
  FlarkV3InlineFactKind.strong ||
  FlarkV3InlineFactKind.code ||
  FlarkV3InlineFactKind.strikethrough => true,
  FlarkV3InlineFactKind.autolinkUri ||
  FlarkV3InlineFactKind.autolinkEmail ||
  FlarkV3InlineFactKind.escapedPunctuation ||
  FlarkV3InlineFactKind.hardLineBreak ||
  FlarkV3InlineFactKind.characterReference ||
  FlarkV3InlineFactKind.directLink ||
  FlarkV3InlineFactKind.directImage ||
  FlarkV3InlineFactKind.referenceLink ||
  FlarkV3InlineFactKind.referenceImage => false,
};

bool _isAtomicInlineKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.escapedPunctuation ||
    kind == FlarkV3InlineFactKind.hardLineBreak;

bool _isLeafInlineKind(FlarkV3InlineFactKind kind) =>
    _isAtomicInlineKind(kind) ||
    kind == FlarkV3InlineFactKind.characterReference;

bool _isLinkInlineKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.autolinkUri ||
    kind == FlarkV3InlineFactKind.autolinkEmail ||
    kind == FlarkV3InlineFactKind.directLink ||
    kind == FlarkV3InlineFactKind.referenceLink;

bool _isActionableContainerLinkKind(FlarkV3InlineFactKind kind) =>
    kind == FlarkV3InlineFactKind.directLink ||
    kind == FlarkV3InlineFactKind.referenceLink;

final class _Utf16Range {
  const _Utf16Range(this.start, this.end);

  final int start;
  final int end;

  int get length => end - start;
}

final class _HiddenMarkerChain {
  const _HiddenMarkerChain({required this.start, required this.end});

  final int start;
  final int end;
}

final class _MutableProjectionWork {
  int markerSortComparisons = 0;
  int boundaryPointsVisited = 0;
  int boundaryIntervalsVisited = 0;
  int styleBoundaryComparisons = 0;
  int factStartEventsApplied = 0;
  int factEndEventsApplied = 0;
  int semanticStackNodesAllocated = 0;
  int runStackReferencesStored = 0;
  int logicalSemanticDepthSum = 0;
  int sourceLeafReads = 0;
  int sourceSlices = 0;

  FlarkV3InlineProjectionWorkReceipt seal() =>
      FlarkV3InlineProjectionWorkReceipt._(
        markerSortComparisons: markerSortComparisons,
        boundaryPointsVisited: boundaryPointsVisited,
        boundaryIntervalsVisited: boundaryIntervalsVisited,
        styleBoundaryComparisons: styleBoundaryComparisons,
        factStartEventsApplied: factStartEventsApplied,
        factEndEventsApplied: factEndEventsApplied,
        semanticStackNodesAllocated: semanticStackNodesAllocated,
        runStackReferencesStored: runStackReferencesStored,
        logicalSemanticDepthSum: logicalSemanticDepthSum,
        sourceLeafReads: sourceLeafReads,
        sourceSlices: sourceSlices,
      );
}
