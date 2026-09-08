import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';

/// Example client UI. No source offsets, Markdown serialization or undo code.
Widget brandedLinkPopover(BuildContext context, FlarkLinkActions actions) {
  final colors = Theme.of(context).colorScheme;
  return Material(
    color: colors.tertiaryContainer,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(18),
      side: BorderSide(color: colors.tertiary),
    ),
    child: Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'LINK DETAILS',
            style: TextStyle(
              color: colors.onTertiaryContainer,
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 1.5,
            ),
          ),
          const SizedBox(height: 8),
          Text(
            actions.resource.destination,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 6,
            children: [
              FilledButton.tonal(
                onPressed: actions.open,
                child: const Text('Visit'),
              ),
              OutlinedButton(
                onPressed: actions.edit,
                child: const Text('Change'),
              ),
              IconButton(
                tooltip: 'Unlink',
                onPressed: actions.remove,
                icon: const Icon(Icons.link_off),
              ),
              IconButton(
                tooltip: 'Close link details',
                onPressed: actions.dismiss,
                icon: const Icon(Icons.close),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}

Future<void> showResourceSheet(
  BuildContext context,
  FlarkResourceSession session,
) => showModalBottomSheet<void>(
  context: context,
  isScrollControlled: true,
  useSafeArea: true,
  showDragHandle: true,
  builder: (_) => _ResourceSheet(session: session),
);

class _ResourceSheet extends StatefulWidget {
  const _ResourceSheet({required this.session});
  final FlarkResourceSession session;
  @override
  State<_ResourceSheet> createState() => _ResourceSheetState();
}

class _ResourceSheetState extends State<_ResourceSheet> {
  late final label = TextEditingController(text: widget.session.label);
  late final url = TextEditingController(text: widget.session.destination);
  late final title = TextEditingController(text: widget.session.title);
  String? error;
  void save({bool remove = false}) {
    final applied = remove
        ? widget.session.remove()
        : widget.session.save(
            destination: url.text,
            label: label.text,
            title: title.text,
          );
    if (applied) {
      Navigator.pop(context);
    } else {
      setState(
        () => error = url.text.trim().isEmpty
            ? 'Enter a URL.'
            : 'This target changed or the edit cannot be applied. Cancel and try again.',
      );
    }
  }

  @override
  void dispose() {
    label.dispose();
    url.dispose();
    title.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.fromLTRB(
      24,
      0,
      24,
      24 + MediaQuery.viewInsetsOf(context).bottom,
    ),
    child: SingleChildScrollView(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            widget.session.image ? 'Image details' : 'Link details',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 12),
          TextField(
            controller: label,
            decoration: InputDecoration(
              labelText: widget.session.image ? 'Alt text' : 'Label',
            ),
          ),
          TextField(
            controller: url,
            autofocus: true,
            decoration: const InputDecoration(labelText: 'URL'),
          ),
          TextField(
            controller: title,
            decoration: const InputDecoration(
              labelText: 'Description (optional)',
            ),
          ),
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          const SizedBox(height: 20),
          Wrap(
            spacing: 12,
            children: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('Cancel'),
              ),
              if (widget.session.resource != null)
                TextButton(
                  onPressed: () => save(remove: true),
                  child: const Text('Remove'),
                ),
              FilledButton(onPressed: save, child: const Text('Apply changes')),
            ],
          ),
        ],
      ),
    ),
  );
}
