part of 'editor_view.dart';

extension _EditorToolbar on _EditorState {
  Widget _buildToolbar() {
    final editor = _editor;
    final revision = editor.revision, selection = editor.selection;
    final row = editor.sourceMode
        ? null
        : editor.document.rowAt(selection.extent);
    final inline = row != null && row.kind != RowKind.codeBlock;
    final heading =
        row != null &&
        (row.kind == RowKind.paragraph ||
            row.kind == RowKind.heading ||
            (row.kind == RowKind.blank && selection.isCollapsed));
    bool active() =>
        mounted &&
        identical(editor, _editor) &&
        !widget.readOnly &&
        editor.revision == revision &&
        editor.selection == selection;
    void command(FlarkCommand command) {
      if (!active()) return;
      _apply(command);
      _focus.requestFocus();
    }

    Widget toggle(String label, String text, int style) {
      final selected = editor.typingContext & style != 0;
      void activate() => command(ToggleStyle(style));
      return Semantics(
        role: SemanticRole.button,
        label: label,
        selected: selected,
        enabled: inline,
        includeChildren: false,
        actions: inline ? {SemanticAction.activate} : {},
        onAction: (action) {
          if (inline && action == SemanticAction.activate) activate();
        },
        child: Button(
          text: text,
          style: CellStyle(inverse: selected),
          onPressed: inline ? activate : null,
        ),
      );
    }

    final info = row?.fenced == true
        ? editor.source.substring(row!.codeInfoStart, row.codeInfoEnd)
        : '';
    final language = codeLanguageName(info);
    final detected = row?.fenced == true && language.isEmpty
        ? editor.codeEditing?.resolveLanguage(row!.text, info)
        : null;
    return Padding(
      padding: const EdgeInsets.only(bottom: 1),
      child: Wrap(
        spacing: 1,
        children: [
          Button(
            text: 'Undo',
            onPressed: editor.history.canUndo
                ? () => command(const Undo())
                : null,
          ),
          Button(
            text: 'Redo',
            onPressed: editor.history.canRedo
                ? () => command(const Redo())
                : null,
          ),
          Select<int>(
            key: ValueKey(('block', editor, revision, selection)),
            semanticLabel: 'Paragraph style',
            value: row?.headingLevel ?? 0,
            options: [
              const SelectOption(value: 0, label: 'Paragraph'),
              for (var level = 1; level <= 6; level++)
                SelectOption(value: level, label: 'Heading $level'),
            ],
            onChanged: heading
                ? (level) => command(SetHeadingLevel(level))
                : null,
          ),
          toggle('Bold', 'B', Style.strong),
          toggle('Italic', 'I', Style.emphasis),
          toggle('Strikethrough', 'S', Style.strikethrough),
          toggle('Inline code', '`code`', Style.code),
          Button(text: 'Link', onPressed: inline ? () => _editLink() : null),
          Button(
            text: 'Image',
            onPressed: inline ? () => _editLink(image: true) : null,
          ),
          if (row?.fenced == true)
            Select<String>(
              // Replace an open picker if its target changes. A dropdown must
              // never apply an old choice to a different fence or document.
              key: ValueKey(('language', editor, revision, selection)),
              semanticLabel: 'Code language',
              value: language,
              options: [
                SelectOption(
                  value: '',
                  label:
                      'Automatic${codeLanguages[detected] == null ? '' : ' · ${codeLanguages[detected]}'}',
                ),
                const SelectOption(value: 'text', label: 'Plain text'),
                if (language.isNotEmpty && !codeLanguages.containsKey(language))
                  SelectOption(value: language, label: language),
                for (final entry in codeLanguages.entries)
                  if (entry.key != 'text')
                    SelectOption(value: entry.key, label: entry.value),
              ],
              onChanged: (value) {
                if (value != language) command(SetCodeLanguage(value));
                if (mounted) _focus.requestFocus();
              },
            ),
          Button(
            text: editor.sourceMode ? 'Rendered' : 'Source',
            onPressed: () {
              _finishInput();
              editor.setSourceMode(!editor.sourceMode);
              _focus.requestFocus();
            },
          ),
        ],
      ),
    );
  }
}
