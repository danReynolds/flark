part of 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

extension _SovereignEditorInlineActionsOverlayEntry on _SovereignEditorState {
  Widget _buildLinkActionsOverlayEntry() {
    final data = _linkActionsOverlayData;
    if (data == null) return const SizedBox.shrink();

    if (widget.inlineToolbarBuilder != null) {
      return Positioned(
        top: data.top,
        left: data.left,
        child: TextFieldTapRegion(
          child: SovereignInlineActionsOverlayCard(
            theme: data.theme,
            maxWidth: data.maxWidth,
            target: data.target,
            textSnapshot: data.textSnapshot,
            resolvedUrl: data.resolvedUrl,
            openEnabled: data.openEnabled,
            standaloneImagePreview: data.standaloneImagePreview,
            toolbarBuilder: widget.inlineToolbarBuilder,
            onOpen: data.openEnabled
                ? () async {
                    await widget.onOpenLink!.call(data.resolvedUrl!);
                  }
                : null,
            onCopy: data.resolvedUrl == null
                ? null
                : () => copyInlineTargetUrlToClipboard(data.resolvedUrl),
            onEdit: () => _showEditInlineTargetDialog(data.target),
          ),
        ),
      );
    }

    return Positioned(
      top: data.top,
      left: data.left,
      child: TextFieldTapRegion(
        child: SovereignInlineActionsOverlayCard(
          theme: data.theme,
          maxWidth: data.maxWidth,
          target: data.target,
          textSnapshot: data.textSnapshot,
          resolvedUrl: data.resolvedUrl,
          openEnabled: data.openEnabled,
          standaloneImagePreview: data.standaloneImagePreview,
          margin: data.theme.margin.copyWith(left: 0),
          onOpen: data.openEnabled
              ? () async {
                  await widget.onOpenLink!.call(data.resolvedUrl!);
                }
              : null,
          onCopy: data.resolvedUrl == null
              ? null
              : () => copyInlineTargetUrlToClipboard(data.resolvedUrl),
          onEdit: () => _showEditInlineTargetDialog(data.target),
        ),
      ),
    );
  }

  String _escapeMarkdownLinkLabel(String input) {
    return input.replaceAll(r'\', r'\\').replaceAll(']', r'\]');
  }

  String _inlineTargetReplacementText({
    required SovereignInlineActionsTarget target,
    required String label,
    required String url,
  }) {
    final safeLabel = _escapeMarkdownLinkLabel(label);
    final trimmedUrl = url.trim();
    if (target.isImage) {
      return '![$safeLabel]($trimmedUrl)';
    }
    final preserveBareStyle = (target.linkKind == SovereignLinkMatchKind.bare ||
            target.linkKind == SovereignLinkMatchKind.autolink) &&
        safeLabel == trimmedUrl;
    if (preserveBareStyle) {
      if (target.linkKind == SovereignLinkMatchKind.autolink) {
        return '<$trimmedUrl>';
      }
      return trimmedUrl;
    }
    return '[$safeLabel]($trimmedUrl)';
  }
}
