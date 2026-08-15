// Run from example/:
// flutter run -d web-server --release --web-hostname 127.0.0.1 \
//   --web-port 8765 -t lib/v3_live_editor_checkpoint.dart

import 'dart:async';

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/material.dart';

import 'v3_engine_lab.dart' show v3EngineLabWebAssets;

/// Small product-shaped document for the first multi-block Web checkpoint.
///
/// This remains canonical Markdown inside [FlarkV3DocumentRuntime]. The
/// visible surface receives only parser-authored, marker-free presentations.
const String v3LiveCheckpointMarkdown =
    '# A live document\n'
    '\n'
    'Write with **bold**, _emphasis_, `inline code`, ~~strikethrough~~, and '
    'escaped \\* punctuation while Flark keeps '
    'canonical Markdown exact.  \n'
    'A parser-certified hard break keeps its trailing spaces out of view.\n'
    '\n'
    'Browse <https://commonmark.org> or email <hello@example.com>. '
    'Try https://example.test/a or www.example.test/b or me@example.test '
    'as marker-free GFM bare autolinks. '
    'Parser-certified references &copy; and &ngE; render as cooked text. '
    'URI <https://e.test/?q=&amp;> cooks the same source token in its visible '
    'label and destination. '
    'Links stay marker-free while their exact targets remain parser-owned.\n'
    '\n'
    'Read the [Flark architecture notes]'
    '(https://flark.dev/revision-7 "Revision 7") beside a '
    '![Local architecture preview]'
    '(asset://checkpoint/architecture "Placeholder only"). '
    'Direct link and image syntax stays hidden; the image remains a safe '
    'labelled placeholder until the app supplies a resolver.\n'
    '\n'
    'Reference forms stay live too: [full reference][launch notes], '
    '[collapsed reference][], [shortcut reference], and '
    '![Reference architecture][reference image].\n'
    '\n'
    '[launch notes]: https://flark.dev/launch "Launch notes"\n'
    '[collapsed reference]: https://flark.dev/collapsed "Collapsed form"\n'
    '[shortcut reference]: https://commonmark.org "Shortcut form"\n'
    '[reference image]: asset://checkpoint/reference "Reference image"\n'
    '\n'
    '## A second idea\n'
    '\n'
    '```dart\n'
    "final message = 'Hello from Flark';\n"
    '```\n'
    '\n'
    'Tap any block to move the live editor, then start typing.';

void main() => runApp(const V3LiveEditorCheckpointApp());

class V3LiveEditorCheckpointApp extends StatelessWidget {
  const V3LiveEditorCheckpointApp({
    super.key,
    this.openOnStart = true,
    this.webAssets,
    this.surfaceController,
    this.onRuntimeOpened,
  });

  final bool openOnStart;
  final FlarkV3WebRuntimeAssets? webAssets;
  final FlarkV3VirtualizedLiveSurfaceController? surfaceController;

  /// Optional observation seam for checkpoint tests and diagnostics.
  ///
  /// The virtualized surface intentionally does not expose source bytes. This
  /// callback lets a checkpoint owner inspect the already-owned runtime
  /// without weakening that package-level boundary.
  final ValueChanged<FlarkV3DocumentRuntime>? onRuntimeOpened;

  @override
  Widget build(BuildContext context) {
    const seed = Color(0xFF315C55);
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Flark live editor checkpoint',
      theme: ThemeData(
        useMaterial3: true,
        brightness: Brightness.light,
        colorScheme: ColorScheme.fromSeed(
          seedColor: seed,
          surface: const Color(0xFFFFFCF6),
        ),
        scaffoldBackgroundColor: const Color(0xFFF4EFE5),
        textSelectionTheme: const TextSelectionThemeData(
          cursorColor: seed,
          selectionColor: Color(0x33315C55),
        ),
      ),
      home: V3LiveEditorCheckpointPage(
        openOnStart: openOnStart,
        webAssets: webAssets,
        surfaceController: surfaceController,
        onRuntimeOpened: onRuntimeOpened,
      ),
    );
  }
}

class V3LiveEditorCheckpointPage extends StatefulWidget {
  const V3LiveEditorCheckpointPage({
    super.key,
    this.openOnStart = true,
    this.webAssets,
    this.surfaceController,
    this.onRuntimeOpened,
  });

  final bool openOnStart;
  final FlarkV3WebRuntimeAssets? webAssets;
  final FlarkV3VirtualizedLiveSurfaceController? surfaceController;
  final ValueChanged<FlarkV3DocumentRuntime>? onRuntimeOpened;

  @override
  State<V3LiveEditorCheckpointPage> createState() =>
      _V3LiveEditorCheckpointPageState();
}

class _V3LiveEditorCheckpointPageState
    extends State<V3LiveEditorCheckpointPage> {
  late final FlarkV3VirtualizedLiveSurfaceController _surfaceController =
      widget.surfaceController ?? FlarkV3VirtualizedLiveSurfaceController();
  final FocusNode _focusNode = FocusNode(debugLabel: 'v3-live-checkpoint');

  FlarkV3DocumentRuntime? _runtime;
  FlarkV3ManagedFlutterBinding? _binding;
  FlarkV3ManagedViewportPresentationSource? _presentationSource;
  StreamSubscription<FlarkV3DocumentRuntimeStatus>? _statusSubscription;
  FlarkV3DocumentRuntimeStatus? _status;
  Object? _error;
  bool _opening = false;
  bool _hasBeenLive = false;
  int _lifecycleGeneration = 0;

  @override
  void initState() {
    super.initState();
    if (widget.openOnStart) {
      WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(_open()));
    }
  }

  Future<void> _open() async {
    if (_opening || _runtime != null) return;
    final generation = ++_lifecycleGeneration;
    setState(() {
      _opening = true;
      _error = null;
      _hasBeenLive = false;
    });

    FlarkV3DocumentRuntime? runtime;
    FlarkV3ManagedFlutterBinding? binding;
    FlarkV3ManagedViewportPresentationSource? presentationSource;
    try {
      runtime = await FlarkV3DocumentRuntime.open(
        v3LiveCheckpointMarkdown,
        webAssets: widget.webAssets ?? v3EngineLabWebAssets(),
      );
      if (!mounted || generation != _lifecycleGeneration) {
        await runtime.close();
        return;
      }

      final initialCaret = v3LiveCheckpointMarkdown.indexOf('start typing');
      binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 8192,
          value: TextEditingValue(
            text: v3LiveCheckpointMarkdown,
            selection: TextSelection.collapsed(offset: initialCaret),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 64 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      presentationSource = binding.attachCompleteDocumentViewportPresentation();
      presentationSource.addListener(_handlePresentationProgress);
      _statusSubscription = runtime.statuses.listen(_handleRuntimeProgress);
      _runtime = runtime;
      _binding = binding;
      _presentationSource = presentationSource;
      _status = runtime.status;
      widget.onRuntimeOpened?.call(runtime);
      if (mounted) {
        setState(() {
          _opening = false;
          _hasBeenLive =
              _hasBeenLive ||
              _isLive(
                status: runtime!.status,
                presentationSource: presentationSource,
              );
        });
      }

      await runtime.initialReady;
      if (mounted && generation == _lifecycleGeneration) {
        setState(() {
          _status = runtime!.status;
          _hasBeenLive =
              _hasBeenLive ||
              _isLive(
                status: runtime.status,
                presentationSource: presentationSource,
              );
        });
      }
    } catch (error) {
      presentationSource?.removeListener(_handlePresentationProgress);
      binding?.dispose();
      final subscription = _statusSubscription;
      _statusSubscription = null;
      if (subscription != null) await subscription.cancel();
      if (runtime != null) {
        await runtime.close().catchError((_) {});
      }
      if (!mounted || generation != _lifecycleGeneration) return;
      _runtime = null;
      _binding = null;
      _presentationSource = null;
      setState(() {
        _opening = false;
        _error = error;
      });
    }
  }

  void _handleRuntimeProgress(FlarkV3DocumentRuntimeStatus status) {
    if (!mounted) return;
    final wasLive = _hasBeenLive;
    final isLive = _isLive(
      status: status,
      presentationSource: _presentationSource,
    );
    final visibleRevisionChanged =
        wasLive && isLive && _status?.sourceRevision != status.sourceRevision;
    final stateChanged = _status?.state != status.state;
    final terminal =
        status.state == FlarkV3DocumentRuntimeState.faulted ||
        status.state == FlarkV3DocumentRuntimeState.closed;
    _status = status;
    if (wasLive && !visibleRevisionChanged && !stateChanged) {
      // The surface listens to parser authority directly. Once the shell has
      // reached its first exact frame, gap/progress statuses must not rebuild
      // the surrounding page on every tap or keystroke.
      return;
    }
    setState(() {
      // A terminal runtime must replace the ready shell with its diagnostic;
      // stable paint must never conceal a failed authority owner.
      _hasBeenLive = terminal ? false : wasLive || isLive;
    });
  }

  void _handlePresentationProgress() {
    if (!mounted) return;
    if (_hasBeenLive) {
      // FlarkV3VirtualizedLiveSurface owns this listenable after startup.
      // Rebuilding the checkpoint chrome here puts unrelated pixels in the
      // parser-recertification hot path.
      return;
    }
    final isLive = _isLive(
      status: _status,
      presentationSource: _presentationSource,
    );
    if (!isLive) return;
    setState(() {
      _hasBeenLive = true;
    });
  }

  void _handleLinkActivated(FlarkV3InlineLinkAnnotation annotation) {
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(
            'Parser-certified destination: ${annotation.destination}',
          ),
          duration: const Duration(seconds: 2),
        ),
      );
  }

  @override
  void dispose() {
    _lifecycleGeneration += 1;
    _presentationSource?.removeListener(_handlePresentationProgress);
    _binding?.dispose();
    final subscription = _statusSubscription;
    if (subscription != null) unawaited(subscription.cancel());
    final runtime = _runtime;
    if (runtime != null) unawaited(runtime.close().catchError((_) {}));
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final presentationSource = _presentationSource;
    final exact =
        presentationSource?.snapshot is FlarkV3ExactViewportSurfaceSnapshot;
    final status = _status;
    final live =
        exact &&
        status?.sourceCurrent == true &&
        status?.structureCurrent == true;
    final liveShell = _hasBeenLive || live;
    final diagnostic = _startupDiagnostic(
      status: status,
      binding: _binding,
      presentationSource: presentationSource,
    );

    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
          child: Align(
            alignment: Alignment.topCenter,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 920),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _CheckpointHeader(live: liveShell),
                  const SizedBox(height: 18),
                  Expanded(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: const Color(0xFFFFFCF6),
                        border: Border.all(color: const Color(0xFFD9D0C1)),
                        borderRadius: BorderRadius.circular(22),
                        boxShadow: const [
                          BoxShadow(
                            color: Color(0x140F2F29),
                            blurRadius: 28,
                            offset: Offset(0, 12),
                          ),
                        ],
                      ),
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(21),
                        child: _buildEditor(presentationSource),
                      ),
                    ),
                  ),
                  const SizedBox(height: 12),
                  _CheckpointFooter(
                    opening: _opening,
                    live: liveShell,
                    status: status,
                    error: _error,
                    diagnostic: diagnostic,
                    onRetry: _runtime == null && !_opening ? _open : null,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  bool _isLive({
    required FlarkV3DocumentRuntimeStatus? status,
    required FlarkV3ManagedViewportPresentationSource? presentationSource,
  }) =>
      presentationSource?.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
      status?.sourceCurrent == true &&
      status?.structureCurrent == true;

  Widget _buildEditor(
    FlarkV3ManagedViewportPresentationSource? presentationSource,
  ) {
    final binding = _binding;
    if (binding == null || presentationSource == null) {
      return _CheckpointLoading(error: _error);
    }
    return FlarkV3VirtualizedLiveSurface(
      key: const Key('v3-live-checkpoint-surface'),
      liveController: binding.controller,
      visibleBlockCoordinator: binding.visibleBlocks,
      presentationSource: presentationSource,
      controller: _surfaceController,
      focusNode: _focusNode,
      editableKey: const Key('v3-live-checkpoint-editable'),
      windowBlockCount: 64,
      horizontalPadding: 28,
      blockSpacing: 18,
      style: const TextStyle(
        fontFamily: 'Inter',
        fontSize: 18,
        height: 1.55,
        color: Color(0xFF1F2A27),
      ),
      codeStyle: const TextStyle(
        fontFamily: 'JetBrains Mono',
        fontSize: 15,
        height: 1.55,
        color: Color(0xFF25443D),
      ),
      paintLayerBuilder: (context, state) => const SizedBox.shrink(),
      sourceGapBuilder: (context, snapshot) => const _CheckpointLoading(),
      onLinkActivated: _handleLinkActivated,
    );
  }

  String _startupDiagnostic({
    required FlarkV3DocumentRuntimeStatus? status,
    required FlarkV3ManagedFlutterBinding? binding,
    required FlarkV3ManagedViewportPresentationSource? presentationSource,
  }) {
    final snapshot = presentationSource?.snapshot;
    final surfaceState = switch (snapshot) {
      FlarkV3SourceGapViewportSurfaceSnapshot(:final reason) => 'gap:$reason',
      FlarkV3ExactViewportSurfaceSnapshot(:final blocks) =>
        'exact:${blocks.length}',
      null => 'not-attached',
    };
    final controller = binding?.controller;
    return 'runtime=${status?.state.name ?? 'opening'} '
        'source=${status?.sourceCurrent ?? false}'
        '@${status?.sourceRevision ?? 0} '
        'structure=${status?.structureCurrent ?? false}'
        '@${status?.structureGeneration ?? 0} '
        'visible=${binding?.visibleBlocks.phase.name ?? 'idle'}'
        '/${binding?.visibleBlocks.boundedAdvanceCount ?? 0} '
        'semantic=${controller?.semanticActionsValid ?? false} '
        'inline=${controller?.hasCertifiedInlinePresentation ?? false} '
        'query=${controller?.paintState.documentQuery.runtimeType ?? 'none'} '
        'viewport=${status?.viewportPresentationGeneration ?? 0}'
        '/${status?.viewportPresentationAttemptOutcomeGeneration ?? 0} '
        'viewportUnavailable='
        '${status?.viewportPresentationUnavailableReason?.name ?? 'none'} '
        'surface=$surfaceState';
  }
}

class _CheckpointHeader extends StatelessWidget {
  const _CheckpointHeader({required this.live});

  final bool live;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      alignment: WrapAlignment.spaceBetween,
      crossAxisAlignment: WrapCrossAlignment.center,
      runSpacing: 12,
      children: [
        const Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Flark live editor',
              style: TextStyle(
                fontSize: 30,
                height: 1.1,
                fontWeight: FontWeight.w700,
                color: Color(0xFF17211E),
              ),
            ),
            SizedBox(height: 6),
            Text(
              'Tap a block and type. Markdown stays canonical; syntax markers '
              'stay out of the editing surface.',
              style: TextStyle(
                fontSize: 14,
                height: 1.4,
                color: Color(0xFF66716C),
              ),
            ),
          ],
        ),
        DecoratedBox(
          decoration: BoxDecoration(
            color: live ? const Color(0xFFE1F0E9) : const Color(0xFFF1ECE2),
            borderRadius: BorderRadius.circular(999),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 8,
                  height: 8,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: live
                        ? const Color(0xFF21845F)
                        : const Color(0xFF9B8C72),
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  live ? 'Live editor ready' : 'Starting runtime',
                  key: const Key('v3-live-checkpoint-status'),
                  style: const TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                    color: Color(0xFF3D4A45),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _CheckpointFooter extends StatelessWidget {
  const _CheckpointFooter({
    required this.opening,
    required this.live,
    required this.status,
    required this.error,
    required this.diagnostic,
    required this.onRetry,
  });

  final bool opening;
  final bool live;
  final FlarkV3DocumentRuntimeStatus? status;
  final Object? error;
  final String diagnostic;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    if (error != null) {
      return Row(
        children: [
          Expanded(
            child: Text(
              'The production parser runtime did not open: $error',
              key: const Key('v3-live-checkpoint-error'),
              style: const TextStyle(color: Color(0xFF9B2C2C)),
            ),
          ),
          TextButton(onPressed: onRetry, child: const Text('Retry')),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(
              live ? Icons.check_circle_outline : Icons.sync,
              size: 16,
              color: live ? const Color(0xFF21845F) : const Color(0xFF85765E),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                live
                    ? 'Live parser rendering · revision ${status!.sourceRevision}'
                    : opening
                    ? 'Opening the production Worker + Wasm runtime…'
                    : 'Waiting for exact parser authority…',
                style: const TextStyle(fontSize: 12, color: Color(0xFF66716C)),
              ),
            ),
            const Text(
              'One stable input client',
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: Color(0xFF66716C),
              ),
            ),
          ],
        ),
        if (!live) ...[
          const SizedBox(height: 6),
          Text(
            diagnostic,
            key: const Key('v3-live-checkpoint-diagnostic'),
            style: const TextStyle(
              fontFamily: 'JetBrains Mono',
              fontSize: 10,
              height: 1.35,
              color: Color(0xFF776B58),
            ),
          ),
        ],
      ],
    );
  }
}

class _CheckpointLoading extends StatelessWidget {
  const _CheckpointLoading({this.error});

  final Object? error;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (error == null) ...[
            const SizedBox.square(
              dimension: 24,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            const SizedBox(height: 12),
          ],
          Text(
            error == null
                ? 'Certifying the document…'
                : 'Parser runtime unavailable',
            style: const TextStyle(fontSize: 13, color: Color(0xFF66716C)),
          ),
        ],
      ),
    );
  }
}
