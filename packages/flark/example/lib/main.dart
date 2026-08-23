import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'dogfood_documents.dart';
import 'native_canary_mode.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  const configuredAtBuild = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
  final libraryPath = configuredAtBuild.isEmpty ? null : configuredAtBuild;
  runApp(
    FlarkDogfoodApp(
      libraryPath: libraryPath,
      nativeCanaryMode: DogfoodNativeCanaryMode.fromEnvironment(),
    ),
  );
}

final class FlarkDogfoodApp extends StatefulWidget {
  const FlarkDogfoodApp({this.libraryPath, this.nativeCanaryMode, super.key});

  final String? libraryPath;
  final DogfoodNativeCanaryMode? nativeCanaryMode;

  @override
  State<FlarkDogfoodApp> createState() => _FlarkDogfoodAppState();
}

final class _FlarkDogfoodAppState extends State<FlarkDogfoodApp> {
  FlarkEditorController? _controller;
  DogfoodDocumentPreset _preset = DogfoodDocumentPreset.productTour;
  DogfoodDocumentPreset? _loadingPreset;
  Duration? _generationDuration;
  Duration? _openDuration;
  Object? _loadError;
  int _loadGeneration = 0;
  bool _readOnly = false;
  bool? _streamedOpenSupported;
  DogfoodNativeCanaryReceiptWriter? _canaryReceiptWriter;
  DogfoodNativeCanaryCommandMailbox? _canaryCommandMailbox;
  final FlarkEditorDebugHandle _canaryDebugHandle = FlarkEditorDebugHandle();

  bool get _loading => _loadingPreset != null;

  @override
  void initState() {
    super.initState();
    if (widget.nativeCanaryMode case final mode?) {
      _canaryReceiptWriter = DogfoodNativeCanaryReceiptWriter(mode);
      _canaryCommandMailbox = DogfoodNativeCanaryCommandMailbox(
        path: mode.commandPath,
        onCommand: _handleCanaryCommand,
        onError: (sequence, error) =>
            _canaryReceiptWriter!.writeCommandError(sequence, error),
      )..start();
    }
    unawaited(_probeStreamedOpenSupport());
    final initialPresetName = widget.nativeCanaryMode?.initialPresetName;
    final initialPreset = initialPresetName == null
        ? DogfoodDocumentPreset.productTour
        : DogfoodDocumentPreset.values.byName(initialPresetName);
    _preset = initialPreset;
    unawaited(_load(initialPreset));
  }

  @override
  void dispose() {
    _loadGeneration += 1;
    _canaryCommandMailbox?.dispose();
    _canaryReceiptWriter?.dispose();
    final controller = _controller;
    if (controller != null) unawaited(controller.close());
    super.dispose();
  }

  /// Answers once, at startup, whether the loaded library carries the
  /// streamed-open entry points, so the picker can disable streamed presets
  /// instead of failing an open after the click.
  Future<void> _probeStreamedOpenSupport() async {
    final supported = await FlarkEditorController.streamedOpenSupported(
      libraryPath: widget.libraryPath,
    );
    if (!mounted) return;
    setState(() => _streamedOpenSupported = supported);
  }

  Future<void> _load(DogfoodDocumentPreset preset) async {
    final generationWatch = Stopwatch()..start();
    final String source;
    if (widget.nativeCanaryMode case final mode?
        when mode.initialPresetName == null) {
      source = mode.source;
    } else {
      source = await compute(buildDogfoodDocument, preset);
    }
    generationWatch.stop();
    await _openSource(
      source,
      preset: preset,
      generationDuration: generationWatch.elapsed,
    );
  }

  Future<FlarkEditorController?> _openSource(
    String source, {
    required DogfoodDocumentPreset preset,
    Duration? generationDuration,
  }) async {
    final generation = ++_loadGeneration;
    setState(() {
      _loadingPreset = preset;
      _loadError = null;
    });
    FlarkEditorController? next;
    try {
      final openWatch = Stopwatch()..start();
      // A streamed preset admits the source in transport-sized chunks: the
      // certified head paints and accepts typing while the tail is still
      // being admitted, so the recorded open duration is time-to-editable,
      // not time-to-complete-document. The length stays undeclared —
      // measuring it up front would encode the whole document a second
      // time, which this path exists to avoid — so closing the chunk
      // stream is what ends the load.
      final opened = preset.streamed
          ? await FlarkEditorController.openUtf8Stream(
              _streamSourceChunks(source),
              libraryPath: widget.libraryPath,
            )
          : await FlarkEditorController.open(
              source,
              libraryPath: widget.libraryPath,
            );
      next = opened;
      if (widget.nativeCanaryMode != null) await opened.continueParsing();
      openWatch.stop();
      if (!mounted || generation != _loadGeneration) {
        await opened.close();
        return null;
      }
      final previous = _controller;
      setState(() {
        _controller = next;
        _preset = preset;
        _loadingPreset = null;
        _generationDuration = generationDuration;
        _openDuration = openWatch.elapsed;
      });
      if (previous != null) {
        unawaited(previous.close());
      }
      _canaryReceiptWriter?.attach(opened);
      if (widget.nativeCanaryMode == null) unawaited(opened.continueParsing());
      return opened;
    } catch (error) {
      if (next != null) await next.close();
      if (!mounted || generation != _loadGeneration) return null;
      setState(() {
        _loadingPreset = null;
        _loadError = error;
      });
      rethrow;
    }
  }

  Future<void> _handleCanaryCommand(DogfoodNativeCanaryCommand command) async {
    final writer = _canaryReceiptWriter!;
    switch (command.operation) {
      case 'reset':
        final canaryId = command.arguments['canaryId']! as String;
        final source = command.arguments['source']! as String;
        final controller = await _openSource(
          source,
          preset: _preset,
          generationDuration: Duration.zero,
        );
        if (controller == null) {
          throw StateError('canary reset was cancelled');
        }
        writer.beginCanary(canaryId);
        await _settleCanaryController(controller);
        await _awaitCanaryFrame();
        await writer.writeNow(commandSequence: command.sequence);
        return;
      case 'settle':
        final controller = _controller;
        if (controller == null) throw StateError('canary has no controller');
        await writer.waitForPlatformInputQuiescence();
        await _settleCanaryController(controller);
        await _awaitCanaryFrame();
        await writer.writeNow(commandSequence: command.sequence);
        return;
      case 'closeSession':
        final controller = _controller;
        if (controller == null) throw StateError('canary has no controller');
        await writer.waitForPlatformInputQuiescence();
        await _settleCanaryController(controller);
        final closeRequestedEpochMicros = DateTime.now().microsecondsSinceEpoch;
        final closeRequestedRssBytes = ProcessInfo.currentRss;
        final closeRequestedMaximumRssBytes = ProcessInfo.maxRss;
        writer.detach();
        setState(() => _controller = null);
        await controller.close();
        await Future<void>.delayed(const Duration(milliseconds: 100));
        final globalLiveState = FlarkNativeDocument.inspectGlobalLiveState(
          libraryPath: widget.libraryPath,
        );
        await writer.writeClosed(
          commandSequence: command.sequence,
          closeRequestedEpochMicros: closeRequestedEpochMicros,
          closeRequestedRssBytes: closeRequestedRssBytes,
          closeRequestedMaximumRssBytes: closeRequestedMaximumRssBytes,
          globalLiveState: globalLiveState,
        );
        return;
      case 'lookupSourcePoint':
        final controller = _controller;
        if (controller == null) throw StateError('canary has no controller');
        final offset = command.arguments['utf16Offset']! as int;
        await _settleCanaryController(controller);
        await _awaitCanaryFrame();
        final geometry = _canaryDebugHandle.geometryForSourceUtf16(offset);
        if (geometry == null) {
          throw StateError('source offset $offset is not painted');
        }
        await writer.writeNow(
          commandSequence: command.sequence,
          sourcePointOffset: offset,
          sourcePointGeometry: geometry,
        );
        return;
      case 'lookupTaskCheckboxPoint':
        final controller = _controller;
        if (controller == null) throw StateError('canary has no controller');
        final target = command.arguments['targetUtf16']! as int;
        await _settleCanaryController(controller);
        await _awaitCanaryFrame();
        final row = controller.rows.firstWhere(
          (candidate) =>
              candidate.listItem?.taskChecked != null &&
              candidate.editableUtf16 != null &&
              candidate.editableUtf16!.start <= target &&
              target <= candidate.editableUtf16!.end,
          orElse: () => throw StateError(
            'task target $target is not in a certified task row',
          ),
        );
        final geometry = _canaryDebugHandle.geometryForTaskCheckboxOrdinal(
          row.ordinal,
        );
        if (geometry == null) {
          throw StateError('task checkbox at $target is not painted');
        }
        await writer.writeNow(
          commandSequence: command.sequence,
          taskActionTarget: target,
          taskActionGeometry: geometry,
        );
        return;
      default:
        throw StateError('unsupported canary command ${command.operation}');
    }
  }

  Future<void> _settleCanaryController(FlarkEditorController controller) async {
    // Native canary mode is test-only instrumentation in the dogfood app.
    // ignore: invalid_use_of_visible_for_testing_member
    await controller.debugWaitForPresentationSettled();
    if (controller.lastError case final error?) throw error;
  }

  Future<void> _awaitCanaryFrame() async {
    WidgetsBinding.instance.ensureVisualUpdate();
    await WidgetsBinding.instance.endOfFrame;
  }

  void _showGuide() {
    showDialog<void>(
      context: context,
      builder: (context) => const _DogfoodGuideDialog(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Flark Dogfood',
      theme: ThemeData(
        brightness: Brightness.light,
        scaffoldBackgroundColor: const Color(0xfff4f2ed),
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xff315efb),
          brightness: Brightness.light,
        ),
        fontFamily: '.AppleSystemUIFont',
      ),
      home: Scaffold(
        body: SafeArea(
          child: Column(
            children: [
              _DogfoodToolbar(
                preset: _preset,
                loadingPreset: _loadingPreset,
                onPresetSelected: _loading ? null : _load,
                onReload: _loading ? null : () => _load(_preset),
                onShowGuide: _showGuide,
                readOnly: _readOnly,
                onReadOnlyChanged: (value) => setState(() => _readOnly = value),
                streamedOpenSupported: _streamedOpenSupported,
              ),
              if (controller != null)
                AnimatedBuilder(
                  animation: controller,
                  builder: (context, _) => _DiagnosticsBar(
                    controller: controller,
                    generationDuration: _generationDuration,
                    openDuration: _openDuration,
                  ),
                )
              else
                const SizedBox(height: 34),
              const Divider(height: 1),
              Expanded(
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    if (controller != null && !_readOnly)
                      ColoredBox(
                        color: const Color(0xfffffefa),
                        child: FlarkEditor(
                          key: ValueKey(controller),
                          controller: controller,
                          autofocus: true,
                          debugInputEventObserver:
                              _canaryReceiptWriter?.recordInputEvent,
                          debugPaintObserver:
                              _canaryReceiptWriter?.recordPaintObservation,
                          debugHandle: _canaryReceiptWriter == null
                              ? null
                              : _canaryDebugHandle,
                          textStyle: const TextStyle(
                            color: Color(0xff25272b),
                            fontSize: 17,
                            height: 1.48,
                          ),
                        ),
                      )
                    else if (controller != null)
                      ColoredBox(
                        color: const Color(0xfffffefa),
                        child: FlarkMarkdownView(
                          key: ValueKey(controller),
                          controller: controller,
                          textStyle: const TextStyle(
                            color: Color(0xff25272b),
                            fontSize: 17,
                            height: 1.48,
                          ),
                        ),
                      )
                    else if (_loadError != null)
                      _LoadFailure(
                        error: _loadError!,
                        onRetry: () => _load(_preset),
                      )
                    else
                      const ColoredBox(color: Color(0xfffffefa)),
                    if (_loading) _LoadingOverlay(preset: _loadingPreset!),
                    if (_loadError != null && controller != null)
                      _LoadErrorBanner(
                        error: _loadError!,
                        onDismiss: () => setState(() => _loadError = null),
                      ),
                  ],
                ),
              ),
              const _DogfoodFooter(),
            ],
          ),
        ),
      ),
    );
  }
}

final class _DogfoodToolbar extends StatelessWidget {
  const _DogfoodToolbar({
    required this.preset,
    required this.loadingPreset,
    required this.onPresetSelected,
    required this.onReload,
    required this.onShowGuide,
    required this.readOnly,
    required this.onReadOnlyChanged,
    required this.streamedOpenSupported,
  });

  final DogfoodDocumentPreset preset;
  final DogfoodDocumentPreset? loadingPreset;
  final ValueChanged<DogfoodDocumentPreset>? onPresetSelected;
  final VoidCallback? onReload;
  final VoidCallback onShowGuide;
  final bool readOnly;
  final ValueChanged<bool> onReadOnlyChanged;

  /// Null while the capability probe is still running.
  final bool? streamedOpenSupported;

  @override
  Widget build(BuildContext context) {
    final displayed = loadingPreset ?? preset;
    return LayoutBuilder(
      builder: (context, constraints) => SizedBox(
        height: 58,
        child: Padding(
          padding: EdgeInsets.symmetric(
            horizontal: constraints.maxWidth < 700 ? 12 : 18,
          ),
          child: constraints.maxWidth < 700
              ? _compactToolbar(displayed)
              : _wideToolbar(displayed),
        ),
      ),
    );
  }

  Widget _compactToolbar(DogfoodDocumentPreset displayed) => Row(
    children: [
      const Text(
        'FLARK',
        style: TextStyle(fontWeight: FontWeight.w900, letterSpacing: 1.5),
      ),
      const SizedBox(width: 10),
      Expanded(child: _documentPicker(displayed, compact: true)),
      IconButton(
        onPressed: onReload,
        tooltip: 'Reset this document',
        icon: const Icon(Icons.refresh, size: 20),
      ),
      PopupMenuButton<_DogfoodToolbarAction>(
        tooltip: 'Dogfood actions',
        onSelected: (action) {
          switch (action) {
            case _DogfoodToolbarAction.edit:
              onReadOnlyChanged(false);
              return;
            case _DogfoodToolbarAction.read:
              onReadOnlyChanged(true);
              return;
            case _DogfoodToolbarAction.guide:
              onShowGuide();
              return;
          }
        },
        itemBuilder: (context) => const [
          PopupMenuItem(
            value: _DogfoodToolbarAction.edit,
            child: ListTile(
              leading: Icon(Icons.edit_outlined),
              title: Text('Edit mode'),
            ),
          ),
          PopupMenuItem(
            value: _DogfoodToolbarAction.read,
            child: ListTile(
              leading: Icon(Icons.visibility_outlined),
              title: Text('Read mode'),
            ),
          ),
          PopupMenuItem(
            value: _DogfoodToolbarAction.guide,
            child: ListTile(
              leading: Icon(Icons.fact_check_outlined),
              title: Text('Feedback guide'),
            ),
          ),
        ],
      ),
    ],
  );

  Widget _wideToolbar(DogfoodDocumentPreset displayed) => Row(
    children: [
      const Text(
        'FLARK',
        style: TextStyle(fontWeight: FontWeight.w900, letterSpacing: 1.5),
      ),
      const SizedBox(width: 9),
      DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0xffe4e9ff),
          borderRadius: BorderRadius.circular(999),
        ),
        child: const Padding(
          padding: EdgeInsets.symmetric(horizontal: 9, vertical: 4),
          child: Text(
            'DOGFOOD',
            style: TextStyle(
              color: Color(0xff2548bf),
              fontSize: 11,
              fontWeight: FontWeight.w800,
              letterSpacing: 0.8,
            ),
          ),
        ),
      ),
      const SizedBox(width: 22),
      _documentPicker(displayed),
      const SizedBox(width: 6),
      IconButton(
        onPressed: onReload,
        tooltip: 'Reset this document',
        icon: const Icon(Icons.refresh, size: 20),
      ),
      const SizedBox(width: 8),
      SegmentedButton<bool>(
        showSelectedIcon: false,
        segments: const [
          ButtonSegment(
            value: false,
            icon: Icon(Icons.edit_outlined, size: 17),
            label: Text('EDIT'),
          ),
          ButtonSegment(
            value: true,
            icon: Icon(Icons.visibility_outlined, size: 17),
            label: Text('READ'),
          ),
        ],
        selected: {readOnly},
        onSelectionChanged: (selection) {
          onReadOnlyChanged(selection.single);
        },
      ),
      const Spacer(),
      OutlinedButton.icon(
        onPressed: onShowGuide,
        icon: const Icon(Icons.fact_check_outlined, size: 18),
        label: const Text('FEEDBACK GUIDE'),
      ),
    ],
  );

  Widget _documentPicker(
    DogfoodDocumentPreset displayed, {
    bool compact = false,
  }) => PopupMenuButton<DogfoodDocumentPreset>(
    enabled: onPresetSelected != null,
    tooltip: 'Switch dogfood document',
    onSelected: onPresetSelected,
    itemBuilder: (context) => [
      for (final candidate in DogfoodDocumentPreset.values)
        // A streamed preset needs the opening-session entry points; against
        // an ordinary library the item stays visible but unselectable and
        // says why, instead of failing the open after the click.
        if (candidate.streamed && streamedOpenSupported == false)
          PopupMenuItem(
            value: candidate,
            enabled: false,
            child: SizedBox(
              width: 270,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    candidate.label,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  const SizedBox(height: 2),
                  const Text(
                    'Needs a library built with the opening-session cargo '
                    'feature',
                    style: TextStyle(color: Color(0xff9a6b1a), fontSize: 12),
                  ),
                ],
              ),
            ),
          )
        else
          PopupMenuItem(
            value: candidate,
            child: SizedBox(
              width: 270,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    candidate.label,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    candidate.description,
                    style: const TextStyle(
                      color: Color(0xff6d7179),
                      fontSize: 12,
                    ),
                  ),
                ],
              ),
            ),
          ),
    ],
    child: DecoratedBox(
      decoration: BoxDecoration(
        color: Colors.white,
        border: Border.all(color: const Color(0xffd8d6d0)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(
          horizontal: compact ? 10 : 12,
          vertical: 8,
        ),
        child: Row(
          mainAxisSize: compact ? MainAxisSize.max : MainAxisSize.min,
          children: [
            Flexible(
              child: Text(
                displayed.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
            ),
            const SizedBox(width: 6),
            const Icon(Icons.expand_more, size: 18),
          ],
        ),
      ),
    ),
  );
}

enum _DogfoodToolbarAction { edit, read, guide }

final class _DiagnosticsBar extends StatelessWidget {
  const _DiagnosticsBar({
    required this.controller,
    required this.generationDuration,
    required this.openDuration,
  });

  final FlarkEditorController controller;
  final Duration? generationDuration;
  final Duration? openDuration;

  @override
  Widget build(BuildContext context) {
    final status = controller.status;
    final faulted = status == FlarkEditorStatus.faulted;
    final publicStatus = switch (status) {
      FlarkEditorStatus.opening => 'opening',
      FlarkEditorStatus.streaming => 'streaming',
      FlarkEditorStatus.faulted => 'faulted',
      FlarkEditorStatus.disposed => 'disposed',
      _ => 'live',
    };
    return ColoredBox(
      color: faulted ? const Color(0xffffe9e7) : const Color(0xffeeece7),
      child: SizedBox(
        height: 34,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18),
            child: Row(
              children: [
                _StatusDot(status: status),
                const SizedBox(width: 7),
                _Metric(publicStatus),
                _Separator(),
                _Metric(_formatBytes(controller.sourceByteLength)),
                _Separator(),
                _Metric('page ${controller.viewportPageIndex + 1}'),
                _Separator(),
                _Metric('input ${controller.inputWindowState.name}'),
                _Separator(),
                _Metric(
                  controller.resyncCount == 0
                      ? '0 resyncs'
                      : '${controller.resyncCount} resyncs '
                            '(${controller.lastResyncReason.name})',
                ),
                if (generationDuration case final duration?) ...[
                  _Separator(),
                  _Metric('generated ${_formatDuration(duration)}'),
                ],
                if (openDuration case final duration?) ...[
                  _Separator(),
                  _Metric('opened ${_formatDuration(duration)}'),
                ],
                if (controller.lastError case final error?) ...[
                  _Separator(),
                  Tooltip(
                    message: error.toString(),
                    child: const _Metric(
                      'engine error',
                      color: Color(0xffa1261d),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

final class _StatusDot extends StatelessWidget {
  const _StatusDot({required this.status});

  final FlarkEditorStatus status;

  @override
  Widget build(BuildContext context) {
    final color = switch (status) {
      FlarkEditorStatus.ready => const Color(0xff16834a),
      FlarkEditorStatus.faulted => const Color(0xffc83b30),
      FlarkEditorStatus.disposed => const Color(0xff777777),
      FlarkEditorStatus.opening => const Color(0xffd38812),
      FlarkEditorStatus.streaming => const Color(0xff315efb),
      FlarkEditorStatus.editing ||
      FlarkEditorStatus.parsing => const Color(0xff16834a),
    };
    return DecoratedBox(
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      child: const SizedBox.square(dimension: 8),
    );
  }
}

final class _Metric extends StatelessWidget {
  const _Metric(this.value, {this.color = const Color(0xff555a62)});

  final String value;
  final Color color;

  @override
  Widget build(BuildContext context) => Text(
    value,
    style: TextStyle(
      color: color,
      fontSize: 12,
      fontFeatures: const [FontFeature.tabularFigures()],
    ),
  );
}

final class _Separator extends StatelessWidget {
  @override
  Widget build(BuildContext context) => const Padding(
    padding: EdgeInsets.symmetric(horizontal: 9),
    child: Text('•', style: TextStyle(color: Color(0xffa2a098))),
  );
}

final class _LoadingOverlay extends StatelessWidget {
  const _LoadingOverlay({required this.preset});

  final DogfoodDocumentPreset preset;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xd9fffefa),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 360),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const SizedBox.square(
                dimension: 28,
                child: CircularProgressIndicator(strokeWidth: 3),
              ),
              const SizedBox(height: 18),
              Text(
                'Opening ${preset.label}',
                style: const TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 6),
              Text(
                preset.description,
                textAlign: TextAlign.center,
                style: const TextStyle(color: Color(0xff6b7078)),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

final class _LoadFailure extends StatelessWidget {
  const _LoadFailure({required this.error, required this.onRetry});

  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xfffffefa),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 680),
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(
                  Icons.error_outline,
                  size: 38,
                  color: Color(0xffb72f26),
                ),
                const SizedBox(height: 14),
                const Text(
                  'Flark could not open its native runtime',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 10),
                SelectableText(error.toString(), textAlign: TextAlign.center),
                const SizedBox(height: 18),
                FilledButton(
                  onPressed: onRetry,
                  child: const Text('TRY AGAIN'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

final class _LoadErrorBanner extends StatelessWidget {
  const _LoadErrorBanner({required this.error, required this.onDismiss});

  final Object error;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.topCenter,
      child: Material(
        color: const Color(0xffffe9e7),
        elevation: 2,
        child: ListTile(
          dense: true,
          leading: const Icon(Icons.error_outline, color: Color(0xffb72f26)),
          title: const Text('The requested document did not open.'),
          subtitle: Text(
            error.toString(),
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: IconButton(
            onPressed: onDismiss,
            icon: const Icon(Icons.close),
          ),
        ),
      ),
    );
  }
}

final class _DogfoodFooter extends StatelessWidget {
  const _DogfoodFooter();

  @override
  Widget build(BuildContext context) {
    return const SizedBox(
      height: 32,
      child: Padding(
        padding: EdgeInsets.symmetric(horizontal: 18),
        child: Row(
          children: [
            Icon(Icons.info_outline, size: 15, color: Color(0xff777b82)),
            SizedBox(width: 7),
            Expanded(
              child: Text(
                'Early dogfood: judge typing, scrolling, selection, projection, and obvious jank — not final polish.',
                overflow: TextOverflow.ellipsis,
                style: TextStyle(color: Color(0xff666a72), fontSize: 12),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

final class _DogfoodGuideDialog extends StatelessWidget {
  const _DogfoodGuideDialog();

  static const feedbackTemplate = '''Flark dogfood feedback

Preset:
Action I was taking:
Expected:
Observed:
Visible lag or jank:
Selection/caret issue:
Projection or visual issue:
Anything else:
''';

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('First dogfood pass'),
      content: const SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Please focus on foundational feel and correctness:',
                style: TextStyle(fontWeight: FontWeight.w700),
              ),
              SizedBox(height: 12),
              _GuideItem(
                'Type quickly and watch for delayed or reordered text.',
              ),
              _GuideItem('Drag selections within and across blocks.'),
              _GuideItem('Copy, cut, paste, undo, and redo.'),
              _GuideItem('Scroll and switch presets while parsing is active.'),
              _GuideItem(
                'Edit rendered Markdown and watch incomplete syntax transition locally.',
              ),
              _GuideItem('Switch between Edit and Read and compare rendering.'),
              _GuideItem(
                'Resize the window and inspect wrapping and hit testing.',
              ),
              SizedBox(height: 14),
              Text(
                'Tables still use exact-source editing; mobile controls, final accessibility, themes, and visual polish are not ready for judgment yet.',
                style: TextStyle(color: Color(0xff6a6f77)),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('CLOSE'),
        ),
        FilledButton.icon(
          onPressed: () async {
            await Clipboard.setData(
              const ClipboardData(text: feedbackTemplate),
            );
          },
          icon: const Icon(Icons.copy, size: 17),
          label: const Text('COPY FEEDBACK TEMPLATE'),
        ),
      ],
    );
  }
}

final class _GuideItem extends StatelessWidget {
  const _GuideItem(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Padding(
          padding: EdgeInsets.only(top: 2),
          child: Icon(
            Icons.check_circle_outline,
            size: 17,
            color: Color(0xff315efb),
          ),
        ),
        const SizedBox(width: 9),
        Expanded(child: Text(text)),
      ],
    ),
  );
}

String _formatBytes(int bytes) {
  if (bytes >= 1024 * 1024) {
    return '${(bytes / (1024 * 1024)).toStringAsFixed(2)} MiB';
  }
  return '${(bytes / 1024).toStringAsFixed(1)} KiB';
}

String _formatDuration(Duration duration) {
  if (duration.inMilliseconds < 1000) return '${duration.inMilliseconds} ms';
  return '${(duration.inMilliseconds / 1000).toStringAsFixed(2)} s';
}

/// Emits [source] as transport-sized UTF-8 chunks, encoding one slice at a
/// time so no second complete copy of the document is ever allocated. Cuts
/// land between UTF-16 code units of different scalars — never inside a
/// surrogate pair — and yields between chunks so the editor keeps painting
/// and accepting input while admission continues.
Stream<Uint8List> _streamSourceChunks(String source) async* {
  const targetUnits = 32 * 1024;
  var start = 0;
  while (start < source.length) {
    var end = math.min(start + targetUnits, source.length);
    // A cut immediately after a high surrogate would split one scalar.
    if (end < source.length &&
        _isHighSurrogate(source.codeUnitAt(end - 1)) &&
        _isLowSurrogate(source.codeUnitAt(end))) {
      end -= 1;
    }
    yield Uint8List.fromList(utf8.encode(source.substring(start, end)));
    start = end;
    await Future<void>.delayed(Duration.zero);
  }
}

bool _isHighSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;

bool _isLowSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;
