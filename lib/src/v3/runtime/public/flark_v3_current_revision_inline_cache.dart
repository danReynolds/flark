import 'dart:collection';

import '../../host/flark_v3_host_protocol.dart';
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

  final LinkedHashMap<_InlineLeafKey, FlarkV3InlineFacts> _entries =
      LinkedHashMap<_InlineLeafKey, FlarkV3InlineFacts>();
  final LinkedHashMap<_InlineLeafKey, FlarkV3TightListItemProjectionPayload>
  _tightListEntries =
      LinkedHashMap<_InlineLeafKey, FlarkV3TightListItemProjectionPayload>();
  FlarkV3StructuralAck? _authority;
  int _retainedFactRecords = 0;

  int get entryCount => _entries.length;
  int get retainedFactRecords => _retainedFactRecords;

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

  /// Drops every authority-bearing value synchronously.
  void clear() {
    _authority = null;
    _entries.clear();
    _tightListEntries.clear();
    _retainedFactRecords = 0;
  }

  void _adoptAuthority(FlarkV3StructuralAck authority) {
    if (_authority == authority) return;
    _entries.clear();
    _tightListEntries.clear();
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

  void _remember(_InlineLeafKey key, FlarkV3InlineFacts facts) {
    final previous = _entries.remove(key);
    if (previous != null) {
      _retainedFactRecords -= previous.facts.length;
    }

    final factRecords = facts.facts.length;
    if (factRecords > maximumFactRecords) return;
    _entries[key] = facts;
    _retainedFactRecords += factRecords;
    while (_entries.length > maximumEntries ||
        _retainedFactRecords > maximumFactRecords) {
      final oldestKey = _entries.keys.first;
      final evicted = _entries.remove(oldestKey)!;
      _retainedFactRecords -= evicted.facts.length;
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

  FlarkV3InlineFacts? _touchFacts(_InlineLeafKey key) {
    final facts = _entries.remove(key);
    if (facts != null) _entries[key] = facts;
    return facts;
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

final class _InlineLeafKey {
  const _InlineLeafKey({
    required this.physicalSource,
    required this.projectedSource,
  });

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
