import 'dart:async';
import 'package:flark/session.dart';
import 'package:flutter/material.dart';
import 'controller.dart' as input;
import 'editor.dart' as host;
import 'theme.dart';
import 'image_previews.dart';
import 'resource_controls.dart';

/// Owns one editing session. Construction starts preparation automatically.
/// Dispose externally supplied controllers after their last view is removed.
class FlarkController extends ChangeNotifier with FlarkActions {
  FlarkController({String markdown = ''})
    : session = FlarkSession(markdown: markdown) {
    session.addListener(_changed);
  }
  @override
  final FlarkSession session;
  input.FlarkController? _input;
  bool _disposed = false;
  input.FlarkController get _bridge {
    final bridge = _input ??= input.FlarkController(session.engine!);
    session.commandHandler = (command, revision) =>
        bridge.command(command, expectedRevision: revision);
    return bridge;
  }

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    session.removeListener(_changed);
    _input?.dispose();
    _input = null;
    session.dispose();
    super.dispose();
  }
}

/// A complete composer: toolbar, scrollable document and platform input.
/// Supply either [controller] or [initialMarkdown]. A widget-owned session is
/// seeded once; use a new key for a different document.
class FlarkEditor extends StatefulWidget {
  const FlarkEditor({
    super.key,
    this.controller,
    this.initialMarkdown,
    this.onChanged,
    this.autofocus = false,
    this.readOnly = false,
    this.showToolbar = true,
    this.focusNode,
    this.theme,
    this.onOpenLink,
    this.baseUri,
    this.imageProvider,
    this.presentResourceEditor,
    this.linkPopoverBuilder,
  }) : assert(controller == null || initialMarkdown == null);
  final FlarkController? controller;
  final String? initialMarkdown;
  final ValueChanged<String>? onChanged;
  final bool autofocus, readOnly, showToolbar;
  final FocusNode? focusNode;
  final FlarkThemeData? theme;
  final ValueChanged<Uri>? onOpenLink;
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  final FlarkResourcePresenter? presentResourceEditor;
  final FlarkLinkPopoverBuilder? linkPopoverBuilder;
  @override
  State<FlarkEditor> createState() => _FlarkEditorState();
}

class _FlarkEditorState extends State<FlarkEditor> {
  late FlarkController _controller;
  late bool _owned;
  bool _restoreFocus = false;
  StreamSubscription<String>? _subscription;
  void _attach() {
    _owned = widget.controller == null;
    _controller =
        widget.controller ??
        FlarkController(markdown: widget.initialMarkdown ?? '');
    _controller.session.attach(this);
    _controller.addListener(_changed);
    _subscription = _controller.changes.listen(
      (text) => widget.onChanged?.call(text),
    );
  }

  void _detach() {
    unawaited(_subscription?.cancel());
    _controller.removeListener(_changed);
    _controller.session.detach(this);
    if (_owned) _controller.dispose();
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _attach();
  }

  @override
  void didUpdateWidget(FlarkEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.controller != oldWidget.controller) {
      _restoreFocus = oldWidget.focusNode?.hasFocus ?? false;
      _detach();
      _attach();
    }
  }

  @override
  void dispose() {
    _detach();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = _controller.state;
    if (state.status == FlarkStatus.loading) {
      return const Center(
        child: CircularProgressIndicator(semanticsLabel: 'Loading editor'),
      );
    }
    if (state.status != FlarkStatus.ready) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              state.status == FlarkStatus.disposed
                  ? 'Editor closed'
                  : 'Unable to load editor',
            ),
            if (state.status == FlarkStatus.failed)
              TextButton(
                onPressed: () {
                  unawaited(
                    _controller.retryLoading().catchError((Object _) {}),
                  );
                },
                child: const Text('Retry'),
              ),
          ],
        ),
      );
    }
    return host.FlarkEditorWidget(
      key: ObjectKey(_controller),
      controller: _controller._bridge,
      actions: _controller,
      session: _controller.session,
      autofocus:
          widget.autofocus ||
          _restoreFocus ||
          (widget.focusNode?.hasFocus ?? false),
      readOnly: widget.readOnly,
      showToolbar: widget.showToolbar,
      focusNode: widget.focusNode,
      theme: widget.theme,
      onOpenLink: widget.onOpenLink,
      baseUri: widget.baseUri,
      imageProvider: widget.imageProvider,
      presentResourceEditor: widget.presentResourceEditor,
      linkPopoverBuilder: widget.linkPopoverBuilder,
    );
  }
}
