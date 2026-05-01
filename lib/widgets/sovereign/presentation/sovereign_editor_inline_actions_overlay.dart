part of 'sovereign_editor.dart';

extension _SovereignEditorInlineActionsOverlay on _SovereignEditorState {
  void _scheduleLinkActionsOverlaySync() {
    if (_linkActionsOverlaySyncScheduled || !mounted) return;
    _linkActionsOverlaySyncScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _linkActionsOverlaySyncScheduled = false;
      if (!mounted) return;
      _syncLinkActionsOverlay();
    });
  }

  void _removeLinkActionsOverlayEntry() {
    _linkActionsOverlayEntry?.remove();
    _linkActionsOverlayEntry = null;
    _linkActionsOverlayData = null;
  }

  void _syncLinkActionsOverlay() {
    final data = _computeLinkActionsOverlayData();
    if (data == null) {
      _removeLinkActionsOverlayEntry();
      return;
    }

    _linkActionsOverlayData = data;
    if (_linkActionsOverlayEntry == null) {
      final overlay = Overlay.of(context, rootOverlay: true);
      _linkActionsOverlayEntry = OverlayEntry(
        builder: (_) => _buildLinkActionsOverlayEntry(),
      );
      overlay.insert(_linkActionsOverlayEntry!);
      return;
    }
    _linkActionsOverlayEntry!.markNeedsBuild();
  }

  _LinkActionsOverlayData? _computeLinkActionsOverlayData() {
    if (!widget.showLinkActionsOverlay) return null;
    if (!_effectiveFocusNode.hasFocus) return null;
    if (widget.controller.value.composing.isValid) return null;

    final selection = widget.controller.selection;
    if (!selection.isValid || !selection.isCollapsed) return null;

    final caret = selection.baseOffset;
    final text = widget.controller.text;
    if (caret < 0 || caret > text.length) return null;
    if (_isOffsetInsideCodeBlock(caret)) return null;

    final resolvedTarget = resolveSovereignInlineActionsTargetAtCaret(
      text,
      caret,
    );
    if (resolvedTarget == null) return null;
    final target = resolvedTarget.target;
    final standaloneImagePreview =
        target.isImage && isStandaloneInlineImageTarget(text, target);
    final resolvedUrl =
        resolvedTarget.resolvedUrl.isEmpty ? null : resolvedTarget.resolvedUrl;

    final overlayState = Overlay.maybeOf(context, rootOverlay: true);
    final overlayContext = overlayState?.context;
    final overlayRender = overlayContext?.findRenderObject();
    final editorStackContext = _overlayStackKey.currentContext;
    final editorStackRender = editorStackContext?.findRenderObject();
    if (overlayRender is! RenderBox || editorStackRender is! RenderBox) {
      return null;
    }

    final editorTopLeftGlobal = editorStackRender.localToGlobal(Offset.zero);
    final editorBottomRightGlobal = editorStackRender.localToGlobal(
      editorStackRender.size.bottomRight(Offset.zero),
    );
    final editorRectInOverlay = Rect.fromPoints(
      overlayRender.globalToLocal(editorTopLeftGlobal),
      overlayRender.globalToLocal(editorBottomRightGlobal),
    );

    final editorTheme = _editorThemeData.linkActions;
    final imagePreviewWidthCap = standaloneImagePreview
        ? editorTheme.overlayMaxStandaloneImageWidth
        : editorTheme.overlayMaxInlineImageWidth;
    final overlayMaxWidthCap =
        target.isImage ? imagePreviewWidthCap : editorTheme.overlayMaxLinkWidth;
    final estimatedOverlayWidth = target.isImage
        ? (standaloneImagePreview
            ? editorTheme.estimatedStandaloneImageWidth
            : editorTheme.estimatedInlineImageWidth)
        : editorTheme.estimatedLinkWidth;
    final estimatedOverlayHeight = target.isImage
        ? (standaloneImagePreview
            ? editorTheme.estimatedStandaloneImageHeight
            : editorTheme.estimatedInlineImageHeight)
        : editorTheme.estimatedLinkHeight;

    final caretRect = _caretRectInOverlaySpace(
      selection,
      targetRenderBox: overlayRender,
    );
    final lineIndex = widget.controller.decoration.lineIndex.lineAtOffset(
      caret,
    );
    final anchorX = caretRect?.right ??
        (editorRectInOverlay.left + (target.displayEnd * _charWidth) + 10);
    final lineTop = caretRect?.top ??
        (editorRectInOverlay.top +
            (_lineHeightPixels * lineIndex) -
            _viewportNotifier.value.dy);
    final lineBottom = caretRect?.bottom ?? (lineTop + _lineHeightPixels);

    final placement = computeSovereignInlineActionsPlacement(
      theme: editorTheme,
      hostRect: editorRectInOverlay,
      anchorX: anchorX,
      anchorTop: lineTop,
      anchorBottom: lineBottom,
      estimatedWidth: estimatedOverlayWidth,
      estimatedHeight: estimatedOverlayHeight,
      maxWidthCap: overlayMaxWidthCap,
      preferAbove: true,
    );

    return _LinkActionsOverlayData(
      top: placement.top,
      left: placement.left,
      maxWidth: placement.maxWidth,
      target: target,
      standaloneImagePreview: standaloneImagePreview,
      textSnapshot: resolvedTarget.textSnapshot,
      resolvedUrl: resolvedUrl,
      openEnabled: widget.onOpenLink != null && resolvedUrl != null,
      theme: editorTheme,
    );
  }

  String? _resolvedInlineTargetUrl(
    SovereignInlineActionsTarget target,
    String text,
  ) {
    if (target.isImage) return target.urlText(text);
    final link = sovereignInlineTargetAsLinkMatch(target);
    if (link == null) return target.urlText(text);
    if (link.kind == SovereignLinkMatchKind.reference) {
      return SovereignStyleScanner.resolveReferenceLinkUrl(text, link);
    }
    return link.urlText(text);
  }

  bool _applyEditedReferenceLinkTarget(
    SovereignInlineActionsTarget target, {
    required String label,
    required String url,
  }) {
    final oldValue = widget.controller.value;
    final oldText = oldValue.text;
    final link = sovereignInlineTargetAsLinkMatch(target);
    if (link == null || link.kind != SovereignLinkMatchKind.reference) {
      return false;
    }
    final definition = SovereignStyleScanner.referenceDefinitionForLink(
      oldText,
      link,
    );
    if (definition == null) return false;
    final rawReferenceLabel = link.referenceLabelText(oldText);
    if (rawReferenceLabel == null || rawReferenceLabel.isEmpty) return false;

    final safeLabel = _escapeMarkdownLinkLabel(label);
    final safeReferenceLabel = _escapeMarkdownLinkLabel(rawReferenceLabel);
    final trimmedUrl = url.trim();
    final linkReplacement = '[$safeLabel][$safeReferenceLabel]';

    final replacedLinkText = oldText.replaceRange(
      target.fullStart,
      target.fullEnd,
      linkReplacement,
    );
    final linkDelta =
        linkReplacement.length - (target.fullEnd - target.fullStart);

    int shiftIfAfterLink(int offset) =>
        offset >= target.fullEnd ? offset + linkDelta : offset;
    final defUrlStart = shiftIfAfterLink(definition.urlStart);
    final defUrlEnd = shiftIfAfterLink(definition.urlEnd);
    if (defUrlStart < 0 ||
        defUrlEnd > replacedLinkText.length ||
        defUrlStart > defUrlEnd) {
      return false;
    }

    final newText = replacedLinkText.replaceRange(
      defUrlStart,
      defUrlEnd,
      trimmedUrl,
    );
    final caret = (target.fullStart + 1 + safeLabel.length).clamp(
      0,
      newText.length,
    );
    widget.controller.value = oldValue.copyWith(
      text: newText,
      selection: TextSelection.collapsed(offset: caret),
      composing: TextRange.empty,
    );
    return true;
  }

  void _applyEditedInlineTarget(
    SovereignInlineActionsTarget target, {
    required String label,
    required String url,
  }) {
    final oldValue = widget.controller.value;
    final oldText = oldValue.text;
    if (target.fullStart < 0 ||
        target.fullEnd > oldText.length ||
        target.fullStart >= target.fullEnd) {
      return;
    }

    if (_applyEditedReferenceLinkTarget(target, label: label, url: url)) {
      return;
    }

    final replacement = _inlineTargetReplacementText(
      target: target,
      label: label,
      url: url,
    );
    final newText = oldText.replaceRange(
      target.fullStart,
      target.fullEnd,
      replacement,
    );
    final replacementIsImage = replacement.startsWith('![');
    final replacementIsMarkdownLink = replacement.startsWith('[');
    final caret = replacementIsImage
        ? (target.fullStart + 2 + _escapeMarkdownLinkLabel(label).length)
        : replacementIsMarkdownLink
            ? (target.fullStart + 1 + _escapeMarkdownLinkLabel(label).length)
            : (target.fullStart + replacement.length);

    widget.controller.value = oldValue.copyWith(
      text: newText,
      selection: TextSelection.collapsed(
        offset: caret.clamp(0, newText.length),
      ),
      composing: TextRange.empty,
    );
  }

  Future<void> _showEditInlineTargetDialog(
    SovereignInlineActionsTarget target,
  ) async {
    final text = widget.controller.text;
    if (target.fullEnd > text.length) return;

    final initialUrl = _resolvedInlineTargetUrl(target, text) ??
        (target.linkKind == SovereignLinkMatchKind.reference
            ? ''
            : target.urlText(text));
    final initialLabel = target.labelText(text);

    if (widget.onEditInlineTarget != null) {
      final result = await widget.onEditInlineTarget!(
        context,
        initialLabel,
        initialUrl,
        target.isImage,
      );
      if (!mounted || result == null) return;
      if (result.url.trim().isEmpty) return;
      _applyEditedInlineTarget(target, label: result.label, url: result.url);
      return;
    }

    final labelController = TextEditingController(text: initialLabel);
    final urlController = TextEditingController(text: initialUrl);
    final linkDialogTheme = SovereignEditorThemeScope.of(
      context,
    ).linkEditDialog;
    final fieldRadius = BorderRadius.circular(
      (linkDialogTheme.borderRadius - 4).clamp(8.0, 20.0),
    );
    InputDecoration decorationFor(String label) => InputDecoration(
          labelText: label,
          labelStyle: linkDialogTheme.fieldLabelStyle,
          filled: true,
          fillColor: linkDialogTheme.fieldFillColor,
          isDense: true,
          contentPadding:
              const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          border: OutlineInputBorder(
            borderRadius: fieldRadius,
            borderSide: BorderSide(color: linkDialogTheme.fieldBorderColor),
          ),
          enabledBorder: OutlineInputBorder(
            borderRadius: fieldRadius,
            borderSide: BorderSide(color: linkDialogTheme.fieldBorderColor),
          ),
          focusedBorder: OutlineInputBorder(
            borderRadius: fieldRadius,
            borderSide: BorderSide(
              color: linkDialogTheme.fieldFocusedBorderColor,
              width: 1.2,
            ),
          ),
        );

    final result = await showDialog<_EditedLinkResult>(
      context: context,
      barrierColor: linkDialogTheme.barrierColor,
      builder: (context) {
        return AlertDialog(
          backgroundColor: linkDialogTheme.backgroundColor,
          surfaceTintColor: Colors.transparent,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(linkDialogTheme.borderRadius),
            side: BorderSide(color: linkDialogTheme.borderColor),
          ),
          titlePadding: const EdgeInsets.fromLTRB(20, 18, 20, 6),
          contentPadding: const EdgeInsets.fromLTRB(20, 8, 20, 8),
          actionsPadding: const EdgeInsets.fromLTRB(14, 0, 14, 14),
          title: Text(
            target.isImage ? 'Edit image' : 'Edit link',
            style: linkDialogTheme.titleStyle,
          ),
          content: SizedBox(
            width: linkDialogTheme.width,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: labelController,
                  style: linkDialogTheme.fieldTextStyle,
                  cursorColor: widget.theme?.cursorColor ??
                      Theme.of(context).colorScheme.primary,
                  decoration: decorationFor(
                    target.isImage ? 'Alt text' : 'Label',
                  ),
                  autofocus: true,
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: urlController,
                  style: linkDialogTheme.fieldTextStyle,
                  cursorColor: widget.theme?.cursorColor ??
                      Theme.of(context).colorScheme.primary,
                  decoration: decorationFor('URL'),
                  keyboardType: TextInputType.url,
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              style: TextButton.styleFrom(
                foregroundColor: linkDialogTheme.cancelForegroundColor,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(
                    (linkDialogTheme.borderRadius - 6).clamp(8.0, 18.0),
                  ),
                ),
              ),
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Cancel'),
            ),
            FilledButton(
              style: FilledButton.styleFrom(
                backgroundColor: linkDialogTheme.saveBackgroundColor,
                foregroundColor: linkDialogTheme.saveForegroundColor,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(
                    (linkDialogTheme.borderRadius - 6).clamp(8.0, 18.0),
                  ),
                ),
              ),
              onPressed: () {
                Navigator.of(context).pop(
                  _EditedLinkResult(
                    label: target.isImage
                        ? labelController.text
                        : (labelController.text.trim().isEmpty
                            ? urlController.text.trim()
                            : labelController.text),
                    url: urlController.text,
                  ),
                );
              },
              child: const Text('Save'),
            ),
          ],
        );
      },
    );

    if (!mounted || result == null) return;
    if (result.url.trim().isEmpty) return;
    _applyEditedInlineTarget(target, label: result.label, url: result.url);
  }
}

class _EditedLinkResult {
  final String label;
  final String url;

  const _EditedLinkResult({required this.label, required this.url});
}

class _LinkActionsOverlayData {
  final double top;
  final double left;
  final double maxWidth;
  final SovereignInlineActionsTarget target;
  final bool standaloneImagePreview;
  final String textSnapshot;
  final String? resolvedUrl;
  final bool openEnabled;
  final SovereignLinkActionsTheme theme;

  const _LinkActionsOverlayData({
    required this.top,
    required this.left,
    required this.maxWidth,
    required this.target,
    required this.standaloneImagePreview,
    required this.textSnapshot,
    required this.resolvedUrl,
    required this.openEnabled,
    required this.theme,
  });
}
