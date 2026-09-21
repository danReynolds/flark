part of 'editor_view.dart';

extension _EditorToolbar on _EditorState {
  Widget _buildToolbar() {
    final editor = _editor;
    final revision = editor.revision, selection = editor.selection;
    final row = editor.sourceMode
        ? null
        : editor.document.rowAt(selection.extent);
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

    Widget toggle(String label, String text, int style) {
      final state =
          widget.actions?.state.styles[FlarkStyle.values.firstWhere(
            (s) => s.kernelStyle == style,
          )] ??
          widget.controller.styleState(style);
      final selected = state.isOn;
      final enabled = state.canToggle;
      void activate() => command(SetStyle(style, enabled: !selected));
      return Semantics(
        role: SemanticRole.button,
        label: label,
        value: state.isMixed ? 'Mixed' : null,
        hint: state.isMixed ? 'Mixed formatting' : (selected ? 'On' : 'Off'),
        selected: selected,
        enabled: enabled,
        includeChildren: false,
        actions: enabled ? {SemanticAction.activate} : {},
        onAction: (action) {
          if (enabled && action == SemanticAction.activate) activate();
        },
        child: Button(
          text: state.isMixed ? '$text−' : text,
          style: CellStyle(inverse: selected),
          onPressed: enabled ? activate : null,
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
            onPressed: (widget.actions?.state.canUndo ?? editor.history.canUndo)
                ? () => command(const Undo())
                : null,
          ),
          Button(
            text: 'Redo',
            onPressed: (widget.actions?.state.canRedo ?? editor.history.canRedo)
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
            onChanged: (widget.actions?.state.heading.canSet ?? heading)
                ? (level) => command(SetHeadingLevel(level))
                : null,
          ),
          toggle('Bold', 'B', Style.strong),
          toggle('Italic', 'I', Style.emphasis),
          toggle('Strikethrough', 'S', Style.strikethrough),
          toggle('Inline code', '`code`', Style.code),
          Button(
            text: 'Link',
            onPressed:
                (widget.actions?.state.link.canSet ?? editor.canSetResource())
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
