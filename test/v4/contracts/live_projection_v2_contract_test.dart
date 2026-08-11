import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

void main() {
  final profile = _object('test/fixtures/v4/live_projection_profile_v2.json');
  final matrixPath = profile['matrix']! as String;
  final matrix = _object(matrixPath);
  final families = _list(matrix['caseFamilies']).map(_map).toList();

  test('pins the continuously rendered v2 matrix by digest', () {
    expect(profile['profileId'], 'flark-live-v2');
    expect(matrix['profileId'], profile['profileId']);
    expect(
      sha256.convert(File(matrixPath).readAsBytesSync()).toString(),
      profile['matrixSha256'],
    );
  });

  test('pins the scoped T2 Mac continuity checkpoint', () {
    final receipt = _map(profile['t2MacDevelopmentReceipt']);
    expect(receipt['evidenceClass'], contains('checkpoint'));
    expect(receipt['fixtureShape'], 'dense-inline');
    expect(receipt['measuredEdits'], 120);
    expect(receipt['servedDisplayHz'], 120);
    expect(receipt['rawProjectionFrames'], 0);
    expect(receipt['missingActiveProjectionFrames'], 0);
    expect(receipt['editorLatencyP99Ms'], lessThan(16));
    expect(receipt['editorAttributedOverBudget'], 0);
    expect(receipt['wallClockClaimEligible'], isFalse);
    expect(receipt['wallClockDeferral'], contains('foreground'));
  });

  test('owns the complete projection behavior denominator', () {
    expect(
      families.map((entry) => entry['id']).toSet(),
      hasLength(families.length),
    );
    expect(
      families.map((entry) => entry['category']).toSet(),
      containsAll({
        'focus_selection_stability',
        'hidden_boundary_affinity',
        'platform_selection_normalization',
        'inline_insertion',
        'syntax_transitions',
        'replacement_deletion',
        'split_merge_paste_history',
        'cross_block_page_selection',
        'composition_lifecycle',
        'grapheme_emoji_bidi_line_endings',
        'pending_nonlocal_invalidation',
        'source_gap_fault_oversized_fallback',
        'gfm_construct_behavior',
        'desktop_mobile_gestures',
        'editor_view_parity',
        'bounded_shape_performance',
      }),
    );
    final statuses = _map(matrix['statusDefinitions']).keys.toSet();
    for (final family in families) {
      expect(family['ownerTranche'], isIn({'T1', 'T2', 'T3', 'T4', 'T5'}));
      expect(family['status'], isIn(statuses));
      expect(family['evidence'], isA<String>());
    }
  });

  test('records source, anchors, projection authority, and outcomes', () {
    final requiredState = _list(matrix['requiredStateFields']).cast<String>();
    final requiredPresentation = _list(
      matrix['requiredPresentationFields'],
    ).cast<String>();
    final executable = _list(matrix['executableCases']).map(_map).toList();
    expect(executable, isNotEmpty);
    expect(
      executable.map((entry) => entry['id']),
      everyElement(isIn(families.map((entry) => entry['id']))),
    );
    for (final entry in executable) {
      _expectState(_map(entry['initial']), requiredState, requiredPresentation);
      var revision = _map(entry['initial'])['revision']! as int;
      for (final rawStep in _list(entry['steps'])) {
        final step = _map(rawStep);
        expect(_map(step['action'])['kind'], isA<String>());
        final expected = _map(step['expected']);
        _expectState(expected, requiredState, requiredPresentation);
        expect(expected['revision'], greaterThanOrEqualTo(revision));
        revision = expected['revision']! as int;
      }
    }
  });
}

void _expectState(
  Map<String, Object?> state,
  List<String> requiredState,
  List<String> requiredPresentation,
) {
  expect(state.keys, containsAll(requiredState));
  expect(state['exactSource'], isA<String>());
  expect(state['revision'], greaterThan(0));
  final anchors = _map(state['anchors']);
  expect(anchors.keys, containsAll({'base', 'extent', 'affinity'}));
  for (final rawPresentation in _list(state['presentation'])) {
    final presentation = _map(rawPresentation);
    expect(presentation.keys, containsAll(requiredPresentation));
    final range = _list(presentation['sourceRange']).cast<int>();
    expect(range, hasLength(2));
    expect(range.first, lessThan(range.last));
    expect(presentation['visibleRuns'], isA<List>());
    expect(presentation['legalCaretStops'], isA<List>());
    expect(presentation['certificationAuthority'], isA<String>());
  }
}

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();
