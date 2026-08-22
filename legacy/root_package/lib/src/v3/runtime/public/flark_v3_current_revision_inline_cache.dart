import 'dart:collection';

import '../../host/flark_v3_host_protocol.dart';
import 'flark_v3_block_quote_projection.dart';
import 'flark_v3_bullet_list_projection.dart';
import 'flark_v3_document_query.dart';
import 'flark_v3_inline_facts.dart';
import 'flark_v3_ordered_list_projection.dart';

/// Bounded decoded-inline reuse for one exact structural ACK.
///
/// The independent host intentionally retains only its latest hot-inline
/// sidecar. This cache lets a later point query reuse already-decoded facts
/// after that singleton moves to another inline-bearing leaf. It never recognizes
/// Markdown and never carries facts across structural authority.
///
/// Query budgets still guard every fresh host query. A cache hit returns an
/// already-owned immutable value and performs no host copy or tree traversal.
final class FlarkV3CurrentRevisionInlineCache {
  FlarkV3CurrentRevisionInlineCache({
    required this.maximumEntries,
    required this.maximumFactRecords,
  }) {
    if (maximumEntries <= 0) {
      throw RangeError.value(
        maximumEntries,
        'maximumEntries',
        'The inline cache must admit at least one leaf.',
      );
    }
    if (maximumFactRecords < 0) {
      throw RangeError.value(
        maximumFactRecords,
        'maximumFactRecords',
        'The inline cache fact bound cannot be negative.',
      );
    }
  }

  final int maximumEntries;
  final int maximumFactRecords;

  final LinkedHashMap<_InlineCacheKey, FlarkV3InlineFacts> _entries =
      LinkedHashMap<_InlineCacheKey, FlarkV3InlineFacts>();
  final LinkedHashMap<_InlineLeafKey, FlarkV3TightListItemProjectionPayload>
  _tightListEntries =
      LinkedHashMap<_InlineLeafKey, FlarkV3TightListItemProjectionPayload>();
  final LinkedHashMap<
    _RecursiveGreenLeafKey,
    FlarkV3BlockQuoteProjectionCertificate
  >
  _recursiveBlockQuoteEntries =
      LinkedHashMap<
        _RecursiveGreenLeafKey,
        FlarkV3BlockQuoteProjectionCertificate
      >();
  final LinkedHashMap<_RecursiveGreenLeafKey, FlarkV3ProjectedInlineFacts>
  _recursiveProjectedInlineEntries =
      LinkedHashMap<_RecursiveGreenLeafKey, FlarkV3ProjectedInlineFacts>();
  FlarkV3StructuralAck? _authority;
  int _retainedFactRecords = 0;

  int get entryCount => _entries.length;
  int get retainedFactRecords => _retainedFactRecords;
  int get recursiveBlockQuoteEntryCount => _recursiveBlockQuoteEntries.length;
  int get recursiveProjectedInlineEntryCount =>
      _recursiveProjectedInlineEntries.length;

  /// Joins cached facts to one freshly decoded exact structural query.
  ///
  /// A query that already carries host-authored facts first admits them into
  /// the cache. A structure-only query may then reuse a prior result for the
  /// same physical and projected inline-content ranges under the same exact
  /// ACK.
  FlarkV3DocumentStructuralQuery resolve({
    required FlarkV3StructuralAck authority,
    required FlarkV3DocumentStructuralQuery query,
  }) {
    _adoptAuthority(authority);
    if (_queryBindsAuthority(query, authority) &&
        (query.structure.kind == FlarkV3DocumentStructureKind.bulletList ||
            query.structure.kind == FlarkV3DocumentStructureKind.orderedList)) {
      return _resolveTightList(authority: authority, query: query);
    }
    final inlineContentSource = query.structure.inlineContentSource;
    if (!_queryBindsAuthority(query, authority) ||
        !query.structure.canCarryInlineFacts ||
        inlineContentSource == null ||
        query.projection.kind != query.structure.kind ||
        !_sameSpan(inlineContentSource, query.projection.projectedSource)) {
      return query;
    }

    final key = _InlineLeafKey(
      physicalSource: query.structure.source,
      projectedSource: query.projection.projectedSource,
    );
    final inlineFacts = query.inlineFacts;
    if (inlineFacts != null) {
      if (_factsBindQuery(inlineFacts, query, authority)) {
        _remember(key, inlineFacts);
      }
      return query;
    }

    final cached = _entries.remove(key);
    if (cached == null) return query;
    if (!_factsBindQuery(cached, query, authority)) {
      _retainedFactRecords -= cached.facts.length;
      return query;
    }
    // Removing and reinserting makes the deterministic insertion-ordered map
    // an LRU without adding another index.
    _entries[key] = cached;
    return FlarkV3DocumentStructuralQuery(
      sourceRevision: query.sourceRevision,
      structureRevision: query.structureRevision,
      structure: query.structure,
      projection: query.projection,
      inlineFacts: cached,
      indentedCodeProjection: query.indentedCodeProjection,
      pointPath: query.pointPath,
      blockQuoteProjection: query.blockQuoteProjection,
      bulletListProjection: query.bulletListProjection,
      orderedListProjection: query.orderedListProjection,
    );
  }

  /// Rendezvous for independently installed recursive-Green quote and inline
  /// certificates under one exact structural ACK.
  ///
  /// The host retains one hot sidecar. A quote marker map can therefore be
  /// replaced by the inline-facts sidecar before a point query observes both.
  /// This method retains each immutable certificate by owner frame and exact
  /// physical Paragraph range, then returns their validated union.
  FlarkV3RecursiveGreenPointQuery resolveRecursiveGreen({
    required FlarkV3StructuralAck authority,
    required FlarkV3RecursiveGreenPointQuery query,
  }) {
    _adoptAuthority(authority);
    if (!_recursiveQueryBindsAuthority(query, authority)) {
      return query;
    }
    if (!_isBlockQuoteParagraph(query)) {
      return _resolveRecursiveGreenInline(authority: authority, query: query);
    }

    final freshProjection = query.blockQuoteProjection;
    final freshParagraphSource = query.paragraphSource;
    final freshFacts = query.inlineFacts;
    final freshInlineSource = query.inlineSource;
    final freshProjectedFacts = query.projectedInlineFacts;

    _RecursiveGreenLeafKey? projectionKey;
    if (freshProjection != null &&
        _recursiveProjectionBindsQuery(freshProjection, query, authority)) {
      projectionKey = _RecursiveGreenLeafKey(
        ownerFrameId: query.owner.frameId,
        physicalSource: freshProjection.source,
        projectedUtf8Length: freshProjection.projectedUtf8Length,
        projectedUtf16Length: freshProjection.projectedUtf16Length,
      );
    }

    _RecursiveGreenLeafKey? factsKey;
    if (freshParagraphSource != null &&
        freshInlineSource != null &&
        freshFacts != null &&
        _recursiveFactsBindQuery(
          freshFacts,
          paragraphSource: freshParagraphSource,
          inlineSource: freshInlineSource,
          query: query,
          authority: authority,
        )) {
      final queryProjection = query.blockQuoteProjection;
      factsKey =
          queryProjection != null &&
              _sameSpan(queryProjection.source, freshParagraphSource)
          ? _RecursiveGreenLeafKey(
              ownerFrameId: query.owner.frameId,
              physicalSource: freshParagraphSource,
              projectedUtf8Length: queryProjection.projectedUtf8Length,
              projectedUtf16Length: queryProjection.projectedUtf16Length,
            )
          : _findRecursiveGreenKey(
              query,
              exactPhysicalSource: freshParagraphSource,
            );
    }

    _RecursiveGreenLeafKey? projectedFactsKey;
    if (freshProjectedFacts != null &&
        _recursiveProjectedFactsBindQuery(
          freshProjectedFacts,
          query: query,
          authority: authority,
          projection: freshProjection,
        )) {
      projectedFactsKey = _RecursiveGreenLeafKey(
        ownerFrameId: query.owner.frameId,
        physicalSource: freshProjectedFacts.physicalSource,
        projectedUtf8Length: freshProjectedFacts.projectedUtf8Length,
        projectedUtf16Length: freshProjectedFacts.projectedUtf16Length,
      );
    }

    final freshKeys = <_RecursiveGreenLeafKey>{
      ?projectionKey,
      ?factsKey,
      ?projectedFactsKey,
    };
    if (freshKeys.length > 1) {
      return query;
    }
    final hasFreshPresentation =
        freshProjection != null ||
        freshFacts != null ||
        freshProjectedFacts != null;
    final key =
        projectionKey ??
        factsKey ??
        projectedFactsKey ??
        (hasFreshPresentation ? null : _findRecursiveGreenKey(query));
    if (key == null) return query;

    if (projectionKey != null) {
      _rememberRecursiveBlockQuote(key, freshProjection!);
    }
    if (factsKey != null) {
      _remember(key, freshFacts!);
    }
    if (projectedFactsKey != null) {
      _rememberRecursiveProjectedInline(key, freshProjectedFacts!);
    }

    final projection = freshProjection ?? _touchRecursiveBlockQuote(key);
    final facts = freshFacts ?? _touchFacts(key);
    final projectedFacts =
        freshProjectedFacts ?? _touchRecursiveProjectedInline(key);
    if (projection != null &&
        !_recursiveProjectionBindsKey(projection, query, authority, key)) {
      _recursiveBlockQuoteEntries.remove(key);
      return query;
    }
    if (facts != null &&
        !_recursiveFactsBindKey(facts, query, authority, key)) {
      final removed = _entries.remove(key);
      if (removed != null) _retainedFactRecords -= removed.facts.length;
      return query;
    }
    if (projectedFacts != null &&
        !_recursiveProjectedFactsBindKey(
          projectedFacts,
          query,
          authority,
          key,
          projection,
        )) {
      final removed = _recursiveProjectedInlineEntries.remove(key);
      if (removed != null) {
        _retainedFactRecords -= removed.facts.length;
      }
      return query;
    }
    if (projection == null && facts == null) return query;

    return query.withPresentationCertificates(
      paragraphSource: freshParagraphSource ?? key.physicalSource,
      inlineSource: facts?.source,
      inlineFacts: facts,
      blockQuoteProjection: projection,
      projectedInlineFacts: projection == null ? null : projectedFacts,
    );
  }

  FlarkV3RecursiveGreenPointQuery _resolveRecursiveGreenInline({
    required FlarkV3StructuralAck authority,
    required FlarkV3RecursiveGreenPointQuery query,
  }) {
    if (!(query.owner.kind?.isInlineBearing ?? false) ||
        query.blockQuoteProjection != null ||
        query.projectedInlineFacts != null) {
      return query;
    }

    final freshParagraphSource = query.paragraphSource;
    final freshInlineSource = query.inlineSource;
    final freshFacts = query.inlineFacts;
    _RecursiveGreenLeafKey? key;
    if (freshParagraphSource != null ||
        freshInlineSource != null ||
        freshFacts != null) {
      if (freshParagraphSource == null ||
          freshInlineSource == null ||
          freshFacts == null ||
          !_recursiveFactsBindQuery(
            freshFacts,
            paragraphSource: freshParagraphSource,
            inlineSource: freshInlineSource,
            query: query,
            authority: authority,
          )) {
        return query;
      }
      key = _RecursiveGreenLeafKey(
        ownerFrameId: query.owner.frameId,
        physicalSource: freshParagraphSource,
        projectedUtf8Length:
            freshInlineSource.endUtf8 - freshInlineSource.startUtf8,
        projectedUtf16Length:
            freshInlineSource.endUtf16 - freshInlineSource.startUtf16,
      );
      _remember(key, freshFacts);
      return query;
    }

    key = _findRecursiveGreenKey(query);
    if (key == null) return query;
    final facts = _touchFacts(key);
    if (facts == null ||
        !_recursiveFactsBindKey(facts, query, authority, key)) {
      if (facts != null) {
        _entries.remove(key);
        _retainedFactRecords -= facts.facts.length;
      }
      return query;
    }
    return query.withPresentationCertificates(
      paragraphSource: key.physicalSource,
      inlineSource: facts.source,
      inlineFacts: facts,
    );
  }

  /// Drops every authority-bearing value synchronously.
  void clear() {
    _authority = null;
    _entries.clear();
    _tightListEntries.clear();
    _recursiveBlockQuoteEntries.clear();
    _recursiveProjectedInlineEntries.clear();
    _retainedFactRecords = 0;
  }

  void _adoptAuthority(FlarkV3StructuralAck authority) {
    if (_authority == authority) return;
    _entries.clear();
    _tightListEntries.clear();
    _recursiveBlockQuoteEntries.clear();
    _recursiveProjectedInlineEntries.clear();
    _retainedFactRecords = 0;
    _authority = authority;
  }

  FlarkV3DocumentStructuralQuery _resolveTightList({
    required FlarkV3StructuralAck authority,
    required FlarkV3DocumentStructuralQuery query,
  }) {
    final freshProjection = switch (query.structure.kind) {
      FlarkV3DocumentStructureKind.bulletList => query.bulletListProjection,
      FlarkV3DocumentStructureKind.orderedList => query.orderedListProjection,
      _ => null,
    };
    final freshFacts = query.inlineFacts;
    _InlineLeafKey? key;

    if (freshProjection != null &&
        _tightListProjectionBindsQuery(freshProjection, query, authority)) {
      key = _InlineLeafKey(
        physicalSource: query.structure.source,
        projectedSource: freshProjection.selectedItem.content,
      );
      _rememberTightList(key, freshProjection);
    }
    if (freshFacts != null &&
        _nestedFactsBindList(freshFacts, query, authority)) {
      key = _InlineLeafKey(
        physicalSource: query.structure.source,
        projectedSource: freshFacts.source,
      );
      _remember(key, freshFacts);
    }
    if (key == null) return query;

    final projection = freshProjection ?? _touchTightList(key);
    final facts = freshFacts ?? _touchFacts(key);
    if (projection == null ||
        facts == null ||
        !_sameSpan(projection.selectedItem.content, facts.source) ||
        !_tightListProjectionBindsQuery(projection, query, authority) ||
        !_nestedFactsBindList(facts, query, authority)) {
      return query;
    }
    return FlarkV3DocumentStructuralQuery(
      sourceRevision: query.sourceRevision,
      structureRevision: query.structureRevision,
      structure: query.structure,
      projection: query.projection,
      inlineFacts: facts,
      indentedCodeProjection: query.indentedCodeProjection,
      pointPath: projection.pointPath,
      blockQuoteProjection: query.blockQuoteProjection,
      bulletListProjection: projection is FlarkV3BulletListProjectionPayload
          ? projection
          : null,
      orderedListProjection: projection is FlarkV3OrderedListProjectionPayload
          ? projection
          : null,
    );
  }

  void _remember(_InlineCacheKey key, FlarkV3InlineFacts facts) {
    final previous = _entries.remove(key);
    if (previous != null) {
      _retainedFactRecords -= previous.facts.length;
    }

    final factRecords = facts.facts.length;
    if (factRecords > maximumFactRecords) return;
    _entries[key] = facts;
    _retainedFactRecords += factRecords;
    _enforceFactBounds(preferProjectedEviction: true);
  }

  void _rememberRecursiveProjectedInline(
    _RecursiveGreenLeafKey key,
    FlarkV3ProjectedInlineFacts facts,
  ) {
    final previous = _recursiveProjectedInlineEntries.remove(key);
    if (previous != null) _retainedFactRecords -= previous.facts.length;
    final factRecords = facts.facts.length;
    if (factRecords > maximumFactRecords) return;
    _recursiveProjectedInlineEntries[key] = facts;
    _retainedFactRecords += factRecords;
    _enforceFactBounds(preferProjectedEviction: false);
  }

  void _enforceFactBounds({required bool preferProjectedEviction}) {
    while (_entries.length + _recursiveProjectedInlineEntries.length >
            maximumEntries ||
        _retainedFactRecords > maximumFactRecords) {
      if (preferProjectedEviction &&
          _recursiveProjectedInlineEntries.isNotEmpty) {
        final evicted = _recursiveProjectedInlineEntries.remove(
          _recursiveProjectedInlineEntries.keys.first,
        )!;
        _retainedFactRecords -= evicted.facts.length;
      } else if (_entries.isNotEmpty) {
        final evicted = _entries.remove(_entries.keys.first)!;
        _retainedFactRecords -= evicted.facts.length;
      } else if (_recursiveProjectedInlineEntries.isNotEmpty) {
        final evicted = _recursiveProjectedInlineEntries.remove(
          _recursiveProjectedInlineEntries.keys.first,
        )!;
        _retainedFactRecords -= evicted.facts.length;
      } else {
        break;
      }
    }
  }

  void _rememberTightList(
    _InlineLeafKey key,
    FlarkV3TightListItemProjectionPayload projection,
  ) {
    _tightListEntries.remove(key);
    _tightListEntries[key] = projection;
    while (_tightListEntries.length > maximumEntries) {
      _tightListEntries.remove(_tightListEntries.keys.first);
    }
  }

  FlarkV3TightListItemProjectionPayload? _touchTightList(_InlineLeafKey key) {
    final projection = _tightListEntries.remove(key);
    if (projection != null) _tightListEntries[key] = projection;
    return projection;
  }

  FlarkV3InlineFacts? _touchFacts(_InlineCacheKey key) {
    final facts = _entries.remove(key);
    if (facts != null) _entries[key] = facts;
    return facts;
  }

  void _rememberRecursiveBlockQuote(
    _RecursiveGreenLeafKey key,
    FlarkV3BlockQuoteProjectionCertificate projection,
  ) {
    _recursiveBlockQuoteEntries.remove(key);
    _recursiveBlockQuoteEntries[key] = projection;
    while (_recursiveBlockQuoteEntries.length > maximumEntries) {
      _recursiveBlockQuoteEntries.remove(
        _recursiveBlockQuoteEntries.keys.first,
      );
    }
  }

  FlarkV3BlockQuoteProjectionCertificate? _touchRecursiveBlockQuote(
    _RecursiveGreenLeafKey key,
  ) {
    final projection = _recursiveBlockQuoteEntries.remove(key);
    if (projection != null) _recursiveBlockQuoteEntries[key] = projection;
    return projection;
  }

  FlarkV3ProjectedInlineFacts? _touchRecursiveProjectedInline(
    _RecursiveGreenLeafKey key,
  ) {
    final facts = _recursiveProjectedInlineEntries.remove(key);
    if (facts != null) _recursiveProjectedInlineEntries[key] = facts;
    return facts;
  }

  _RecursiveGreenLeafKey? _findRecursiveGreenKey(
    FlarkV3RecursiveGreenPointQuery query, {
    FlarkV3SourceSpan? exactPhysicalSource,
  }) {
    bool matches(_RecursiveGreenLeafKey key) =>
        key.ownerFrameId == query.owner.frameId &&
        _containsSpan(key.physicalSource, query.source) &&
        (exactPhysicalSource == null ||
            _sameSpan(key.physicalSource, exactPhysicalSource));
    for (final key in _recursiveBlockQuoteEntries.keys) {
      if (matches(key)) return key;
    }
    for (final key in _recursiveProjectedInlineEntries.keys) {
      if (matches(key)) return key;
    }
    for (final candidate in _entries.keys) {
      if (candidate is _RecursiveGreenLeafKey && matches(candidate)) {
        return candidate;
      }
    }
    return null;
  }
}

bool _queryBindsAuthority(
  FlarkV3DocumentStructuralQuery query,
  FlarkV3StructuralAck authority,
) =>
    query.sourceRevision == authority.sourceVersion.revision &&
    query.structureRevision == authority.sourceVersion.revision;

bool _factsBindQuery(
  FlarkV3InlineFacts facts,
  FlarkV3DocumentStructuralQuery query,
  FlarkV3StructuralAck authority,
) =>
    facts.sourceVersion == authority.sourceVersion &&
    facts.profilePartition == authority.syntaxProfile.value &&
    _containsSpan(query.projection.projectedSource, facts.source) &&
    facts.source.startUtf8 < facts.source.endUtf8;

bool _nestedFactsBindList(
  FlarkV3InlineFacts facts,
  FlarkV3DocumentStructuralQuery query,
  FlarkV3StructuralAck authority,
) =>
    facts.sourceVersion == authority.sourceVersion &&
    facts.profilePartition == authority.syntaxProfile.value &&
    facts.source.startUtf8 < facts.source.endUtf8 &&
    facts.source.startUtf8 >= query.structure.source.startUtf8 &&
    facts.source.endUtf8 <= query.structure.source.endUtf8 &&
    facts.source.startUtf16 >= query.structure.source.startUtf16 &&
    facts.source.endUtf16 <= query.structure.source.endUtf16;

bool _tightListProjectionBindsQuery(
  FlarkV3TightListItemProjectionPayload projection,
  FlarkV3DocumentStructuralQuery query,
  FlarkV3StructuralAck authority,
) =>
    switch (query.structure.kind) {
      FlarkV3DocumentStructureKind.bulletList =>
        projection is FlarkV3BulletListProjectionPayload,
      FlarkV3DocumentStructureKind.orderedList =>
        projection is FlarkV3OrderedListProjectionPayload,
      _ => false,
    } &&
    projection.sourceVersion == authority.sourceVersion &&
    _sameSpan(projection.source, query.structure.source) &&
    projection.selectedItem.content.startUtf8 <
        projection.selectedItem.content.endUtf8;

bool _recursiveQueryBindsAuthority(
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3StructuralAck authority,
) =>
    query.sourceRevision == authority.sourceVersion.revision &&
    query.structureRevision == authority.sourceVersion.revision;

bool _isBlockQuoteParagraph(FlarkV3RecursiveGreenPointQuery query) =>
    query.owner.kind == FlarkV3RecursiveGreenKind.paragraph &&
    query.ancestry
        .take(query.ownerIndex)
        .any(
          (ancestor) => ancestor.kind == FlarkV3RecursiveGreenKind.blockQuote,
        );

bool _recursiveProjectionBindsQuery(
  FlarkV3BlockQuoteProjectionCertificate projection,
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3StructuralAck authority,
) =>
    projection.sourceVersion == authority.sourceVersion &&
    projection.source.startUtf8 < projection.source.endUtf8 &&
    _containsSpan(projection.source, query.source) &&
    (query.paragraphSource == null ||
        _sameSpan(query.paragraphSource!, projection.source));

bool _recursiveProjectionBindsKey(
  FlarkV3BlockQuoteProjectionCertificate projection,
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3StructuralAck authority,
  _RecursiveGreenLeafKey key,
) =>
    key.ownerFrameId == query.owner.frameId &&
    _sameSpan(key.physicalSource, projection.source) &&
    _recursiveProjectionBindsQuery(projection, query, authority);

bool _recursiveFactsBindQuery(
  FlarkV3InlineFacts facts, {
  required FlarkV3SourceSpan paragraphSource,
  required FlarkV3SourceSpan inlineSource,
  required FlarkV3RecursiveGreenPointQuery query,
  required FlarkV3StructuralAck authority,
}) =>
    facts.sourceVersion == authority.sourceVersion &&
    facts.profilePartition == authority.syntaxProfile.value &&
    paragraphSource.startUtf8 < paragraphSource.endUtf8 &&
    _containsSpan(paragraphSource, query.source) &&
    _containsSpan(paragraphSource, inlineSource) &&
    _sameSpan(inlineSource, facts.source) &&
    facts.source.startUtf8 < facts.source.endUtf8;

bool _recursiveFactsBindKey(
  FlarkV3InlineFacts facts,
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3StructuralAck authority,
  _RecursiveGreenLeafKey key,
) =>
    key.ownerFrameId == query.owner.frameId &&
    facts.sourceVersion == authority.sourceVersion &&
    facts.profilePartition == authority.syntaxProfile.value &&
    _containsSpan(key.physicalSource, query.source) &&
    (query.paragraphSource == null ||
        _sameSpan(query.paragraphSource!, key.physicalSource)) &&
    _containsSpan(key.physicalSource, facts.source) &&
    facts.source.startUtf8 < facts.source.endUtf8;

bool _recursiveProjectedFactsBindQuery(
  FlarkV3ProjectedInlineFacts facts, {
  required FlarkV3RecursiveGreenPointQuery query,
  required FlarkV3StructuralAck authority,
  required FlarkV3BlockQuoteProjectionCertificate? projection,
}) =>
    facts.sourceVersion == authority.sourceVersion &&
    facts.profilePartition == authority.syntaxProfile.value &&
    facts.physicalSource.startUtf8 < facts.physicalSource.endUtf8 &&
    _containsSpan(facts.physicalSource, query.source) &&
    (query.paragraphSource == null ||
        _sameSpan(query.paragraphSource!, facts.physicalSource)) &&
    (projection == null ||
        _sameSpan(projection.source, facts.physicalSource) &&
            projection.projectedUtf8Length == facts.projectedUtf8Length &&
            projection.projectedUtf16Length == facts.projectedUtf16Length);

bool _recursiveProjectedFactsBindKey(
  FlarkV3ProjectedInlineFacts facts,
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3StructuralAck authority,
  _RecursiveGreenLeafKey key,
  FlarkV3BlockQuoteProjectionCertificate? projection,
) =>
    key.ownerFrameId == query.owner.frameId &&
    _sameSpan(key.physicalSource, facts.physicalSource) &&
    key.projectedUtf8Length == facts.projectedUtf8Length &&
    key.projectedUtf16Length == facts.projectedUtf16Length &&
    _recursiveProjectedFactsBindQuery(
      facts,
      query: query,
      authority: authority,
      projection: projection,
    );

sealed class _InlineCacheKey {
  const _InlineCacheKey();
}

final class _InlineLeafKey extends _InlineCacheKey {
  const _InlineLeafKey({
    required this.physicalSource,
    required this.projectedSource,
  }) : super();

  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan projectedSource;

  @override
  bool operator ==(Object other) =>
      other is _InlineLeafKey &&
      _sameSpan(other.physicalSource, physicalSource) &&
      _sameSpan(other.projectedSource, projectedSource);

  @override
  int get hashCode =>
      Object.hash(_spanHash(physicalSource), _spanHash(projectedSource));
}

final class _RecursiveGreenLeafKey extends _InlineCacheKey {
  const _RecursiveGreenLeafKey({
    required this.ownerFrameId,
    required this.physicalSource,
    required this.projectedUtf8Length,
    required this.projectedUtf16Length,
  }) : super();

  final BigInt ownerFrameId;
  final FlarkV3SourceSpan physicalSource;
  final int projectedUtf8Length;
  final int projectedUtf16Length;

  @override
  bool operator ==(Object other) =>
      other is _RecursiveGreenLeafKey &&
      other.ownerFrameId == ownerFrameId &&
      _sameSpan(other.physicalSource, physicalSource) &&
      other.projectedUtf8Length == projectedUtf8Length &&
      other.projectedUtf16Length == projectedUtf16Length;

  @override
  int get hashCode => Object.hash(
    ownerFrameId,
    _spanHash(physicalSource),
    projectedUtf8Length,
    projectedUtf16Length,
  );
}

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _containsSpan(FlarkV3SourceSpan parent, FlarkV3SourceSpan child) =>
    parent.startUtf8 <= child.startUtf8 &&
    child.endUtf8 <= parent.endUtf8 &&
    parent.startUtf16 <= child.startUtf16 &&
    child.endUtf16 <= parent.endUtf16;

int _spanHash(FlarkV3SourceSpan span) =>
    Object.hash(span.startUtf8, span.endUtf8, span.startUtf16, span.endUtf16);
