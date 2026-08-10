import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final matrix = _object('../../test/fixtures/v4/input_window_matrix_v1.json');
  final limits = _map(matrix['limits']);
  final states = _list(matrix['states']).toSet();
  final results = _list(matrix['results']).toSet();
  final transitions = _list(matrix['transitions']).map(_map).toList();
  final byId = {
    for (final transition in transitions)
      transition['id'] as String: transition,
  };

  test(
    'pins the complete bounded input-window state machine and scenarios',
    () {
      expect(states, {
        'detached',
        'synchronized',
        'composition_pinned',
        'bulk_staging',
        'resync_required',
        'closed',
        'faulted',
      });
      expect(results, {
        'accepted',
        'deferred',
        'ignored_retired',
        'needs_more_context',
        'staged_bulk_required',
        'bulk_progress',
        'resync_required',
        'closed',
        'faulted',
      });
      expect(byId, hasLength(transitions.length));
      expect(byId.keys, {
        'IW-ATTACH-001',
        'IW-LOCAL-BATCH-ACCEPT-001',
        'IW-LOCAL-BATCH-CHAIN-REJECT-001',
        'IW-FULL-VALUE-FALLBACK-001',
        'IW-WINDOW-MOVE-SAME-REVISION-001',
        'IW-STALE-WINDOW-CALLBACK-001',
        'IW-OLD-TEXT-MISMATCH-001',
        'IW-IME-START-001',
        'IW-IME-UPDATE-001',
        'IW-COMPOSITION-MOVE-DEFER-001',
        'IW-COMPOSITION-MOVE-COALESCE-001',
        'IW-IME-COMMIT-APPLY-DEFERRED-001',
        'IW-IME-CANCEL-RESTORE-001',
        'IW-IME-EXTERNAL-REVISION-CONFLICT-001',
        'IW-IME-OVER-CAP-001',
        'IW-IME-STALE-CALLBACK-001',
        'IW-IME-UNPROVABLE-BOUNDARY-001',
        'IW-IME-STALE-GENERATION-001',
        'IW-CROSS-EDGE-EXPAND-001',
        'IW-CROSS-EDGE-BULK-001',
        'IW-STALE-REVISION-001',
        'IW-RESYNC-NEW-CONNECTION-001',
        'IW-RETIRED-AFTER-RESYNC-CALLBACK-001',
        'IW-IME-BULK-BEGIN-001',
        'IW-IME-BULK-PUMP-001',
        'IW-IME-BULK-COMPLETE-001',
        'IW-MULTIDELTA-BULK-BEGIN-001',
        'IW-MULTIDELTA-BULK-PUMP-001',
        'IW-MULTIDELTA-BULK-COMPLETE-001',
        'IW-OVERSIZED-SELECTION-EXPOSE-001',
        'IW-OVERSIZED-SELECTION-REPLACE-001',
        'IW-OVERSIZED-SELECTION-BULK-PUMP-001',
        'IW-OVERSIZED-SELECTION-BULK-COMPLETE-001',
        'IW-STALE-SELECTION-CALLBACK-001',
        'IW-CURRENT-SELECTION-CALLBACK-001',
        'IW-SPLIT-SCALAR-REJECT-001',
        'IW-SPLIT-CRLF-REJECT-001',
        'IW-SPLIT-EGC-REJECT-001',
        'IW-SCALAR-WINDOW-ADJUST-001',
        'IW-CRLF-WINDOW-ADJUST-001',
        'IW-EGC-NEEDS-MORE-CONTEXT-001',
        'IW-EGC-CONTEXT-EXPAND-001',
        'IW-EGC-EXPAND-RETRY-001',
        'IW-EGC-EXCEEDS-WINDOW-001',
        'IW-CLOSE-001',
        'IW-FAULT-001',
      });
    },
  );

  test('binds every resolved selection to flark_core-owned Rust anchors', () {
    final authority = _map(matrix['selectionAuthority']);
    final snapshots = _list(authority['canonicalSnapshots']).map(_map).toList();
    final byAuthority = <String, Map<String, Object?>>{};
    final anchorHandles = <int>{};
    for (final snapshot in snapshots) {
      final revision = snapshot['namedRevision']! as int;
      final generation = snapshot['generation']! as int;
      final key = '$revision:$generation';
      expect(
        byAuthority,
        isNot(contains(key)),
        reason: '$key must identify exactly one canonical selection',
      );
      byAuthority[key] = snapshot;
      expect(revision, greaterThan(0));
      expect(generation, greaterThan(0));
      for (final field in const ['baseAnchor', 'extentAnchor']) {
        final handle = snapshot[field]! as int;
        expect(handle, greaterThan(0));
        expect(
          anchorHandles.add(handle),
          isTrue,
          reason: 'reused $field $handle',
        );
      }
      expect(snapshot['affinity'], anyOf('upstream', 'downstream'));
      expect(_ints(snapshot['resolvedUtf16']), hasLength(2));
    }
    expect(
      snapshots.map((snapshot) => snapshot['affinity']).toSet(),
      containsAll({'upstream', 'downstream'}),
    );

    final usedKeys = <String>{};
    void expectCanonicalSelection(
      String transitionId,
      int revision,
      Map<String, Object?> selection, {
      required String location,
    }) {
      final generation = selection['generation']! as int;
      final key = '$revision:$generation';
      final snapshot = byAuthority[key];
      expect(
        snapshot,
        isNotNull,
        reason: '$transitionId $location lacks $key anchors',
      );
      expect(snapshot!['namedRevision'], revision);
      expect(snapshot['generation'], generation);
      expect(
        _ints(snapshot['resolvedUtf16']),
        [selection['base'], selection['extent']],
        reason:
            '$transitionId $location integer offsets must be a resolved '
            'projection, not canonical selection authority',
      );
      usedKeys.add(key);
    }

    for (final transition in transitions) {
      for (final rawState in [transition['from'], transition['to']]) {
        final state = _map(rawState);
        final selection = _map(state['selection']);
        final revision = state['sourceRevision']! as int;
        expectCanonicalSelection(
          transition['id']! as String,
          revision,
          selection,
          location: 'state',
        );

        final composition = state['composition'];
        if (composition is Map &&
            composition['baseRevision'] != null &&
            composition['precompositionSelection'] != null) {
          expectCanonicalSelection(
            transition['id']! as String,
            composition['baseRevision']! as int,
            _map(composition['precompositionSelection']),
            location: 'precomposition selection',
          );
        }
      }
    }
    expect(usedKeys, byAuthority.keys.toSet());
    expect(authority['canonicalOwner'], 'flark_core');
    expect(
      authority['transitionSelectionMeaning'],
      contains('projections only'),
    );
  });

  test('freezes callback authority and bounds every represented window', () {
    final authorityFields = _list(matrix['callbackAuthorityFields']).toSet();
    final callbackKinds = _list(matrix['callbackKinds']).toSet();
    final transport = _map(matrix['authorityTransport']);
    expect(authorityFields, {
      'representedRevision',
      'connectionEpoch',
      'windowEpoch',
      'oldWindowTextSha256',
      'selectionGeneration',
    });
    expect(_list(transport['platformFields']).toSet(), {
      'text_input_client_id',
      'ordered_editing_value_or_delta_batch',
    });
    expect(_list(transport['hostAttachedFields']).toSet(), authorityFields);
    expect(transport['authorityObjectMeaning'], contains('not echoed'));
    expect(
      matrix['compositionSourcePolicy'],
      allOf(
        contains('canonical Rust source revision'),
        contains('one user undo unit'),
      ),
    );
    final selectionAuthority = _map(matrix['selectionAuthority']);
    expect(selectionAuthority['canonicalOwner'], 'flark_core');
    expect(
      selectionAuthority['canonicalRepresentation'],
      allOf(contains('Rust'), contains('affinity'), contains('generation')),
    );
    expect(selectionAuthority['rustOwner'], contains('anchor transform'));
    expect(selectionAuthority['flutterOwner'], contains('shadow'));
    expect(
      _map(matrix['invariants'])['selection'],
      allOf(
        contains('flark_core owns'),
        contains('Rust owns source'),
        contains('Flutter owns only'),
      ),
    );

    for (final transition in transitions) {
      final from = _map(transition['from']);
      final to = _map(transition['to']);
      final event = _map(transition['event']);
      expect(states, contains(from['state']));
      expect(states, contains(to['state']));
      expect(results, contains(to['result']));
      _expectWindowBounded(from, limits);
      _expectWindowBounded(to, limits);

      if (from['window'] != null &&
          to['window'] != null &&
          _ints(from['window']).first != _ints(to['window']).first) {
        expect(
          to['connectionEpoch'],
          isNot(from['connectionEpoch']),
          reason: '${transition['id']} changed local offset identity',
        );
        expect(to['windowEpoch'], 1);
        expect(to['platformCommands'], [
          'close_connection',
          'open_connection',
          'set_editing_state',
        ]);
      }

      if (callbackKinds.contains(event['kind'])) {
        final authority = _map(event['authority']);
        expect(authority.keys.toSet(), containsAll(authorityFields));
        final frozenAuthority = [
          authority['representedRevision'],
          authority['connectionEpoch'],
          authority['windowEpoch'],
          authority['oldWindowTextSha256'],
          authority['selectionGeneration'],
        ];
        final currentAuthority = [
          from['sourceRevision'],
          from['connectionEpoch'],
          from['windowEpoch'],
          from['windowTextSha256'],
          _map(from['selection'])['generation'],
        ];
        if ({
          'retired_connection_epoch',
          'stale_revision',
        }.contains(to['reason'])) {
          expect(frozenAuthority, isNot(currentAuthority));
        } else {
          expect(frozenAuthority, currentAuthority);
        }
      }

      if (to['state'] == 'composition_pinned') {
        final window = _ints(to['window']);
        final composition = _ints(_map(to['composition'])['range']);
        expect(composition.first, greaterThanOrEqualTo(window.first));
        expect(composition.last, lessThanOrEqualTo(window.last));
        expect(
          composition.last - composition.first,
          lessThanOrEqualTo(limits['maximumCompositionUtf16'] as int),
        );
      }
    }
  });

  test(
    'validates delta batches atomically and detects stale window identity',
    () {
      final accepted = byId['IW-LOCAL-BATCH-ACCEPT-001']!;
      final acceptedEvent = _map(accepted['event']);
      final acceptedDeltas = _list(acceptedEvent['deltas']).map(_map).toList();
      expect(acceptedDeltas, hasLength(2));
      expect(
        acceptedDeltas.first['oldTextSha256'],
        _map(accepted['from'])['windowTextSha256'],
      );
      expect(
        acceptedDeltas.first['newTextSha256'],
        acceptedDeltas.last['oldTextSha256'],
      );
      expect(
        acceptedEvent['proposedFullValueSha256'],
        acceptedDeltas.last['newTextSha256'],
      );
      expect(
        _map(accepted['to'])['windowTextSha256'],
        acceptedEvent['proposedFullValueSha256'],
      );
      _expectOneAtomicMutation(accepted);

      final fullValue = byId['IW-FULL-VALUE-FALLBACK-001']!;
      final fullValueEvent = _map(fullValue['event']);
      expect(
        fullValueEvent['oldTextSha256'],
        _map(fullValue['from'])['windowTextSha256'],
      );
      expect(
        fullValueEvent['newTextSha256'],
        _map(fullValue['to'])['windowTextSha256'],
      );
      _expectOneAtomicMutation(fullValue);

      final rejected = byId['IW-LOCAL-BATCH-CHAIN-REJECT-001']!;
      final rejectedDeltas = _list(
        _map(rejected['event'])['deltas'],
      ).map(_map).toList();
      expect(
        rejectedDeltas.first['newTextSha256'],
        isNot(rejectedDeltas.last['oldTextSha256']),
      );
      _expectRejectedWithoutSourceMutation(
        rejected,
        reason: 'delta_text_chain_mismatch',
      );

      final moved = byId['IW-WINDOW-MOVE-SAME-REVISION-001']!;
      final movedFrom = _map(moved['from']);
      final movedTo = _map(moved['to']);
      expect(movedTo['sourceRevision'], movedFrom['sourceRevision']);
      expect(
        movedTo['connectionEpoch'],
        (movedFrom['connectionEpoch'] as int) + 1,
      );
      expect(movedTo['windowEpoch'], 1);
      expect(movedTo['windowTextSha256'], isNot(movedFrom['windowTextSha256']));
      expect(movedTo['platformCommands'], [
        'close_connection',
        'open_connection',
        'set_editing_state',
      ]);
      _expectIgnoredRetired(byId['IW-STALE-WINDOW-CALLBACK-001']!);
      _expectRejectedWithoutSourceMutation(
        byId['IW-OLD-TEXT-MISMATCH-001']!,
        reason: 'old_window_text_mismatch',
      );
      _expectRejectedWithoutSourceMutation(
        byId['IW-STALE-REVISION-001']!,
        reason: 'stale_revision',
      );
    },
  );

  test('admits the complete callback batch against one runtime envelope', () {
    final contract = _map(matrix['smallEditEnvelope']);
    final descriptorBytes = contract['descriptorSizeBytes']! as int;
    final maximumBytes = contract['maximumTotalBytes']! as int;
    expect(maximumBytes, limits['maximumSmallEditEnvelopeBytes']);
    expect(descriptorBytes, 32);
    expect(_list(contract['components']), [
      'descriptor_bytes',
      'packed_replacement_utf8_bytes',
      'deleted_source_utf8_bytes',
    ]);
    expect(contract['admissionScope'], contains('complete ordered callback'));
    expect(
      contract['replacementPartition'],
      allOf(
        contains('descriptor order'),
        contains('no gaps'),
        contains('reuse'),
      ),
    );

    final admittedIds = <String>{};
    final stagedIds = <String>{};
    for (final transition in transitions) {
      final event = _map(transition['event']);
      if (event['runtimeEditEnvelope'] == null) continue;
      final envelope = _map(event['runtimeEditEnvelope']);
      final descriptorCount = envelope['descriptorCount']! as int;
      final replacementBytes = envelope['packedReplacementUtf8Bytes']! as int;
      final deletedBytes = envelope['deletedSourceUtf8Bytes']! as int;
      final total = envelope['totalBytes']! as int;
      expect(descriptorCount, inInclusiveRange(1, 64));
      expect(replacementBytes, greaterThanOrEqualTo(0));
      expect(deletedBytes, greaterThanOrEqualTo(0));
      expect(
        total,
        descriptorCount * descriptorBytes + replacementBytes + deletedBytes,
        reason: '${transition['id']} envelope arithmetic',
      );
      if (event['kind'] == 'replace_local_batch') {
        expect(
          descriptorCount,
          _list(event['deltas']).length,
          reason: '${transition['id']} admits the aggregate delta batch',
        );
      }

      final to = _map(transition['to']);
      switch (envelope['path']) {
        case 'small_edit':
          expect(total, lessThanOrEqualTo(maximumBytes));
          _expectOneAtomicMutation(transition);
          admittedIds.add(transition['id']! as String);
          break;
        case 'bulk_transaction':
          expect(total, greaterThan(maximumBytes));
          expect(to['state'], 'bulk_staging');
          expect(to['result'], 'staged_bulk_required');
          final pending = _map(to['pendingBulk']);
          expect(pending['totalEnvelopeBytes'], total);
          expect(pending['descriptorCount'], descriptorCount);
          expect(pending['packedReplacementUtf8Bytes'], replacementBytes);
          expect(pending['deletedSourceUtf8Bytes'], deletedBytes);
          _expectNoSourceMutation(transition);
          stagedIds.add(transition['id']! as String);
          break;
        default:
          fail('${transition['id']} has unknown runtime path');
      }
    }
    expect(admittedIds, contains('IW-IME-START-001'));
    expect(admittedIds, contains('IW-LOCAL-BATCH-ACCEPT-001'));
    expect(stagedIds, {
      'IW-CROSS-EDGE-BULK-001',
      'IW-IME-BULK-BEGIN-001',
      'IW-MULTIDELTA-BULK-BEGIN-001',
      'IW-OVERSIZED-SELECTION-REPLACE-001',
    });

    final multidelta = _map(byId['IW-MULTIDELTA-BULK-BEGIN-001']!['event']);
    final deltas = _list(multidelta['deltas']).map(_map).toList();
    expect(deltas, hasLength(2));
    expect(
      deltas
          .map((delta) => delta['replacementUtf8Bytes']! as int)
          .every((bytes) => bytes < maximumBytes),
      isTrue,
    );
    expect(
      _map(multidelta['runtimeEditEnvelope'])['totalBytes'],
      greaterThan(maximumBytes),
      reason: 'individually small deltas are admitted as one aggregate batch',
    );
  });

  test('bulk begin and pumps are nonmutating until one atomic completion', () {
    _expectBulkSequence(
      byId['IW-IME-BULK-BEGIN-001']!,
      byId['IW-IME-BULK-PUMP-001']!,
      byId['IW-IME-BULK-COMPLETE-001']!,
    );
    _expectBulkSequence(
      byId['IW-MULTIDELTA-BULK-BEGIN-001']!,
      byId['IW-MULTIDELTA-BULK-PUMP-001']!,
      byId['IW-MULTIDELTA-BULK-COMPLETE-001']!,
    );
    _expectBulkSequence(
      byId['IW-OVERSIZED-SELECTION-REPLACE-001']!,
      byId['IW-OVERSIZED-SELECTION-BULK-PUMP-001']!,
      byId['IW-OVERSIZED-SELECTION-BULK-COMPLETE-001']!,
    );

    final imeCompletion = _map(byId['IW-IME-BULK-COMPLETE-001']!['to']);
    expect(imeCompletion['state'], 'composition_pinned');
    expect(_ints(_map(imeCompletion['composition'])['range']), [1500, 5596]);
    expect(
      _ints(_map(imeCompletion['composition'])['range']).last -
          _ints(_map(imeCompletion['composition'])['range']).first,
      limits['maximumCompositionUtf16'],
    );
  });

  test(
    'pins IME atomicity, cancellation, conflicts, and latest-wins movement',
    () {
      final start = byId['IW-IME-START-001']!;
      final update = byId['IW-IME-UPDATE-001']!;
      _expectOneAtomicMutation(start);
      _expectOneAtomicMutation(update);
      expect(_map(start['event'])['inputModel'], 'delta');
      expect(_map(update['event'])['inputModel'], 'full_value');
      expect(
        _map(update['event'])['replacementDerivation'],
        'bounded_diff_against_serialized_shadow',
      );
      final startTo = _map(start['to']);
      expect(startTo['state'], 'composition_pinned');
      expect(
        _map(startTo['composition'])['precompositionSelection'],
        isNotNull,
      );

      final firstMove = _map(byId['IW-COMPOSITION-MOVE-DEFER-001']!['to']);
      final latestMove = _map(byId['IW-COMPOSITION-MOVE-COALESCE-001']!['to']);
      expect(firstMove['result'], 'deferred');
      expect(_map(firstMove['pendingWindowDemand'])['ordinal'], 1);
      expect(firstMove['platformCommands'], isNull);
      expect(_map(latestMove['pendingWindowDemand']), {
        'requestedWindow': [1876, 3924],
        'ordinal': 2,
      });
      expect(latestMove['platformCommands'], isNull);

      final commit = byId['IW-IME-COMMIT-APPLY-DEFERRED-001']!;
      _expectOneAtomicMutation(commit);
      final commitTo = _map(commit['to']);
      expect(commitTo['composition'], isNull);
      expect(commitTo['pendingWindowDemand'], isNull);
      expect(commitTo['window'], [1876, 3924]);
      expect(commitTo['windowEpoch'], 1);
      expect(commitTo['platformCommands'], [
        'close_connection',
        'open_connection',
        'set_editing_state',
      ]);

      final cancel = byId['IW-IME-CANCEL-RESTORE-001']!;
      _expectOneAtomicMutation(cancel);
      final cancelFrom = _map(cancel['from']);
      final precomposition = _map(cancelFrom['composition']);
      final precompositionSelection = _map(
        precomposition['precompositionSelection'],
      );
      final cancelTo = _map(cancel['to']);
      expect(cancelTo['restoredPrecomposition'], isTrue);
      expect(cancelTo['composition'], isNull);
      expect(cancelTo['pendingWindowDemand'], isNull);
      expect(cancelTo['appliedPendingDemandOrdinal'], 3);
      expect(cancelTo['window'], [476, 2524]);
      expect(cancelTo['windowEpoch'], 1);
      expect(
        cancelTo['cancelledCompositionGlobalRange'],
        precomposition['range'],
      );
      expect(
        cancelTo['restoredPrecompositionGlobalRange'],
        precomposition['precompositionGlobalRange'],
      );
      expect(
        cancelTo['restoredPrecompositionSourceSliceUtf8'],
        precomposition['precompositionSourceSliceUtf8'],
      );
      expect(
        utf8.encode(precomposition['precompositionSourceSliceUtf8']! as String),
        hasLength(precomposition['precompositionSourceSliceUtf8Bytes']! as int),
      );
      expect(
        cancelTo['restoredPrecompositionSourceSliceUtf8Bytes'],
        precomposition['precompositionSourceSliceUtf8Bytes'],
      );
      expect(
        cancelTo['restoredPrecompositionSourceSliceSha256'],
        precomposition['precompositionSourceSliceSha256'],
      );
      expect(
        precomposition['precompositionSourceSliceSha256'],
        'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
      );
      expect(
        _map(cancelTo['selection'])['base'],
        precompositionSelection['base'],
      );
      expect(
        _map(cancelTo['selection'])['extent'],
        precompositionSelection['extent'],
      );

      final conflictTo = _map(
        byId['IW-IME-EXTERNAL-REVISION-CONFLICT-001']!['to'],
      );
      expect(conflictTo['reason'], 'external_revision_during_composition');
      expect(conflictTo['composition'], isNull);
      expect(conflictTo['platformCommands'], [
        'clear_composing',
        'close_connection',
      ]);
      _expectRejectedWithoutSourceMutation(
        byId['IW-IME-OVER-CAP-001']!,
        reason: 'composition_too_large',
      );
      for (final entry in {
        'IW-IME-OVER-CAP-001': 'composition_too_large',
        'IW-IME-STALE-CALLBACK-001': 'composition_old_text_mismatch',
        'IW-IME-UNPROVABLE-BOUNDARY-001': 'unproven_composition_boundary',
        'IW-IME-STALE-GENERATION-001': 'stale_composition_generation',
      }.entries) {
        final transition = byId[entry.key]!;
        _expectRejectedWithoutSourceMutation(transition, reason: entry.value);
        expect(_map(transition['to'])['platformCommands'], [
          'clear_composing',
          'close_connection',
        ]);
      }
    },
  );

  test('preserves oversized selection intent and generation authority', () {
    final exposed = _map(byId['IW-OVERSIZED-SELECTION-EXPOSE-001']!['to']);
    expect(exposed['selection'], {
      'base': 100,
      'extent': 50000,
      'generation': 40,
    });
    expect(exposed['platformSelectionLocal'], [1024, 1024]);
    expect(exposed['hostOriginatedReconnect'], isTrue);
    expect(exposed['windowEpoch'], 1);

    final replacement = byId['IW-OVERSIZED-SELECTION-REPLACE-001']!;
    expect(_map(replacement['event'])['intent'], 'replace_current_selection');
    final pending = _map(_map(replacement['to'])['pendingBulk']);
    expect(pending['globalReplacementRange'], [100, 50000]);
    expect(
      _map(replacement['to'])['surrogateCaretUsedAsInsertionPoint'],
      isFalse,
    );
    _expectNoSourceMutation(replacement);
    final completion = byId['IW-OVERSIZED-SELECTION-BULK-COMPLETE-001']!;
    expect(_map(completion['to'])['globalReplacementRange'], [100, 50000]);
    expect(
      _map(completion['to'])['surrogateCaretUsedAsInsertionPoint'],
      isFalse,
    );
    _expectOneAtomicMutation(completion);

    final staleSelection = byId['IW-STALE-SELECTION-CALLBACK-001']!;
    expect(
      _map(staleSelection['event'])['platformClientIdState'],
      'retired_after_host_selection_exposure',
    );
    _expectIgnoredRetired(staleSelection);
    final current = byId['IW-CURRENT-SELECTION-CALLBACK-001']!;
    final currentFrom = _map(current['from']);
    final currentTo = _map(current['to']);
    expect(currentTo['sourceMutation'], isFalse);
    expect(currentTo['sourceRevision'], currentFrom['sourceRevision']);
    expect(
      _map(currentTo['selection'])['generation'],
      (_map(currentFrom['selection'])['generation'] as int) + 1,
    );
  });

  test('pins bounded bulk and Unicode boundary outcomes', () {
    _expectOneAtomicMutation(byId['IW-CROSS-EDGE-EXPAND-001']!);
    final bulk = byId['IW-CROSS-EDGE-BULK-001']!;
    expect(_map(bulk['to'])['result'], 'staged_bulk_required');
    _expectNoSourceMutation(bulk);

    _expectRejectedWithoutSourceMutation(
      byId['IW-SPLIT-SCALAR-REJECT-001']!,
      reason: 'split_scalar_boundary',
    );
    _expectRejectedWithoutSourceMutation(
      byId['IW-SPLIT-CRLF-REJECT-001']!,
      reason: 'split_crlf_boundary',
    );
    _expectRejectedWithoutSourceMutation(
      byId['IW-SPLIT-EGC-REJECT-001']!,
      reason: 'split_grapheme_boundary',
    );
    final scalar = _map(byId['IW-SCALAR-WINDOW-ADJUST-001']!['to']);
    expect(scalar['boundaryAdjustment'], 'expand_scalar');
    expect(scalar['window'], [2000, 4049]);
    expect(scalar['hostOriginatedReconnect'], isTrue);
    final crlf = _map(byId['IW-CRLF-WINDOW-ADJUST-001']!['to']);
    expect(crlf['boundaryAdjustment'], 'expand_crlf');
    expect(crlf['window'], [2000, 4049]);
    expect(crlf['hostOriginatedReconnect'], isTrue);

    final needsContext = byId['IW-EGC-NEEDS-MORE-CONTEXT-001']!;
    final needsContextTo = _map(needsContext['to']);
    expect(needsContextTo['result'], 'needs_more_context');
    expect(_map(needsContextTo['pendingWindowDemand']), {
      'kind': 'bounded_grapheme_context_expansion',
      'direction': 'upstream',
    });
    _expectNoSourceMutation(needsContext);
    final expansion = byId['IW-EGC-CONTEXT-EXPAND-001']!;
    final expansionFrom = _map(expansion['from']);
    final expansionTo = _map(expansion['to']);
    for (final field in [
      'sourceRevision',
      'connectionEpoch',
      'windowEpoch',
      'window',
      'windowTextSha256',
      'selection',
      'pendingWindowDemand',
    ]) {
      expect(expansionFrom[field], needsContextTo[field]);
    }
    _expectNoSourceMutation(expansion);
    expect(expansionTo['connectionEpoch'], 16);
    expect(expansionTo['windowEpoch'], 1);
    expect(expansionTo['window'], [3072, 6144]);
    expect(expansionTo['pendingWindowDemand'], isNull);
    expect(expansionTo['platformCommands'], [
      'close_connection',
      'open_connection',
      'set_editing_state',
    ]);
    final retry = byId['IW-EGC-EXPAND-RETRY-001']!;
    final retryFrom = _map(retry['from']);
    for (final field in [
      'sourceRevision',
      'connectionEpoch',
      'windowEpoch',
      'window',
      'windowTextSha256',
      'selection',
      'pendingWindowDemand',
    ]) {
      expect(retryFrom[field], expansionTo[field]);
    }
    expect(_map(retry['event'])['retryOf'], 'IW-EGC-NEEDS-MORE-CONTEXT-001');
    _expectOneAtomicMutation(retry);
    _expectRejectedWithoutSourceMutation(
      byId['IW-EGC-EXCEEDS-WINDOW-001']!,
      reason: 'grapheme_exceeds_window',
    );

    final resync = byId['IW-RESYNC-NEW-CONNECTION-001']!;
    final resyncFrom = _map(resync['from']);
    final resyncTo = _map(resync['to']);
    expect(
      resyncTo['connectionEpoch'],
      (resyncFrom['connectionEpoch'] as int) + 1,
    );
    expect(resyncTo['windowEpoch'], 1);
    _expectIgnoredRetired(byId['IW-RETIRED-AFTER-RESYNC-CALLBACK-001']!);
  });

  test('all explicit rejection paths are mutation-free', () {
    for (final id in [
      'IW-LOCAL-BATCH-CHAIN-REJECT-001',
      'IW-STALE-WINDOW-CALLBACK-001',
      'IW-OLD-TEXT-MISMATCH-001',
      'IW-IME-OVER-CAP-001',
      'IW-IME-STALE-CALLBACK-001',
      'IW-IME-UNPROVABLE-BOUNDARY-001',
      'IW-IME-STALE-GENERATION-001',
      'IW-CROSS-EDGE-BULK-001',
      'IW-STALE-REVISION-001',
      'IW-RETIRED-AFTER-RESYNC-CALLBACK-001',
      'IW-STALE-SELECTION-CALLBACK-001',
      'IW-SPLIT-SCALAR-REJECT-001',
      'IW-SPLIT-CRLF-REJECT-001',
      'IW-SPLIT-EGC-REJECT-001',
      'IW-EGC-NEEDS-MORE-CONTEXT-001',
      'IW-EGC-EXCEEDS-WINDOW-001',
    ]) {
      _expectNoSourceMutation(byId[id]!);
    }
  });
}

void _expectBulkSequence(
  Map<String, Object?> begin,
  Map<String, Object?> pump,
  Map<String, Object?> complete,
) {
  final beginFrom = _map(begin['from']);
  final beginTo = _map(begin['to']);
  final pumpFrom = _map(pump['from']);
  final pumpTo = _map(pump['to']);
  final completeFrom = _map(complete['from']);
  final completeTo = _map(complete['to']);

  expect(beginTo['state'], 'bulk_staging');
  expect(beginTo['result'], 'staged_bulk_required');
  expect(pumpTo['state'], 'bulk_staging');
  expect(pumpTo['result'], 'bulk_progress');
  for (final transition in [begin, pump]) {
    _expectNoSourceMutation(transition);
    expect(_map(transition['to'])['atomicCommitCount'], 0);
  }
  for (final field in [
    'sourceRevision',
    'connectionEpoch',
    'windowEpoch',
    'window',
    'selection',
    'composition',
    'pendingBulk',
  ]) {
    expect(pumpFrom[field], beginTo[field], reason: '${begin['id']} -> pump');
    expect(
      completeFrom[field],
      pumpTo[field],
      reason: '${pump['id']} -> completion',
    );
  }
  final beginPending = _map(beginTo['pendingBulk']);
  final pumpPending = _map(pumpTo['pendingBulk']);
  expect(beginPending['phase'], 'staged');
  expect(pumpPending['phase'], 'commit_pending');
  expect(
    pumpPending['progressToken'],
    isNot(beginPending['progressToken']),
    reason: '${pump['id']} must advance its resumable token',
  );
  expect(_map(pump['event'])['progressToken'], beginPending['progressToken']);
  expect(
    _map(complete['event'])['progressToken'],
    pumpPending['progressToken'],
  );
  expect(completeTo['pendingBulk'], isNull);
  expect(completeTo['bulkTransactionComplete'], isTrue);
  _expectOneAtomicMutation(complete);
  expect(
    completeTo['sourceRevision'],
    (beginFrom['sourceRevision'] as int) + 1,
  );
  expect(
    _map(completeTo['selection'])['generation'],
    (_map(beginFrom['selection'])['generation'] as int) + 1,
  );
}

void _expectOneAtomicMutation(Map<String, Object?> transition) {
  final from = _map(transition['from']);
  final to = _map(transition['to']);
  expect(to['sourceMutation'], isTrue);
  expect(to['atomicCommitCount'], 1);
  expect(to['sourceRevision'], (from['sourceRevision'] as int) + 1);
  if (to['connectionEpoch'] == from['connectionEpoch']) {
    expect(to['windowEpoch'], (from['windowEpoch'] as int) + 1);
  } else {
    expect(to['connectionEpoch'], (from['connectionEpoch'] as int) + 1);
    expect(to['windowEpoch'], 1);
    expect(to['hostOriginatedReconnect'], isTrue);
  }
  expect(
    _map(to['selection'])['generation'],
    (_map(from['selection'])['generation'] as int) + 1,
  );
}

void _expectRejectedWithoutSourceMutation(
  Map<String, Object?> transition, {
  required String reason,
}) {
  final to = _map(transition['to']);
  expect(to['state'], 'resync_required');
  expect(to['result'], 'resync_required');
  expect(to['reason'], reason);
  _expectNoSourceMutation(transition);
}

void _expectIgnoredRetired(Map<String, Object?> transition) {
  final from = _map(transition['from']);
  final to = _map(transition['to']);
  expect(to['state'], from['state']);
  expect(to['result'], 'ignored_retired');
  expect(to['reason'], 'retired_connection_epoch');
  expect(to['activeConnectionMutation'], isFalse);
  expect(to['connectionEpoch'], from['connectionEpoch']);
  expect(to['windowEpoch'], from['windowEpoch']);
  expect(to['window'], from['window']);
  expect(to['windowTextSha256'], from['windowTextSha256']);
  expect(to['platformCommands'], isNull);
  _expectNoSourceMutation(transition);
}

void _expectNoSourceMutation(Map<String, Object?> transition) {
  final from = _map(transition['from']);
  final to = _map(transition['to']);
  expect(to['sourceMutation'], isFalse);
  expect(to['sourceRevision'], from['sourceRevision']);
  expect(to['selection'], from['selection']);
}

void _expectWindowBounded(
  Map<String, Object?> state,
  Map<String, Object?> limits,
) {
  if (state['window'] == null) return;
  final window = _ints(state['window']);
  expect(window, hasLength(2));
  expect(window.first, greaterThanOrEqualTo(0));
  expect(window.last, greaterThanOrEqualTo(window.first));
  expect(
    window.last - window.first,
    lessThanOrEqualTo(limits['maximumWindowUtf16'] as int),
  );
  expect(
    state['windowUtf8Bytes'],
    lessThanOrEqualTo(limits['maximumWindowUtf8'] as int),
  );
  expect(state['connectionEpoch'], greaterThan(0));
  expect(state['windowEpoch'], greaterThan(0));
  expect(
    state['windowTextSha256'],
    isA<String>().having(
      (value) => RegExp(r'^[0-9a-f]{64}$').hasMatch(value),
      'lowercase SHA-256',
      isTrue,
    ),
  );
  expect(state['windowBoundaryProof'], 'scalar_crlf_grapheme');
}

List<int> _ints(Object? value) => _list(value).cast<int>();

Map<String, Object?> _object(String path) =>
    _map(jsonDecode(File(path).readAsStringSync()));

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Object? value) => (value as List).cast<Object?>();
