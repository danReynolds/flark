import 'package:flutter/material.dart';
import 'controller.dart';
import 'code_font.dart';
import 'source_window.dart';

/// Read-only exact source around the editor's caret, using the same bounded
/// pages as source editing. Full-document export remains controller.text.
class FlarkSourceView extends StatelessWidget {
  const FlarkSourceView({super.key, required this.controller});
  final FlarkController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final source = controller.text;
      final page = SourceWindow.at(source, controller.editor.selection.extent);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Source page ${page.index + 1} of ${page.count} · follows caret',
            style: Theme.of(context).textTheme.labelSmall,
          ),
          const SizedBox(height: 12),
          Expanded(
            child: SingleChildScrollView(
              key: ValueKey(page.start),
              child: SelectableText(
                source.substring(page.start, page.end),
                style: const TextStyle(
                  fontFamily: flarkCodeFontFamily,
                  fontSize: 13,
                  height: 1.5,
                ),
              ),
            ),
          ),
        ],
      );
    },
  );
}
