import 'resource_controls.dart';
import 'package:flutter/material.dart';

/// Resolve document-relative resources without interpreting Markdown.
Uri? flarkResourceUri(String value, Uri? base) {
  final uri = Uri.tryParse(value);
  return uri == null ? null : base?.resolveUri(uri) ?? uri;
}

Uri? flarkOpenableUri(String value, Uri? base) {
  final uri = flarkResourceUri(value, base);
  return uri != null && const {'https', 'http', 'mailto'}.contains(uri.scheme)
      ? uri
      : null;
}

class ResourceDialog extends StatefulWidget {
  const ResourceDialog({super.key, required this.session});
  final FlarkResourceSession session;
  @override
  State<ResourceDialog> createState() => _ResourceDialogState();
}

class _ResourceDialogState extends State<ResourceDialog> {
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
        () => error = !remove && destination.text.trim().isEmpty
            ? 'Enter a destination.'
            : 'This edit could not be applied. Cancel and reselect the text.',
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
  Widget build(BuildContext context) => AlertDialog(
    title: Text(
      '${widget.session.resource == null ? 'Insert' : 'Edit'} ${widget.session.image ? 'image' : 'link'}',
    ),
    content: SizedBox(
      width: 420,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: label,
              decoration: InputDecoration(
                labelText: widget.session.image ? 'Alt text' : 'Text',
              ),
            ),
            TextField(
              controller: destination,
              autofocus: true,
              decoration: InputDecoration(
                labelText: widget.session.image ? 'Image URL' : 'Destination',
              ),
              onSubmitted: (_) => submit(),
            ),
            TextField(
              controller: title,
              decoration: const InputDecoration(labelText: 'Title (optional)'),
              onSubmitted: (_) => submit(),
            ),
            if (error != null)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Text(
                  error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
          ],
        ),
      ),
    ),
    actions: [
      if (widget.session.onOpen != null)
        TextButton(
          onPressed: widget.session.open,
          child: const Text('Open link'),
        ),
      if (widget.session.resource != null)
        TextButton(
          onPressed: () => submit(remove: true),
          child: Text(widget.session.image ? 'Remove image' : 'Remove link'),
        ),
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: submit, child: const Text('Save')),
    ],
  );
}
