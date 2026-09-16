import 'package:flark/resources.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart' show Button, Dialog;

export 'package:flark/resources.dart'
    show FlarkResourceSession, FlarkLinkActions;

typedef FlarkFleuryResourcePresenter =
    Future<void> Function(BuildContext context, FlarkResourceSession session);
typedef FlarkFleuryLinkPopoverBuilder =
    Widget Function(BuildContext context, FlarkLinkActions actions);

/// TextInput's base paint comes from interactiveStyle, while Text uses
/// DefaultTextStyle. Supply the surface foreground to both, beneath explicit
/// application styles, so an unfocused field remains readable on a light fill.
class _ResourceTheme extends StatelessWidget {
  const _ResourceTheme({required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final base = CellStyle(foreground: theme.colorScheme.foreground);
    return Theme(
      data: theme.copyWith(
        textStyle: base.merge(DefaultTextStyle.of(context)),
        interactiveStyle: base.merge(theme.interactiveStyle ?? CellStyle.none),
      ),
      child: child,
    );
  }
}

/// Default resource controls, replaceable without replacing guarded actions.
class FlarkLinkPopover extends StatelessWidget {
  const FlarkLinkPopover({super.key, required this.actions});
  final FlarkLinkActions actions;

  @override
  Widget build(BuildContext context) => Semantics(
    role: SemanticRole.region,
    label: actions.resource.isImage ? 'Image actions' : 'Link actions',
    child: _ResourceTheme(
      child: Container(
        color: Theme.of(context).colorScheme.background,
        border: BoxBorder(
          style: Theme.of(context).borderStyle,
          cellStyle: CellStyle(
            foreground: Theme.of(context).colorScheme.foreground,
            background: Theme.of(context).colorScheme.background,
          ),
        ),
        padding: const EdgeInsets.symmetric(horizontal: 1),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              actions.resource.destination,
              allowSelect: false,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            Wrap(
              spacing: 1,
              children: [
                Button(text: 'Open', onPressed: actions.open),
                if (actions.edit != null)
                  Button(text: 'Edit', onPressed: actions.edit),
                if (actions.remove != null)
                  Button(text: 'Remove', onPressed: actions.remove),
                Button(text: 'Close', onPressed: actions.dismiss),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

/// Default resource form uses Fleury's normal focus, text input and modal route.
class FlarkResourceDialog extends StatefulWidget {
  const FlarkResourceDialog({super.key, required this.session});
  final FlarkResourceSession session;
  @override
  State<FlarkResourceDialog> createState() => _ResourceDialogState();
}

class _ResourceDialogState extends State<FlarkResourceDialog> {
  late final label = TextEditingController(text: widget.session.label);
  late final destination = TextEditingController(
    text: widget.session.destination,
  );
  late final title = TextEditingController(text: widget.session.title);
  String? error;

  void submit({bool remove = false}) {
    final accepted = remove
        ? widget.session.remove()
        : widget.session.save(
            destination: destination.text,
            label: label.text,
            title: title.text,
          );
    if (accepted) {
      Navigator.of(context).pop();
    } else {
      setState(
        () =>
            error = widget.session.failureFor(remove ? 'x' : destination.text),
      );
    }
  }

  @override
  void dispose() {
    label.dispose();
    destination.dispose();
    title.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => _ResourceTheme(
    child: Container(
      color: Theme.of(context).colorScheme.background,
      child: Dialog(
        border: BoxBorder(
          style: Theme.of(context).borderStyle,
          cellStyle: CellStyle(
            foreground: Theme.of(context).colorScheme.foreground,
          ),
        ),
        title: widget.session.formTitle,
        width: (MediaQuery.of(context).size.cols - 4).clamp(16, 56),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(widget.session.labelField, allowSelect: false),
            TextInput(
              controller: label,
              semanticLabel: widget.session.labelField,
              onSubmit: (_) => submit(),
            ),
            Text(widget.session.destinationField, allowSelect: false),
            TextInput(
              controller: destination,
              semanticLabel: widget.session.destinationField,
              autofocus: true,
              onSubmit: (_) => submit(),
            ),
            Text(widget.session.titleField, allowSelect: false),
            TextInput(
              controller: title,
              semanticLabel: widget.session.titleField,
              onSubmit: (_) => submit(),
            ),
            if (error != null) Text(error!, maxLines: 2),
            Wrap(
              spacing: 1,
              children: [
                Button(
                  text: 'Cancel',
                  onPressed: () => Navigator.of(context).pop(),
                ),
                if (widget.session.resource != null)
                  Button(
                    text: 'Remove',
                    onPressed: () => submit(remove: true),
                  ),
                Button(text: 'Save', onPressed: submit),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}
