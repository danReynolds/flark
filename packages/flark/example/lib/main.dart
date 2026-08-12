import 'dart:async';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'dogfood_documents.dart';
import 'scenario_mode.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final configured = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final libraryPath =
      configured ??
      File(
        '../../../native/comrak_bridge/target/release/libflark_abi.dylib',
      ).absolute.path;
  runApp(
    FlarkDogfoodApp(
      libraryPath: libraryPath,
      scenarioMode: DogfoodScenarioMode.fromEnvironment(),
    ),
  );
}

final class FlarkDogfoodApp extends StatefulWidget {
  const FlarkDogfoodApp({
    required this.libraryPath,
    this.scenarioMode,
    super.key,
  });

  final String libraryPath;
  final DogfoodScenarioMode? scenarioMode;

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
  DogfoodScenarioReceiptWriter? _scenarioReceiptWriter;
  DogfoodScenarioCommandMailbox? _scenarioCommandMailbox;
  final FlarkEditorDebugHandle _scenarioDebugHandle = FlarkEditorDebugHandle();

  bool get _loading => _loadingPreset != null;

  @override
  void initState() {
    super.initState();
    if (widget.scenarioMode case final mode?) {
      _scenarioReceiptWriter = DogfoodScenarioReceiptWriter(mode);
      if (mode.commandPath case final commandPath?) {
        _scenarioCommandMailbox = DogfoodScenarioCommandMailbox(
          path: commandPath,
          onCommand: _handleScenarioCommand,
          onError: (sequence, error) =>
              _scenarioReceiptWriter!.writeCommandError(sequence, error),
        )..start();
      }
    }
    unawaited(_load(DogfoodDocumentPreset.productTour));
  }

  @override
  void dispose() {
    _loadGeneration += 1;
    _scenarioCommandMailbox?.dispose();
    _scenarioReceiptWriter?.dispose();
    final controller = _controller;
    if (controller != null) unawaited(controller.close());
    super.dispose();
  }

  Future<void> _load(DogfoodDocumentPreset preset) async {
    final generationWatch = Stopwatch()..start();
    final String source;
    if (widget.scenarioMode case final mode?) {
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
      final opened = await FlarkEditorController.open(
        source,
        libraryPath: widget.libraryPath,
      );
      next = opened;
      if (widget.scenarioMode != null) await opened.continueParsing();
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
      _scenarioReceiptWriter?.attach(opened);
      if (widget.scenarioMode == null) unawaited(opened.continueParsing());
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

  Future<void> _handleScenarioCommand(DogfoodScenarioCommand command) async {
    final writer = _scenarioReceiptWriter!;
    switch (command.operation) {
      case 'reset':
        final scenarioId = command.arguments['scenarioId']! as String;
        final source = command.arguments['source']! as String;
        final controller = await _openSource(
          source,
          preset: _preset,
          generationDuration: Duration.zero,
        );
        if (controller == null) {
          throw StateError('scenario reset was cancelled');
        }
        writer.beginScenario(scenarioId);
        await _settleScenarioController(controller);
        await _awaitScenarioFrame();
        await writer.writeNow(commandSequence: command.sequence);
        return;
      case 'settle':
        final controller = _controller;
        if (controller == null) throw StateError('scenario has no controller');
        await writer.waitForPlatformInputQuiescence();
        await _settleScenarioController(controller);
        await _awaitScenarioFrame();
        await writer.writeNow(commandSequence: command.sequence);
        return;
      case 'lookupSourcePoint':
        final controller = _controller;
        if (controller == null) throw StateError('scenario has no controller');
        final offset = command.arguments['utf16Offset']! as int;
        await _settleScenarioController(controller);
        await _awaitScenarioFrame();
        final geometry = _scenarioDebugHandle.geometryForSourceUtf16(offset);
        if (geometry == null) {
          throw StateError('source offset $offset is not painted');
        }
        await writer.writeNow(
          commandSequence: command.sequence,
          sourcePointOffset: offset,
          sourcePointGeometry: geometry,
        );
        return;
      default:
        throw StateError('unsupported scenario command ${command.operation}');
    }
  }

  Future<void> _settleScenarioController(
    FlarkEditorController controller,
  ) async {
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 1));
    }
    if (controller.pendingEdits != 0) {
      throw StateError('scenario edit did not settle in 5 seconds');
    }
    await controller.continueParsing();
    if (controller.lastError case final error?) throw error;
  }

  Future<void> _awaitScenarioFrame() async {
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
                              _scenarioReceiptWriter?.recordInputEvent,
                          debugPaintObserver:
                              _scenarioReceiptWriter?.recordPaintObservation,
                          debugHandle: _scenarioReceiptWriter == null
                              ? null
                              : _scenarioDebugHandle,
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
  });

  final DogfoodDocumentPreset preset;
  final DogfoodDocumentPreset? loadingPreset;
  final ValueChanged<DogfoodDocumentPreset>? onPresetSelected;
  final VoidCallback? onReload;
  final VoidCallback onShowGuide;
  final bool readOnly;
  final ValueChanged<bool> onReadOnlyChanged;

  @override
  Widget build(BuildContext context) {
    final displayed = loadingPreset ?? preset;
    return SizedBox(
      height: 58,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 18),
        child: Row(
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
            PopupMenuButton<DogfoodDocumentPreset>(
              enabled: onPresetSelected != null,
              tooltip: 'Switch dogfood document',
              onSelected: onPresetSelected,
              itemBuilder: (context) => [
                for (final candidate in DogfoodDocumentPreset.values)
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
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        displayed.label,
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(width: 8),
                      const Icon(Icons.expand_more, size: 18),
                    ],
                  ),
                ),
              ),
            ),
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
        ),
      ),
    );
  }
}

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
                _Metric('${controller.resyncCount} resyncs'),
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
