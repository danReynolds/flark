part of 'editor_view.dart';

extension _EditorToolbar on _EditorState {
  Widget _buildToolbar() {
    final editor = _editor;
    final revision = editor.revision, selection = editor.selection;
    // The toolbar's one model of what it can do: the session's publication
    // when there is one, otherwise the same state built from the editor, so
    // its availability never differs from what a consumer is told.
    final state =
        widget.session?.state ??
        FlarkState(
          markdown: editor.source,
          revision: revision,
          status: FlarkStatus.ready,
          error: null,
          editor: editor,
        );
    final row = editor.sourceMode
        ? null
        : editor.document.rowAt(selection.extent);
    bool active() =>
        mounted &&
        identical(editor, _editor) &&
        !widget.readOnly &&
        editor.revision == revision &&
        editor.selection == selection;
    // A command ends a composition before it applies, as a consumer's does:
    // applied during one, a style chosen for the next text went when the
    // composition committed.
    void command(FlarkCommand command) {
      if (!active()) return;
      editor.applyAfterComposition(command);
      _focus.requestFocus();
    }

    Widget toggle(String label, String text, FlarkStyle style) {
      final styled = state.styles[style];
      final selected = styled.isOn;
      final enabled = styled.canToggle;
      void activate() =>
          command(SetStyle(style.kernelStyle, enabled: !selected));
      return Semantics(
        role: SemanticRole.button,
        label: label,
        value: styled.isMixed ? 'Mixed' : null,
        hint: styled.isMixed ? 'Mixed formatting' : (selected ? 'On' : 'Off'),
        selected: selected,
        enabled: enabled,
        includeChildren: false,
        actions: enabled ? {SemanticAction.activate} : {},
        onAction: (action) {
          if (enabled && action == SemanticAction.activate) activate();
        },
        child: Button(
          text: styled.isMixed ? '$text−' : text,
          style: CellStyle(inverse: selected),
          onPressed: enabled ? activate : null,
        ),
      );
    }

    // The caret's fence's info string, or null outside fenced code.
    final info = state.code.language;
    final languages = info == null
        ? null
        : CodeMirrorLanguageMenu(
            editor.codeEditing,
            info: info,
            code: editor.document.caretRow.text,
          );
    return Padding(
      padding: const EdgeInsets.only(bottom: 1),
      child: Wrap(
        spacing: 1,
        children: [
          Button(
            text: 'Undo',
            onPressed: state.canUndo ? () => command(const Undo()) : null,
          ),
          Button(
            text: 'Redo',
            onPressed: state.canRedo ? () => command(const Redo()) : null,
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
            onChanged: state.heading.canSet
                ? (level) => command(SetHeadingLevel(level))
                : null,
          ),
          toggle('Bold', 'B', FlarkStyle.bold),
          toggle('Italic', 'I', FlarkStyle.italic),
          toggle('Strikethrough', 'S', FlarkStyle.strikethrough),
          toggle('Inline code', '`code`', FlarkStyle.inlineCode),
          Button(
            text: 'Link',
            onPressed: state.link.canSet ? () => _editLink() : null,
          ),
          Button(
            text: 'Image',
            onPressed: editor.canSetResource(image: true)
                ? () => _editLink(image: true)
                : null,
          ),
          if (languages != null)
            Select<String>(
              // Replace an open picker if its target changes. A dropdown must
              // never apply an old choice to a different fence or document.
              key: ValueKey(('language', editor, revision, selection)),
              semanticLabel: 'Code language',
              value: languages.value,
              options: [
                SelectOption(
                  value: '',
                  label:
                      'Automatic${languages.detected == null ? '' : ' · ${languages.detected}'}',
                ),
                const SelectOption(value: 'text', label: 'Plain text'),
                // A language the menu does not offer keeps its written name.
                if (languages.value.isNotEmpty &&
                    languages.value != 'text' &&
                    !languages.labels.containsKey(languages.value))
                  SelectOption(value: languages.value, label: languages.value),
                for (final MapEntry(key: name, value: label)
                    in languages.labels.entries)
                  SelectOption(value: name, label: label),
              ],
              onChanged: state.code.canSetLanguage
                  ? (value) {
                      if (value != languages.value) {
                        command(SetCodeLanguage(value));
                      }
                      if (mounted) _focus.requestFocus();
                    }
                  : null,
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
