import 'package:flark/resources.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart' show Button, Dialog;

export 'package:flark/resources.dart'
    show FlarkResourceSession, FlarkLinkActions;

typedef FlarkFleuryResourcePresenter =
    Future<void> Function(BuildContext context, FlarkResourceSession session);
typedef FlarkFleuryLinkPopoverBuilder =
    Widget Function(BuildContext context, FlarkLinkActions actions);

/// Default link controls, replaceable without replacing their guarded actions.
class FlarkLinkPopover extends StatelessWidget {
  const FlarkLinkPopover({super.key, required this.actions});
  final FlarkLinkActions actions;

  @override
  Widget build(BuildContext context) => Semantics(
    role: SemanticRole.region,
    label: 'Link actions',
    child: Container(
      color: Theme.of(context).colorScheme.background,
      border: BoxBorder(style: Theme.of(context).borderStyle),
      padding: const EdgeInsets.symmetric(horizontal: 1),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
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
              Button(label: 'Open', onPressed: actions.open),
              if (actions.edit != null)
                Button(label: 'Edit', onPressed: actions.edit),
              if (actions.remove != null)
                Button(label: 'Remove', onPressed: actions.remove),
              Button(label: 'Close', onPressed: actions.dismiss),
            ],
          ),
        ],
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
        () => error = widget.session.failureFor(remove ? 'x' : destination.text),
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
  Widget build(BuildContext context) => Dialog(
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
              label: 'Cancel',
              onPressed: () => Navigator.of(context).pop(),
            ),
            if (widget.session.resource != null)
              Button(label: 'Remove', onPressed: () => submit(remove: true)),
            Button(label: 'Save', onPressed: submit),
          ],
        ),
      ],
    ),
  );
}
