import 'dart:async';
import 'dart:math' as math;

import 'document.dart';
import 'editor_coordinator.dart';
import 'editor_session.dart';

sealed class FlarkEditorParseStep {
  const FlarkEditorParseStep();
}

/// Parsing cannot advance while the editor is closed or composition pins the
/// current source presentation.
final class FlarkEditorParseStopped extends FlarkEditorParseStep {
  const FlarkEditorParseStopped();
}

/// A streamed source sealed and can now converge through the ordinary parser
/// path.
final class FlarkEditorParseOpeningSealed extends FlarkEditorParseStep {
  const FlarkEditorParseOpeningSealed();
}

sealed class FlarkEditorParsePublication extends FlarkEditorParseStep {
  FlarkEditorParsePublication._({
    required FlarkEditorParseDriver owner,
    required FlarkEditorStamp stamp,
  }) : _owner = owner,
       _stamp = stamp;

  final FlarkEditorParseDriver _owner;
  final FlarkEditorStamp _stamp;

  int get editGeneration => _stamp.editGeneration;
}

/// A newer parser-certified streamed head is ready for a bounded viewport
/// refresh.
final class FlarkEditorParseOpeningPublication
    extends FlarkEditorParsePublication {
  FlarkEditorParseOpeningPublication._({
    required super.owner,
    required super.stamp,
    required this.revision,
    required this.certifiedByteEnd,
  }) : super._();

  final int revision;
  final int certifiedByteEnd;
  bool _settled = false;
}

/// The current edit/adoption tails have reached parser-ready source and the
/// host may refresh its bounded viewport.
final class FlarkEditorParseReadyPublication
    extends FlarkEditorParsePublication {
  FlarkEditorParseReadyPublication._({
    required super.owner,
    required super.stamp,
  }) : super._();
}

/// Owns native parser progression and its generation barriers.
///
/// The driver never installs host input or render state and never calls back
/// into a frontend. It returns one typed step whenever the host must adopt a
/// viewport or observe the streamed-to-buffered phase transition.
final class FlarkEditorParseDriver {
  FlarkEditorParseDriver({
    required FlarkCoreDocument document,
    required FlarkCoreEditorSession session,
    required FlarkEditorCoordinator coordinator,
    int workUnits = 512,
    int openingHeadProbeBytes = 4 * 1024,
    int viewportRowsPerPage = 32,
  }) : _document = document,
       _session = session,
       _coordinator = coordinator,
       _workUnits = workUnits,
       _openingHeadProbeBytes = openingHeadProbeBytes,
       _viewportRowsPerPage = viewportRowsPerPage,
       _startedOpening = document.isOpening {
    if (workUnits <= 0 ||
        openingHeadProbeBytes <= 0 ||
        viewportRowsPerPage <= 0) {
      throw ArgumentError('Parser driver bounds must be positive');
    }
    if (_startedOpening) {
      unawaited(
        _document.openingSealed.then<void>(
          (_) {},
          onError: (Object error, StackTrace stackTrace) {
            _openingError = error;
            _openingErrorStackTrace = stackTrace;
          },
        ),
      );
    }
  }

  final FlarkCoreDocument _document;
  final FlarkCoreEditorSession _session;
  final FlarkEditorCoordinator _coordinator;
  final int _workUnits;
  final int _openingHeadProbeBytes;
  final int _viewportRowsPerPage;
  final bool _startedOpening;

  int _openingPublishedCertifiedEnd = -1;
  bool _reportedOpeningSeal = false;
  Object? _openingError;
  StackTrace? _openingErrorStackTrace;

  Future<FlarkEditorParseStep> next() async {
    while (_document.isOpening && !_stopped) {
      _throwOpeningError();
      await _document.pump(workUnits: _workUnits);
      if (_stopped) return const FlarkEditorParseStopped();
      if (!_document.isOpening) break;

      final probe = await _document.queryViewport(
        endByte: math.min(_document.sourceByteLength, _openingHeadProbeBytes),
        maxRows: _viewportRowsPerPage,
      );
      final certified = probe.isCertified && probe.rows.isNotEmpty;
      final certifiedEnd = certified ? probe.rows.last.sourceBytes.end : 0;
      final upgraded =
          probe.revision != _coordinator.openingPublishedRevision ||
          certifiedEnd > _openingPublishedCertifiedEnd;
      if (probe.continuation != 0) {
        await _document.releaseViewportContinuation(probe);
      }
      if (!certified || !upgraded || _coordinator.pendingEdits != 0) continue;
      return FlarkEditorParseOpeningPublication._(
        owner: this,
        stamp: _coordinator.stamp,
        revision: probe.revision,
        certifiedByteEnd: certifiedEnd,
      );
    }

    _throwOpeningError();
    if (_stopped) return const FlarkEditorParseStopped();
    if (_startedOpening && !_reportedOpeningSeal) {
      _reportedOpeningSeal = true;
      return const FlarkEditorParseOpeningSealed();
    }

    parseLoop:
    while (!_stopped) {
      final stamp = _coordinator.stamp;
      final editBarrier = _coordinator.editTail;
      final adoptionBarrier = _coordinator.sourceEditAdoptionTail;
      await Future.wait([editBarrier, adoptionBarrier]);
      if (_stopped) return const FlarkEditorParseStopped();
      if (!_coordinator.accepts(stamp) ||
          !identical(editBarrier, _coordinator.editTail) ||
          !identical(adoptionBarrier, _coordinator.sourceEditAdoptionTail)) {
        continue;
      }
      while (!_document.isReady && !_coordinator.closed) {
        await _document.pump(workUnits: _workUnits);
        if (_session.compositionActive) {
          return const FlarkEditorParseStopped();
        }
        if (!_coordinator.accepts(stamp)) continue parseLoop;
      }
      if (_stopped) return const FlarkEditorParseStopped();
      if (!_coordinator.accepts(stamp)) continue;
      return FlarkEditorParseReadyPublication._(owner: this, stamp: stamp);
    }
    return const FlarkEditorParseStopped();
  }

  bool accepts(FlarkEditorParsePublication publication) {
    _requireOwned(publication);
    return _coordinator.accepts(publication._stamp);
  }

  /// Records an opening publication only if its generation is still current.
  bool adoptOpening(FlarkEditorParseOpeningPublication publication) {
    _requireOwned(publication);
    if (publication._settled) {
      throw StateError('Opening parse publication is already settled');
    }
    publication._settled = true;
    if (!_coordinator.accepts(publication._stamp)) return false;
    _coordinator.recordOpeningPublication(publication.revision);
    _openingPublishedCertifiedEnd = math.max(
      _openingPublishedCertifiedEnd,
      publication.certifiedByteEnd,
    );
    return true;
  }

  bool get _stopped => _coordinator.closed || _session.compositionActive;

  void _throwOpeningError() {
    final error = _openingError;
    if (error != null) {
      Error.throwWithStackTrace(error, _openingErrorStackTrace!);
    }
  }

  void _requireOwned(FlarkEditorParsePublication publication) {
    if (!identical(publication._owner, this)) {
      throw StateError('Parse publication belongs to another driver');
    }
  }
}
