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
    void command(FlarkCommand command) {
      if (!active()) return;
      final actions = widget.actions;
      if (actions == null) {
        _apply(command);
      } else {
        switch (command) {
          case SetStyle(:final style, :final enabled):
            actions.setStyle(
              FlarkStyle.values.firstWhere((s) => s.kernelStyle == style),
              enabled: enabled,
            );
          case SetHeadingLevel(:final level):
            actions.setHeading(level);
          case SetCodeLanguage(:final language):
            actions.setCodeLanguage(language);
          case Undo():
            actions.undo();
          case Redo():
            actions.redo();
          default:
            _apply(command);
        }
      }
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
    final language = codeMirrorLanguageName(info ?? '');
    final code = editor.codeEditing;
    final detected = info != null && language.isEmpty
        ? code?.resolveLanguage(editor.document.caretRow.text, info)
        : null;
    final labels = {
      for (final choice
          in code is FlarkCodeMirror ? code.languages : CodeMirrorLanguages.all)
        choice.name: choice.label,
    };
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
            onPressed: state.link.canSet
                ? () => widget.actions?.showLinkEditor() ?? _editLink()
                : null,
          ),
          Button(
            text: 'Image',
            onPressed: editor.canSetResource(image: true)
                ? () =>
                      widget.actions?.showImageEditor() ??
                      _editLink(image: true)
                : null,
          ),
          if (info != null)
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
                      'Automatic${labels[detected] == null ? '' : ' · ${labels[detected]}'}',
                ),
                const SelectOption(value: 'text', label: 'Plain text'),
                if (language.isNotEmpty &&
                    language != 'text' &&
                    !labels.containsKey(language))
                  SelectOption(value: language, label: language),
                for (final MapEntry(key: name, value: label) in labels.entries)
                  SelectOption(value: name, label: label),
              ],
              onChanged: state.code.canSetLanguage
                  ? (value) {
                      if (value != language) command(SetCodeLanguage(value));
                      if (mounted) _focus.requestFocus();
                    }
                  : null,
            ),
          Button(
            text: editor.sourceMode ? 'Rendered' : 'Source',
            onPressed: () {
              _finishInput();
              widget.actions == null
                  ? editor.setSourceMode(!editor.sourceMode)
                  : widget.actions!.setSourceMode(!editor.sourceMode);
              _focus.requestFocus();
            },
          ),
        ],
      ),
    );
  }
}
