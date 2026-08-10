import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

void main() {
  final matrix = _object('test/fixtures/v4/live_projection_matrix_v1.json');
  final cases = _list(matrix['cases']).map(_map).toList();

  test('owns every required live-projection behavior category', () {
    expect(cases.map((entry) => entry['id']).toSet(), hasLength(cases.length));
    expect(
      cases.map((entry) => entry['category']).toSet(),
      containsAll({
        'incomplete_syntax',
        'marker_reveal_hide',
        'selection_hidden_markers',
        'split_merge',
        'paste',
        'undo_redo',
        'composition',
        'non_local_dependency',
        'pending_to_certified',
        'line_endings',
      }),
    );
    expect(
      cases.map((entry) => entry['currentStatus']),
      everyElement(anyOf('seed_evidence', 'missing')),
    );
  });

  test('records exact source, UI state, certification, and terminal state', () {
    expect(
      matrix['semanticFactRule'],
      allOf(
        contains('exact nonzero revision'),
        contains('wholly covered'),
        contains('neutral source'),
        contains('unrelated certified'),
      ),
    );
    final terminalOutcomes = _list(matrix['terminalOutcomes']).toSet();
    final certificationStates = _list(matrix['certificationStates']).toSet();
    const sourceMutatingActions = <String>{
      'replace',
      'replace_selection',
      'insert',
      'staged_paste',
      'undo_token_replay',
      'redo_token_replay',
      'ime_delta',
    };
    var sawMixedCertificationWithCurrentFacts = false;

    for (final entry in cases) {
      final initial = _map(entry['initial']);
      _expectUiState(initial, requireTerminal: false);
      var revision = initial['revision'] as int;
      expect(revision, greaterThan(0));
      _expectFactsUseCertifiedAuthority(entry['id']! as String, initial);
      for (final rawStep in _list(entry['steps'])) {
        final step = _map(rawStep);
        final actionKind = _map(step['action'])['kind']! as String;
        expect(actionKind, isNotEmpty);
        final expected = _map(step['expected']);
        _expectUiState(expected, requireTerminal: true);
        final nextRevision = expected['revision'] as int;
        expect(
          nextRevision,
          sourceMutatingActions.contains(actionKind) ? revision + 1 : revision,
          reason:
              '${entry['id']} must use nonzero committed revisions and mint '
              'exactly one revision per source-changing action',
        );
        revision = nextRevision;
        expect(terminalOutcomes, contains(expected['terminalOutcome']));

        final certification = _map(expected['certification']);
        expect(
          certification.values,
          everyElement(isIn(certificationStates)),
          reason: '${entry['id']} used an undeclared certification state',
        );
        _expectFactsUseCertifiedAuthority(entry['id']! as String, expected);
        final hasPending = certification.values.any(
          (value) => value != 'certified',
        );
        final hasCertified = certification.values.contains('certified');
        if (hasPending &&
            hasCertified &&
            _list(expected['semanticFacts']).isNotEmpty) {
          sawMixedCertificationWithCurrentFacts = true;
        }
      }
    }

    expect(
      sawMixedCertificationWithCurrentFacts,
      isTrue,
      reason:
          'the matrix must preserve certified distant facts while edited '
          'ranges remain neutral pending',
    );
  });
}

void _expectFactsUseCertifiedAuthority(
  String caseId,
  Map<String, Object?> state,
) {
  final certification = _map(state['certification']);
  final certificationRanges = _map(state['certificationRanges']);
  expect(
    certificationRanges.keys.toSet(),
    certification.keys.toSet(),
    reason: '$caseId must range every certification authority exactly once',
  );
  final rangesByAuthority = <String, List<List<int>>>{
    for (final entry in certificationRanges.entries)
      entry.key: _list(
        entry.value,
      ).map((value) => _list(value).cast<int>()).toList(),
  };
  final source = state.containsKey('source')
      ? state['source']! as String
      : state['exactSource']! as String;
  for (final entry in rangesByAuthority.entries) {
    expect(
      entry.value,
      isNotEmpty,
      reason: '$caseId ${entry.key} has no range',
    );
    for (final range in entry.value) {
      expect(range, hasLength(2));
      expect(range.first, lessThan(range.last));
      if (source != 'fixture-derived') {
        expect(range.last, lessThanOrEqualTo(source.length));
      }
    }
  }

  final nonCertifiedRanges = <List<int>>[
    for (final entry in certification.entries)
      if (entry.value != 'certified') ...rangesByAuthority[entry.key]!,
  ];
  final facts = _list(state['semanticFacts']).map(_map);
  for (final fact in facts) {
    final authority = fact['authority'];
    expect(authority, isA<String>(), reason: '$caseId fact lacks authority');
    expect(
      fact['revision'],
      state['revision'],
      reason: '$caseId exposed a semantic fact from a different revision',
    );
    final sourceRange = _list(fact['sourceRange']).cast<int>();
    expect(sourceRange, hasLength(2));
    expect(sourceRange.first, lessThan(sourceRange.last));
    expect(
      certification[authority],
      'certified',
      reason:
          '$caseId exposed a semantic fact outside an explicitly certified '
          'range authority',
    );
    expect(
      rangesByAuthority[authority]!.any(
        (certifiedRange) =>
            certifiedRange.first <= sourceRange.first &&
            sourceRange.last <= certifiedRange.last,
      ),
      isTrue,
      reason: '$caseId fact is not wholly covered by its certified authority',
    );
    expect(
      nonCertifiedRanges.any(
        (range) =>
            sourceRange.first < range.last && range.first < sourceRange.last,
      ),
      isFalse,
      reason: '$caseId fact intersects pending or source-gap authority',
    );
  }
}

void _expectUiState(
  Map<String, Object?> state, {
  required bool requireTerminal,
}) {
  final source = state[requireTerminal ? 'exactSource' : 'source'];
  expect(source, isA<String>());
  expect(state['revision'], isA<int>());
  final anchors = _map(state['anchors']);
  expect(anchors.keys.toSet(), {'base', 'extent', 'affinity'});
  expect(state['selection'], isA<List>());
  expect(state, contains('composition'));
  expect(state['visibleText'], isA<String>());
  expect(state['certification'], isA<Map>());
  expect(state['certificationRanges'], isA<Map>());
  expect(state['semanticFacts'], isA<List>());
  if (requireTerminal) expect(state['terminalOutcome'], isA<String>());
}

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();
