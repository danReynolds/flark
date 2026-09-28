import 'dart:async';
import 'package:flark/session.dart';
import 'package:fleury/fleury_core.dart';
import 'controller.dart' as input;
import 'editor_view.dart' as host;
import 'theme.dart';
import 'image_previews.dart';
import 'resource_controls.dart';

/// One owned document, with the same callable API as the Flutter controller.
class FlarkController extends ChangeNotifier with FlarkActions {
  /// [syncLimit] and [liveLimits] bound what renders live, by default
  /// [flarkDefaultLiveBytes] and [FlarkLiveLimits]; beyond them the document
  /// edits in source mode.
  FlarkController({
    String markdown = '',
    int? syncLimit,
    FlarkLiveLimits liveLimits = const FlarkLiveLimits(),
  }) : session = FlarkSession(
         markdown: markdown,
         syncLimit: syncLimit,
         liveLimits: liveLimits,
       ) {
    session.addListener(_changed);
  }
  @override
  final FlarkSession session;
  input.FlarkFleuryController? _input;
  bool _disposed = false;
  input.FlarkFleuryController get _bridge =>
      _input ??= input.FlarkFleuryController(session.engine!);
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

/// Complete cell-grid composer. Supply a controller or an initial document.
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
    this.imagePreviewBuilder,
    this.presentResourceEditor,
    this.linkPopoverBuilder,
  }) : assert(controller == null || initialMarkdown == null);
  final FlarkController? controller;
  final String? initialMarkdown;
  final void Function(String)? onChanged;
  final bool autofocus, readOnly, showToolbar;
  final FocusNode? focusNode;
  final FlarkCellTheme? theme;
  final void Function(Uri)? onOpenLink;
  final Uri? baseUri;
  final FlarkFleuryImagePreviewBuilder? imagePreviewBuilder;
  final FlarkFleuryResourcePresenter? presentResourceEditor;
  final FlarkFleuryLinkPopoverBuilder? linkPopoverBuilder;
  @override
  State<FlarkEditor> createState() => _FlarkEditorState();
}

class _FlarkEditorState extends State<FlarkEditor> {
  late FlarkController _controller;
  late bool _owned;
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
      return const Text('Loading editor…');
    }
    if (state.status != FlarkStatus.ready) {
      return Column(
        children: [
          Text(
            state.status == FlarkStatus.disposed
                ? 'Editor closed'
                : 'Unable to load editor',
          ),
          if (state.status == FlarkStatus.failed)
            Button(
              text: 'Retry',
              onPressed: () {
                unawaited(_controller.retryLoading().catchError((Object _) {}));
              },
            ),
        ],
      );
    }
    return host.FlarkEditorView(
      key: ValueKey(_controller),
      controller: _controller._bridge,
      actions: _controller,
      session: _controller.session,
      autofocus: widget.autofocus || (widget.focusNode?.hasFocus ?? false),
      readOnly: widget.readOnly,
      showToolbar: widget.showToolbar,
      focusNode: widget.focusNode,
      theme: widget.theme,
      onOpenLink: widget.onOpenLink,
      baseUri: widget.baseUri,
      imagePreviewBuilder: widget.imagePreviewBuilder,
      presentResourceEditor: widget.presentResourceEditor,
      linkPopoverBuilder: widget.linkPopoverBuilder,
    );
  }
}
