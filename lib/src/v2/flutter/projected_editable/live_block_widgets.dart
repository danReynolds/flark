// Block-type widgets for live-rendered editing: list items, task
// checkboxes, code fences (copy button, language badge/selector), tables
// and their cell editors, plus the marker painters.

part of '../flark_projected_editable_text.dart';

final class _EditableListItemBlock extends StatelessWidget {
  const _EditableListItemBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.blockHandle,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;
  String get currentDisplayText => blockHandle?.displayText ?? displayText;

  @override
  Widget build(BuildContext context) {
    final block = currentBlock;
    final displayText = currentDisplayText;
    final marker = _listMarkerInfo(controller.markdown, block);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _ListMarkerGlyph(marker: marker, style: style),
          const SizedBox(width: 8),
          Expanded(
            child: _EditableProjectedBlockText(
              controller: controller,
              block: block,
              blockHandle: blockHandle,
              displayText: displayText,
              style: style,
              cursorColor: cursorColor,
              backgroundCursorColor: backgroundCursorColor,
              focusNode: focusNode,
              autofocus: autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: onMoveToPreviousBlock,
              onMoveToNextBlock: onMoveToNextBlock,
            ),
          ),
        ],
      ),
    );
  }
}

final class _ListMarkerGlyph extends StatelessWidget {
  const _ListMarkerGlyph({required this.marker, required this.style});

  final _ListMarkerInfo marker;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final markerText = marker.orderedLabel;
    if (markerText != null) {
      return SizedBox(
        key: const Key('FlarkLiveBlockListMarker'),
        width: 24,
        child: Text(
          markerText,
          textAlign: TextAlign.right,
          style: style.copyWith(color: theme.listMarkerColor),
        ),
      );
    }
    return SizedBox(
      key: const Key('FlarkLiveBlockListMarker'),
      width: 16,
      height: (style.fontSize ?? 14) * (style.height ?? 1.2),
      child: CustomPaint(
        painter: _BulletMarkerPainter(color: theme.listMarkerColor),
      ),
    );
  }
}

final class _BulletMarkerPainter extends CustomPainter {
  const _BulletMarkerPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    canvas.drawCircle(Offset(size.width / 2, size.height * 0.52), 2.3, paint);
  }

  @override
  bool shouldRepaint(_BulletMarkerPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}

final class _EditableTaskListItemBlock extends StatefulWidget {
  const _EditableTaskListItemBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.blockHandle,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;
  String get currentDisplayText => blockHandle?.displayText ?? displayText;

  @override
  State<_EditableTaskListItemBlock> createState() {
    return _EditableTaskListItemBlockState();
  }
}

final class _EditableTaskListItemBlockState
    extends State<_EditableTaskListItemBlock> {
  @override
  Widget build(BuildContext context) {
    final block = widget.currentBlock;
    final displayText = widget.currentDisplayText;
    final checked = block.taskListItem?.checked ?? false;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: EdgeInsets.only(
              top: ((widget.style.fontSize ?? 14) * 0.18),
            ),
            child: Semantics(
              key: const Key('FlarkLiveBlockTaskCheckbox'),
              checked: checked,
              label: checked ? 'Task, completed' : 'Task, not completed',
              container: true,
              onTap: () => _toggle(context),
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                excludeFromSemantics: true,
                onTap: () => _toggle(context),
                child: _TaskCheckboxGlyph(checked: checked),
              ),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: _EditableProjectedBlockText(
              controller: widget.controller,
              block: block,
              blockHandle: widget.blockHandle,
              displayText: displayText,
              style: widget.style,
              cursorColor: widget.cursorColor,
              backgroundCursorColor: widget.backgroundCursorColor,
              focusNode: widget.focusNode,
              autofocus: widget.autofocus,
              markdownInputPolicy: true,
              onMoveToPreviousBlock: widget.onMoveToPreviousBlock,
              onMoveToNextBlock: widget.onMoveToNextBlock,
            ),
          ),
        ],
      ),
    );
  }

  void _toggle(BuildContext context) {
    final interactions = FlarkMarkdownInteractions.maybeOf(context);
    final block = widget.currentBlock;
    final checked = block.taskListItem?.checked ?? false;
    if (interactions != null) {
      if (!interactions.config.enableTaskCheckboxToggles) return;
      interactions.setTaskListChecked(block, !checked);
      _restoreFocusAfterToggle();
      return;
    }
    widget.controller.dispatch(
      command: FlarkMarkdownBlockCommands.setTaskListChecked,
      payload: FlarkSetTaskListCheckedPayload(
        taskItemRange: block.sourceRange,
        checked: !checked,
        userEvent: 'input.liveBlock.taskToggle',
      ),
    );
    _restoreFocusAfterToggle();
  }

  void _restoreFocusAfterToggle() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.focusNode?.requestFocus();
    });
  }
}

final class _TaskCheckboxGlyph extends StatelessWidget {
  const _TaskCheckboxGlyph({required this.checked});

  final bool checked;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    return SizedBox(
      width: 15,
      height: 15,
      child: CustomPaint(
        painter: _TaskCheckboxPainter(
          checked: checked,
          checkedColor: theme.checkboxCheckedColor,
          borderColor: theme.checkboxBorderColor,
          fillColor: theme.checkboxFillColor,
          checkmarkColor: theme.checkboxCheckmarkColor,
        ),
      ),
    );
  }
}

final class _TaskCheckboxPainter extends CustomPainter {
  const _TaskCheckboxPainter({
    required this.checked,
    required this.checkedColor,
    required this.borderColor,
    required this.fillColor,
    required this.checkmarkColor,
  });

  final bool checked;
  final Color checkedColor;
  final Color borderColor;
  final Color fillColor;
  final Color checkmarkColor;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final fill = Paint()..color = checked ? checkedColor : fillColor;
    final border = Paint()
      ..color = checked ? checkedColor : borderColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.4;
    final shape = RRect.fromRectAndRadius(rect, const Radius.circular(3));
    canvas.drawRRect(shape, fill);
    canvas.drawRRect(shape, border);
    if (!checked) return;

    final check = Paint()
      ..color = checkmarkColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final path = Path()
      ..moveTo(size.width * 0.25, size.height * 0.52)
      ..lineTo(size.width * 0.43, size.height * 0.70)
      ..lineTo(size.width * 0.76, size.height * 0.30);
    canvas.drawPath(path, check);
  }

  @override
  bool shouldRepaint(_TaskCheckboxPainter oldDelegate) {
    return oldDelegate.checked != checked ||
        oldDelegate.checkedColor != checkedColor ||
        oldDelegate.borderColor != borderColor ||
        oldDelegate.fillColor != fillColor ||
        oldDelegate.checkmarkColor != checkmarkColor;
  }
}

final class _EditableCodeBlock extends StatelessWidget {
  const _EditableCodeBlock({
    required this.controller,
    required this.block,
    required this.displayText,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.focusNode,
    this.autofocus = false,
    this.blockHandle,
    this.onMoveToPreviousBlock,
    this.onMoveToNextBlock,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final String displayText;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final FocusNode? focusNode;
  final bool autofocus;
  final VoidCallback? onMoveToPreviousBlock;
  final VoidCallback? onMoveToNextBlock;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;
  String get currentDisplayText => blockHandle?.displayText ?? displayText;

  static const double _copyChromeReserveWidth = 52;
  static const double _languageChromeReserveWidth = 104;

  @override
  Widget build(BuildContext context) {
    final block = currentBlock;
    final displayText = currentDisplayText;
    final language =
        FlarkLiveCodeFenceInputPolicy.languageFromSource(
          controller.markdown,
          block,
        ) ??
        block.codeBlock?.language;
    final editingOpeningLine =
        FlarkLiveCodeFenceInputPolicy.selectionInOpeningLine(
          markdown: controller.markdown,
          block: block,
          selection: controller.selection,
        );
    if (editingOpeningLine) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: _EditableProjectedBlockText(
          editableKey: const Key('FlarkLiveBlockCodeOpeningEditable'),
          controller: controller,
          block: block,
          blockHandle: blockHandle,
          displayText: displayText,
          style: style,
          cursorColor: cursorColor,
          backgroundCursorColor: backgroundCursorColor,
          focusNode: focusNode,
          autofocus: autofocus,
          markdownInputPolicy: true,
          sourceRangeForEdits: FlarkLiveCodeFenceInputPolicy.openingLineRange,
          onMoveToPreviousBlock: onMoveToPreviousBlock,
          onMoveToNextBlock: onMoveToNextBlock,
        ),
      );
    }
    final theme = FlarkMarkdownTheme.of(context);
    final interactions = FlarkMarkdownInteractions.maybeOf(context);
    final showLanguageSelector =
        interactions != null &&
        interactions.editable &&
        interactions.config.enableCodeFenceLanguagePicker &&
        interactions.config.codeLanguages.isNotEmpty;
    final hasLanguageChrome =
        showLanguageSelector || (language != null && language.isNotEmpty);
    final chromeReserveWidth =
        _copyChromeReserveWidth +
        (hasLanguageChrome ? _languageChromeReserveWidth : 0);
    final editable = _EditableProjectedBlockText(
      editableKey: const Key('FlarkLiveBlockCodeEditable'),
      controller: controller,
      block: block,
      blockHandle: blockHandle,
      displayText: displayText,
      style: style
          .copyWith(
            color: theme.codeTextColor,
            fontFamily: 'monospace',
            height: 1.35,
          )
          .merge(theme.codeTextStyle),
      cursorColor: cursorColor,
      backgroundCursorColor: backgroundCursorColor,
      focusNode: focusNode,
      autofocus: autofocus,
      markdownInputPolicy: true,
      sourceRangeForEdits: FlarkLiveCodeFenceInputPolicy.bodyRange,
      sourceEditForReplacement: FlarkLiveCodeFenceInputPolicy.sourceEdit,
      codeSyntaxLanguage: language,
      onMoveToPreviousBlock: onMoveToPreviousBlock,
      onMoveToNextBlock: onMoveToNextBlock,
    );
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: DecoratedBox(
        key: const Key('FlarkLiveBlockCodeFence'),
        decoration: BoxDecoration(
          color: theme.codeBlockBackgroundColor,
          border: Border.all(color: theme.borderColor),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 9),
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Actions(
                actions: {
                  _CodeIndentIntent: CallbackAction<_CodeIndentIntent>(
                    onInvoke: (intent) {
                      _applyCodeIndent(indent: intent.indent);
                      return null;
                    },
                  ),
                },
                child: Shortcuts(
                  shortcuts: const {
                    SingleActivator(LogicalKeyboardKey.tab): _CodeIndentIntent(
                      indent: true,
                    ),
                    SingleActivator(LogicalKeyboardKey.tab, shift: true):
                        _CodeIndentIntent(indent: false),
                  },
                  child: Padding(
                    padding: EdgeInsets.only(right: chromeReserveWidth),
                    child: editable,
                  ),
                ),
              ),
              Positioned(
                top: 0,
                right: 0,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _CodeCopyButton(
                      source: FlarkLiveCodeFenceInputPolicy.copyText(
                        controller.markdown,
                        block,
                      ),
                      focusNode: focusNode,
                      style: style,
                    ),
                    if (showLanguageSelector || hasLanguageChrome)
                      const SizedBox(width: 6),
                    if (showLanguageSelector)
                      _CodeLanguageSelector(
                        interactions: interactions,
                        block: block,
                        blockHandle: blockHandle,
                        language: language,
                        style: style,
                        focusNode: focusNode,
                      )
                    else if (language != null && language.isNotEmpty)
                      _CodeLanguageBadge(language: language, style: style),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  bool _applyCodeIndent({required bool indent}) {
    final block = currentBlock;
    final bodyRange = FlarkLiveCodeFenceInputPolicy.bodyRange(
      controller.markdown,
      block,
    );
    if (bodyRange == null) return false;
    final selection = controller.selection;
    if (selection.start < bodyRange.start || selection.end > bodyRange.end) {
      return false;
    }

    final operations = indent
        ? FlarkMarkdownFencedCodePolicy.indentOperations(
            markdown: controller.markdown,
            bodyRange: bodyRange,
            selection: selection,
          )
        : FlarkMarkdownFencedCodePolicy.outdentOperations(
            markdown: controller.markdown,
            bodyRange: bodyRange,
            selection: selection,
          );
    if (operations.isEmpty) return false;

    controller.applyTransaction(
      FlarkTransaction(
        operations: operations,
        selectionBefore: selection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: indent
              ? 'input.liveBlock.codeIndent'
              : 'input.liveBlock.codeOutdent',
          parseInvalidationRange: bodyRange,
          projectionInvalidationRange: bodyRange,
        ),
      ),
    );
    return true;
  }
}

final class _CodeCopyButton extends StatelessWidget {
  const _CodeCopyButton({
    required this.source,
    required this.focusNode,
    required this.style,
  });

  final String source;
  final FocusNode? focusNode;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final labelStyle = style.copyWith(
      color: theme.chromeLabelColor,
      fontFamily: 'monospace',
      fontSize: (style.fontSize ?? 14) - 1,
      fontWeight: FontWeight.w700,
    );
    void copy() {
      Clipboard.setData(ClipboardData(text: source));
      final focusNode = this.focusNode;
      if (focusNode == null) return;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (focusNode.canRequestFocus) focusNode.requestFocus();
      });
    }

    return Semantics(
      key: const Key('FlarkLiveBlockCodeCopyButton'),
      button: true,
      label: 'Copy code',
      onTap: copy,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        excludeFromSemantics: true,
        onTap: copy,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: theme.chipBackgroundColor,
            borderRadius: const BorderRadius.all(Radius.circular(4)),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            child: Text('Copy', style: labelStyle),
          ),
        ),
      ),
    );
  }
}

final class _CodeLanguageBadge extends StatelessWidget {
  const _CodeLanguageBadge({required this.language, required this.style});

  final String language;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    return Align(
      alignment: Alignment.centerRight,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: theme.chipBackgroundColor,
          borderRadius: const BorderRadius.all(Radius.circular(4)),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
          child: Text(
            language,
            style: style.copyWith(
              color: theme.chromeLabelColor,
              fontFamily: 'monospace',
              fontSize: (style.fontSize ?? 14) - 1,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
      ),
    );
  }
}

final class _CodeLanguageSelector extends StatefulWidget {
  const _CodeLanguageSelector({
    required this.interactions,
    required this.block,
    required this.language,
    required this.style,
    required this.focusNode,
    this.blockHandle,
  });

  final FlarkMarkdownInteractions interactions;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final String? language;
  final TextStyle style;
  final FocusNode? focusNode;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;

  @override
  State<_CodeLanguageSelector> createState() => _CodeLanguageSelectorState();
}

final class _CodeLanguageSelectorState extends State<_CodeLanguageSelector> {
  static const double _menuWidth = 152;

  final LayerLink _menuAnchor = LayerLink();
  final GlobalKey _buttonBoundsKey = GlobalKey();
  final GlobalKey _menuBoundsKey = GlobalKey();
  OverlayEntry? _menuEntry;
  bool _globalPointerRouteAttached = false;

  bool get _open => _menuEntry != null;

  @override
  void dispose() {
    _closeMenu(notify: false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final currentValue = widget.language ?? '';
    final currentLabel = _languageLabel(currentValue);
    final labelStyle = widget.style.copyWith(
      color: theme.chromeLabelColor,
      fontFamily: 'monospace',
      fontSize: (widget.style.fontSize ?? 14) - 1,
      fontWeight: FontWeight.w700,
    );

    return CompositedTransformTarget(
      link: _menuAnchor,
      child: KeyedSubtree(
        key: const Key('FlarkLiveBlockCodeLanguageButton'),
        child: GestureDetector(
          key: _buttonBoundsKey,
          behavior: HitTestBehavior.opaque,
          onTap: _toggleMenu,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: _open
                  ? theme.chipActiveBackgroundColor
                  : theme.chipBackgroundColor,
              borderRadius: const BorderRadius.all(Radius.circular(4)),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              child: Text(currentLabel, style: labelStyle),
            ),
          ),
        ),
      ),
    );
  }

  void _toggleMenu() {
    if (_open) {
      _closeMenu();
    } else {
      _openMenu();
    }
  }

  void _openMenu() {
    final overlay = Overlay.maybeOf(context);
    if (overlay == null) return;
    _menuEntry = OverlayEntry(builder: _buildMenuOverlay);
    overlay.insert(_menuEntry!);
    _attachGlobalPointerRoute();
    if (mounted) setState(() {});
  }

  Widget _buildMenuOverlay(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final currentValue = widget.language ?? '';
    return CompositedTransformFollower(
      link: _menuAnchor,
      showWhenUnlinked: false,
      targetAnchor: Alignment.bottomRight,
      followerAnchor: Alignment.topRight,
      offset: const Offset(0, 4),
      child: UnconstrainedBox(
        alignment: Alignment.topRight,
        child: SizedBox(
          width: _menuWidth,
          child: KeyedSubtree(
            key: const Key('FlarkLiveBlockCodeLanguageMenu'),
            child: DecoratedBox(
              key: _menuBoundsKey,
              decoration: BoxDecoration(
                color: theme.menuBackgroundColor,
                border: Border.all(color: theme.borderColor),
                borderRadius: const BorderRadius.all(Radius.circular(6)),
                boxShadow: [
                  BoxShadow(
                    color: theme.menuShadowColor,
                    blurRadius: 10,
                    offset: const Offset(0, 4),
                  ),
                ],
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (final option
                        in widget.interactions.config.codeLanguages)
                      _CodeLanguageOptionButton(
                        option: option,
                        selected: option.value == currentValue,
                        style: widget.style,
                        onTap: () => _selectLanguage(option.value),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _attachGlobalPointerRoute() {
    if (_globalPointerRouteAttached) return;
    GestureBinding.instance.pointerRouter.addGlobalRoute(
      _handleGlobalPointerEvent,
    );
    _globalPointerRouteAttached = true;
  }

  void _detachGlobalPointerRoute() {
    if (!_globalPointerRouteAttached) return;
    GestureBinding.instance.pointerRouter.removeGlobalRoute(
      _handleGlobalPointerEvent,
    );
    _globalPointerRouteAttached = false;
  }

  void _handleGlobalPointerEvent(PointerEvent event) {
    if (event is! PointerDownEvent || !_open) return;
    if (_containsGlobalPoint(_buttonBoundsKey.currentContext, event.position)) {
      return;
    }
    if (_containsGlobalPoint(_menuBoundsKey.currentContext, event.position)) {
      return;
    }
    _closeMenu();
  }

  bool _containsGlobalPoint(BuildContext? context, Offset globalPosition) {
    final renderObject = context?.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.attached) return false;
    final localPosition = renderObject.globalToLocal(globalPosition);
    return renderObject.size.contains(localPosition);
  }

  void _closeMenu({bool notify = true}) {
    final entry = _menuEntry;
    if (entry == null) return;
    _menuEntry = null;
    _detachGlobalPointerRoute();
    entry.remove();
    if (notify && mounted) setState(() {});
  }

  void _selectLanguage(String language) {
    _closeMenu();
    final handled = widget.interactions.setCodeFenceLanguage(
      widget.currentBlock,
      language,
    );
    if (!handled) return;
    _adoptImmediateMarkdownParseForController(widget.interactions.controller);

    final focusNode = widget.focusNode;
    if (focusNode == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !focusNode.canRequestFocus) return;
      focusNode.requestFocus();
    });
  }

  String _languageLabel(String value) {
    for (final option in widget.interactions.config.codeLanguages) {
      if (option.value == value) return option.label;
    }
    return value.isEmpty ? 'Auto' : value;
  }
}

final class _CodeLanguageOptionButton extends StatelessWidget {
  const _CodeLanguageOptionButton({
    required this.option,
    required this.selected,
    required this.style,
    required this.onTap,
  });

  final FlarkCodeLanguageOption option;
  final bool selected;
  final TextStyle style;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    return GestureDetector(
      key: ValueKey('FlarkLiveBlockCodeLanguageOption:${option.value}'),
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Text(
          option.label,
          style: style.copyWith(
            color: selected
                ? theme.chromeSelectedLabelColor
                : theme.chromeLabelColor,
            fontSize: (style.fontSize ?? 14) - 1,
            fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
          ),
        ),
      ),
    );
  }
}

final class _CodeIndentIntent extends Intent {
  const _CodeIndentIntent({required this.indent});

  final bool indent;
}

final class _EditableTableBlock extends StatelessWidget {
  const _EditableTableBlock({
    required this.controller,
    required this.block,
    required this.style,
    required this.cursorColor,
    required this.backgroundCursorColor,
    this.blockHandle,
  });

  final FlarkFlutterController controller;
  final FlarkRenderBlock block;
  final _LiveRenderedBlockHandle? blockHandle;
  final TextStyle style;
  final Color cursorColor;
  final Color backgroundCursorColor;

  FlarkRenderBlock get currentBlock => blockHandle?.block ?? block;

  @override
  Widget build(BuildContext context) {
    final block = currentBlock;
    final table = _ParsedEditableTable.fromRenderBlock(
      controller.markdown,
      block,
    );
    if (table == null || table.rows.isEmpty) {
      final displayText = _FlarkLiveRenderedBlockEditorState._projectedText(
        controller,
      );
      return _EditableProjectedBlockText(
        controller: controller,
        block: block,
        displayText: displayText,
        style: style,
        cursorColor: cursorColor,
        backgroundCursorColor: backgroundCursorColor,
      );
    }

    final theme = FlarkMarkdownTheme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: DecoratedBox(
        key: const Key('FlarkLiveBlockTable'),
        decoration: BoxDecoration(
          border: Border.all(color: theme.borderColor),
          borderRadius: const BorderRadius.all(Radius.circular(6)),
        ),
        child: ClipRRect(
          borderRadius: const BorderRadius.all(Radius.circular(6)),
          child: Table(
            defaultVerticalAlignment: TableCellVerticalAlignment.middle,
            border: TableBorder.symmetric(
              inside: BorderSide(color: theme.tableDividerColor),
            ),
            children: [
              for (var rowIndex = 0; rowIndex < table.rows.length; rowIndex++)
                TableRow(
                  decoration: BoxDecoration(
                    color: rowIndex == 0
                        ? theme.tableHeaderBackgroundColor
                        : theme.tableRowBackgroundColor,
                  ),
                  children: [
                    for (
                      var columnIndex = 0;
                      columnIndex < table.rows[rowIndex].cells.length;
                      columnIndex++
                    )
                      Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 6,
                        ),
                        child: _EditableTableCell(
                          key: ValueKey(
                            'table-cell:$rowIndex:$columnIndex:'
                            '${table.rows[rowIndex].cells[columnIndex].range.start}',
                          ),
                          controller: controller,
                          blockHandle: blockHandle,
                          rowIndex: rowIndex,
                          columnIndex: columnIndex,
                          cell: table.rows[rowIndex].cells[columnIndex],
                          style: _tableCellStyle(style, rowIndex),
                          textAlign: _tableTextAlign(block, columnIndex),
                          cursorColor: cursorColor,
                          backgroundCursorColor: backgroundCursorColor,
                          editableKey: Key(
                            'FlarkLiveBlockTableCell-$rowIndex-'
                            '$columnIndex',
                          ),
                        ),
                      ),
                  ],
                ),
            ],
          ),
        ),
      ),
    );
  }

  TextStyle _tableCellStyle(TextStyle base, int rowIndex) {
    if (rowIndex != 0) return base;
    return base.copyWith(fontWeight: FontWeight.w700);
  }

  TextAlign _tableTextAlign(FlarkRenderBlock block, int columnIndex) {
    final alignments = block.table?.columnAlignments ?? const [];
    if (columnIndex >= alignments.length) return TextAlign.left;
    return switch (alignments[columnIndex]) {
      FlarkRenderTableColumnAlignment.center => TextAlign.center,
      FlarkRenderTableColumnAlignment.right => TextAlign.right,
      FlarkRenderTableColumnAlignment.left ||
      FlarkRenderTableColumnAlignment.none ||
      FlarkRenderTableColumnAlignment.unknown => TextAlign.left,
    };
  }
}

final class _EditableTableCell extends StatefulWidget {
  const _EditableTableCell({
    super.key,
    required this.controller,
    required this.rowIndex,
    required this.columnIndex,
    required this.cell,
    required this.style,
    required this.textAlign,
    required this.cursorColor,
    required this.backgroundCursorColor,
    required this.editableKey,
    this.blockHandle,
  });

  final FlarkFlutterController controller;
  final _LiveRenderedBlockHandle? blockHandle;
  final int rowIndex;
  final int columnIndex;
  final _ParsedEditableTableCell cell;
  final TextStyle style;
  final TextAlign textAlign;
  final Color cursorColor;
  final Color backgroundCursorColor;
  final Key editableKey;

  _ParsedEditableTableCell get currentCell {
    final handle = blockHandle;
    if (handle == null) return cell;
    final table = _ParsedEditableTable.fromRenderBlock(
      controller.markdown,
      handle.block,
    );
    if (table == null ||
        rowIndex >= table.rows.length ||
        columnIndex >= table.rows[rowIndex].cells.length) {
      return cell;
    }
    return table.rows[rowIndex].cells[columnIndex];
  }

  @override
  State<_EditableTableCell> createState() => _EditableTableCellState();
}

final class _EditableTableCellState extends State<_EditableTableCell> {
  final _editableStateKey = GlobalKey<EditableTextState>();
  late final TextEditingController _textController;
  late final FocusNode _focusNode;
  final _compositionUndoGrouping = _FlarkCompositionUndoGrouping();
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    _textController = TextEditingController();
    _focusNode = FocusNode();
    _textController.addListener(_handleTextChanged);
    _syncFromController();
  }

  @override
  void didUpdateWidget(_EditableTableCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncFromController();
  }

  @override
  void dispose() {
    _textController.removeListener(_handleTextChanged);
    _textController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final editor = EditableText(
      readOnly: FlarkEditorReadOnlyScope.of(context),
      key: _editableStateKey,
      controller: _textController,
      focusNode: _focusNode,
      style: widget.style,
      textAlign: widget.textAlign,
      cursorColor: widget.cursorColor,
      selectionColor:
          FlarkMarkdownTheme.of(context).selectionColor ??
          _selectionColorForCursor(widget.cursorColor),
      selectionControls: flarkTextSelectionControlsForPlatform(context),
      backgroundCursorColor: widget.backgroundCursorColor,
      keyboardType: TextInputType.multiline,
      textInputAction: TextInputAction.newline,
      inputFormatters: const [_TableCellInputFormatter()],
      minLines: 1,
      maxLines: null,
      paintCursorAboveText: true,
      rendererIgnoresPointer: true,
    );
    return flarkEditableTextGestureDetector(
      key: widget.editableKey,
      editableTextKey: _editableStateKey,
      child: editor,
    );
  }

  void _handleTextChanged() {
    if (_syncing) return;
    final cell = widget.currentCell;
    final oldLocalSelection = _localSelection(cell.text.length);
    final value = flarkTextValueWithPureInsertionSelection(
      oldText: cell.text,
      oldSelection: oldLocalSelection,
      newValue: _textController.value,
    );
    _adoptNormalizedTextControllerValue(value);
    final compositionUndoGroupId = _compositionUndoGrouping.groupIdFor(value);
    if (value.text != cell.text) {
      final replacement = cell.replacementText(value.text);
      _replaceSourceRange(
        controller: widget.controller,
        range: cell.range,
        replacementText: replacement,
        userEvent: 'input.liveBlock.tableCell',
        undoGroupId: compositionUndoGroupId,
        selectionAfter: _tableCellSelectionAfterReplacement(
          cell: cell,
          value: value,
        ),
      );
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }

    final selection = value.selection;
    if (!selection.isValid) {
      _compositionUndoGrouping.clearIfCommitted(value);
      return;
    }
    widget.controller.applySelection(
      FlarkSelection(
        baseOffset: cell.range.start + selection.baseOffset,
        extentOffset: cell.range.start + selection.extentOffset,
      ),
      userEvent: 'selection.liveBlock.tableCell',
    );
    _compositionUndoGrouping.clearIfCommitted(value);
  }

  void _syncFromController() {
    final current = _textController.value;
    final cell = widget.currentCell;
    final selection = _localSelection(cell.text.length);
    final next = TextEditingValue(
      text: cell.text,
      selection: selection,
      composing: current.text == cell.text
          ? current.composing
          : TextRange.empty,
    );
    if (current == next) return;
    _syncing = true;
    _textController.value = next;
    _syncing = false;
  }

  void _adoptNormalizedTextControllerValue(TextEditingValue value) {
    if (_textController.value == value) return;
    _syncing = true;
    _textController.value = value;
    _syncing = false;
  }

  TextSelection _localSelection(int textLength) {
    final cell = widget.currentCell;
    final selection = widget.controller.selection;
    if (selection.start < cell.range.start || selection.end > cell.range.end) {
      final current = _textController.selection;
      if (current.isValid &&
          current.baseOffset <= textLength &&
          current.extentOffset <= textLength) {
        return current;
      }
      return TextSelection.collapsed(offset: textLength);
    }
    return TextSelection(
      baseOffset: _localOffsetInsideSanitizedTableCell(
        cell.text,
        selection.baseOffset - cell.range.start,
      ),
      extentOffset: _localOffsetInsideSanitizedTableCell(
        cell.text,
        selection.extentOffset - cell.range.start,
      ),
    );
  }
}

final class _ParsedEditableTable {
  const _ParsedEditableTable({required this.rows});

  final List<_ParsedEditableTableRow> rows;

  static _ParsedEditableTable? fromRenderBlock(
    String markdown,
    FlarkRenderBlock block,
  ) {
    final table = block.table;
    if (table == null || table.rows.isEmpty) return null;

    final columnCount = _resolvedRenderTableColumnCount(table);
    if (columnCount <= 0) return const _ParsedEditableTable(rows: []);

    return _ParsedEditableTable(
      rows: [
        for (final row in table.rows)
          _ParsedEditableTableRow(
            insertionOffset: _rowInsertionOffset(markdown, row),
            cells: [
              for (var index = 0; index < columnCount; index++)
                if (index < row.cells.length)
                  _cellFromDescriptor(markdown, row.cells[index])
                else
                  _emptyCellAfterDescriptor(markdown, row),
            ],
          ),
      ],
    );
  }

  static int _resolvedRenderTableColumnCount(FlarkRenderTableDescriptor table) {
    if (table.columnAlignments.isNotEmpty) return table.columnAlignments.length;
    var columnCount = 0;
    for (final row in table.rows) {
      if (row.cells.length > columnCount) columnCount = row.cells.length;
    }
    return columnCount;
  }

  static int _rowInsertionOffset(
    String markdown,
    FlarkRenderTableRowDescriptor row,
  ) {
    if (row.cells.isNotEmpty) {
      return _trimmedCellRange(markdown, row.cells.last.sourceRange).end;
    }
    return row.sourceRange.end;
  }

  static _ParsedEditableTableCell _cellFromDescriptor(
    String markdown,
    FlarkRenderTableCellDescriptor cell,
  ) {
    final contentRange = _trimmedCellRange(markdown, cell.sourceRange);
    return _ParsedEditableTableCell(
      text: _unescapeCellText(
        markdown.substring(contentRange.start, contentRange.end),
      ),
      range: contentRange,
    );
  }

  static FlarkSourceRange _trimmedCellRange(
    String markdown,
    FlarkSourceRange range,
  ) {
    var start = range.start.clamp(0, markdown.length);
    var end = range.end.clamp(start, markdown.length);
    while (start < end && _isWhitespace(markdown.codeUnitAt(start))) {
      start++;
    }
    while (end > start && _isWhitespace(markdown.codeUnitAt(end - 1))) {
      end--;
    }
    return FlarkSourceRange(start, end);
  }

  static _ParsedEditableTableCell _emptyCellAfterDescriptor(
    String markdown,
    FlarkRenderTableRowDescriptor row,
  ) {
    final insertionOffset = _rowInsertionOffset(markdown, row);
    return _ParsedEditableTableCell(
      text: '',
      range: FlarkSourceRange(insertionOffset, insertionOffset),
      replacementPrefix: ' | ',
    );
  }

  static bool _isWhitespace(int codeUnit) {
    return codeUnit == 32 || codeUnit == 9;
  }

  static String _unescapeCellText(String text) {
    return text.replaceAll(r'\|', '|');
  }
}

final class _ParsedEditableTableRow {
  const _ParsedEditableTableRow({
    required this.cells,
    required this.insertionOffset,
  });

  final List<_ParsedEditableTableCell> cells;
  final int insertionOffset;
}

final class _ParsedEditableTableCell {
  const _ParsedEditableTableCell({
    required this.text,
    required this.range,
    this.replacementPrefix = '',
  });

  final String text;
  final FlarkSourceRange range;
  final String replacementPrefix;

  String replacementText(String value) {
    return '$replacementPrefix${_sanitizeTableCell(value)}';
  }
}
