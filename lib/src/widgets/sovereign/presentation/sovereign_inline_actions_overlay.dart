import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

import 'sovereign_inline_actions_targeting.dart';

export 'sovereign_inline_actions_targeting.dart';

const Key kSovereignLinkActionsOverlayKey = Key('SovereignLinkActionsOverlay');
const Key kSovereignInlineImagePreviewKey = Key('SovereignInlineImagePreview');
const Key kSovereignInlineImagePreviewCaptionKey = Key(
  'SovereignInlineImagePreviewCaption',
);
const Key kSovereignInlineImagePreviewImageAreaKey = Key(
  'SovereignInlineImagePreviewImageArea',
);
const Key kSovereignInlineImagePreviewRetryKey = Key(
  'SovereignInlineImagePreviewRetry',
);
const Key kSovereignInlineImagePreviewUnsupportedKey = Key(
  'SovereignInlineImagePreviewUnsupported',
);

typedef SovereignInlineToolbarBuilder = Widget Function(
  BuildContext context, {
  required String? url,
  required bool isImage,
  required VoidCallback? onOpen,
  required VoidCallback? onCopy,
  required VoidCallback onEdit,
});

class SovereignInlineActionsOverlayCard extends StatefulWidget {
  const SovereignInlineActionsOverlayCard({
    super.key,
    required this.theme,
    required this.maxWidth,
    required this.target,
    required this.textSnapshot,
    required this.resolvedUrl,
    required this.openEnabled,
    required this.standaloneImagePreview,
    this.onOpen,
    this.onCopy,
    this.onEdit,
    this.toolbarBuilder,
    this.margin = EdgeInsets.zero,
  });

  final SovereignLinkActionsTheme theme;
  final double maxWidth;
  final SovereignInlineActionsTarget target;
  final String textSnapshot;
  final String? resolvedUrl;
  final bool openEnabled;
  final bool standaloneImagePreview;
  final VoidCallback? onOpen;
  final VoidCallback? onCopy;
  final VoidCallback? onEdit;
  final SovereignInlineToolbarBuilder? toolbarBuilder;
  final EdgeInsets margin;

  @override
  State<SovereignInlineActionsOverlayCard> createState() =>
      _SovereignInlineActionsOverlayCardState();
}

class _SovereignInlineActionsOverlayCardState
    extends State<SovereignInlineActionsOverlayCard> {
  int _imageReloadNonce = 0;

  void _retryInlineImagePreview() {
    setState(() {
      _imageReloadNonce++;
    });
  }

  @override
  Widget build(BuildContext context) {
    if (widget.toolbarBuilder != null) {
      final editCallback = widget.onEdit;
      if (editCallback == null) {
        return const SizedBox.shrink();
      }
      return widget.toolbarBuilder!(
        context,
        url: widget.resolvedUrl,
        isImage: widget.target.isImage,
        onOpen: widget.onOpen,
        onCopy: widget.onCopy,
        onEdit: editCallback,
      );
    }

    final hasUrl = (widget.resolvedUrl?.isNotEmpty ?? false);
    final actionWidgets = <Widget>[
      if (hasUrl)
        _buildAction(
          icon: Icons.open_in_new_rounded,
          label: 'Open',
          onTap: widget.openEnabled ? widget.onOpen : null,
        ),
      if (hasUrl)
        _buildAction(
          icon: Icons.content_copy_rounded,
          label: 'Copy',
          onTap: widget.onCopy,
        ),
      if (widget.onEdit != null)
        _buildAction(
          icon:
              widget.target.isImage ? Icons.image_outlined : Icons.link_rounded,
          label: 'Edit',
          onTap: widget.onEdit,
        ),
    ];

    if (actionWidgets.isEmpty) {
      return const SizedBox.shrink();
    }

    return Container(
      key: kSovereignLinkActionsOverlayKey,
      constraints: BoxConstraints(maxWidth: widget.maxWidth),
      margin: widget.margin,
      child: Material(
        color: widget.theme.backgroundColor,
        elevation: widget.theme.elevation,
        borderRadius: BorderRadius.circular(widget.theme.borderRadius),
        child: Container(
          padding: widget.theme.padding,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(widget.theme.borderRadius),
            border: Border.all(color: widget.theme.borderColor),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (widget.target.isImage) _buildImagePreview(),
              Wrap(
                spacing: widget.theme.actionGap,
                runSpacing: widget.theme.actionGap,
                children: actionWidgets,
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildAction({
    required IconData icon,
    required String label,
    required VoidCallback? onTap,
  }) {
    final disabled = onTap == null;
    final radius = BorderRadius.circular(widget.theme.actionBorderRadius);
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: radius,
        onTap: onTap,
        child: Ink(
          decoration: BoxDecoration(
            color: widget.theme.actionBackgroundColor,
            borderRadius: radius,
            border: widget.theme.actionBorderColor == null
                ? null
                : Border.all(color: widget.theme.actionBorderColor!),
          ),
          child: Padding(
            padding: widget.theme.actionPadding,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  icon,
                  size: 14,
                  color: disabled
                      ? widget.theme.iconColor.withValues(alpha: 0.35)
                      : widget.theme.iconColor,
                ),
                const SizedBox(width: 4),
                Text(
                  label,
                  style: widget.theme.textStyle.copyWith(
                    color: disabled
                        ? widget.theme.textStyle.color?.withValues(alpha: 0.45)
                        : widget.theme.textStyle.color,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildImagePreview() {
    final radius = BorderRadius.circular(
      (widget.theme.borderRadius - 2).clamp(8.0, 16.0),
    );
    final imageUrl =
        widget.resolvedUrl ?? widget.target.urlText(widget.textSnapshot);
    final altText = widget.target.labelText(widget.textSnapshot);
    final previewableNetworkImage = _isPreviewableNetworkImageUrl(imageUrl);
    final isStandalone = widget.standaloneImagePreview;
    final previewWidth = widget.maxWidth.clamp(
      180.0,
      isStandalone ? 520.0 : 300.0,
    );
    final previewHeight = isStandalone ? 180.0 : 132.0;
    final previewMaxHeight = isStandalone ? 264.0 : 196.0;
    final showUrl = isStandalone;
    final previewBorderColor = widget.theme.imagePreviewBorderColor ??
        widget.theme.actionBorderColor ??
        widget.theme.borderColor;
    final previewBackgroundColor = widget.theme.imagePreviewBackgroundColor ??
        (widget.theme.actionBackgroundColor ?? widget.theme.backgroundColor)
            .withValues(alpha: 0.85);
    final loadingBackgroundColor =
        widget.theme.imagePreviewLoadingBackgroundColor ??
            widget.theme.backgroundColor.withValues(alpha: 0.55);
    final errorBackgroundColor =
        widget.theme.imagePreviewErrorBackgroundColor ??
            widget.theme.backgroundColor.withValues(alpha: 0.6);
    final captionBackgroundColor = widget.theme.imageCaptionBackgroundColor ??
        widget.theme.backgroundColor.withValues(alpha: 0.34);
    final captionBorderColor = widget.theme.imageCaptionBorderColor ??
        widget.theme.actionBorderColor ??
        widget.theme.borderColor.withValues(alpha: 0.65);
    final captionTextStyle = widget.theme.imageCaptionTextStyle ??
        widget.theme.textStyle.copyWith(
          fontWeight: FontWeight.w600,
          color: widget.theme.textStyle.color?.withValues(alpha: 0.92),
        );
    final imageUrlTextStyle = widget.theme.imageUrlTextStyle ??
        widget.theme.textStyle.copyWith(
          fontSize: (widget.theme.textStyle.fontSize ?? 11) - 1,
          color: widget.theme.textStyle.color?.withValues(alpha: 0.62),
          fontWeight: FontWeight.w500,
        );

    Widget buildImageSurface() {
      if (!previewableNetworkImage) {
        final unsupportedWidget = DecoratedBox(
          key: kSovereignInlineImagePreviewUnsupportedKey,
          decoration: BoxDecoration(color: errorBackgroundColor),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 240),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 10,
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      Icons.image_not_supported_outlined,
                      color: widget.theme.iconColor.withValues(alpha: 0.86),
                      size: 28,
                    ),
                    const SizedBox(height: 6),
                    Text(
                      'Preview unavailable',
                      textAlign: TextAlign.center,
                      style: widget.theme.textStyle.copyWith(
                        fontWeight: FontWeight.w600,
                        color: widget.theme.textStyle.color?.withValues(
                          alpha: 0.9,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
        final imageBody = SizedBox(
          key: kSovereignInlineImagePreviewImageAreaKey,
          height: previewHeight,
          child: unsupportedWidget,
        );
        if (!isStandalone || widget.onOpen == null) {
          return imageBody;
        }
        return Material(
          color: Colors.transparent,
          child: InkWell(onTap: widget.onOpen, child: imageBody),
        );
      }

      final imageWidget = Image.network(
        imageUrl,
        key: ValueKey<String>(
          'SovereignInlineImagePreviewImage:$imageUrl:$_imageReloadNonce',
        ),
        fit: BoxFit.cover,
        gaplessPlayback: true,
        filterQuality: FilterQuality.medium,
        frameBuilder: (context, child, frame, wasSynchronouslyLoaded) {
          final visible = wasSynchronouslyLoaded || frame != null;
          return AnimatedOpacity(
            opacity: visible ? 1 : 0,
            duration: const Duration(milliseconds: 140),
            curve: Curves.easeOut,
            child: child,
          );
        },
        loadingBuilder: (context, child, progress) {
          if (progress == null) return child;
          final progressValue = progress.expectedTotalBytes == null ||
                  progress.expectedTotalBytes == 0
              ? null
              : progress.cumulativeBytesLoaded / progress.expectedTotalBytes!;
          return DecoratedBox(
            decoration: BoxDecoration(color: loadingBackgroundColor),
            child: Stack(
              fit: StackFit.expand,
              children: [
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(
                        height: 10,
                        width: isStandalone ? 140 : 110,
                        decoration: BoxDecoration(
                          color: widget.theme.iconColor.withValues(alpha: 0.10),
                          borderRadius: BorderRadius.circular(6),
                        ),
                      ),
                      const SizedBox(height: 8),
                      Container(
                        height: 10,
                        width: isStandalone ? 220 : 150,
                        decoration: BoxDecoration(
                          color: widget.theme.iconColor.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(6),
                        ),
                      ),
                      const Spacer(),
                      if (progressValue != null)
                        ClipRRect(
                          borderRadius: BorderRadius.circular(999),
                          child: LinearProgressIndicator(
                            minHeight: 3,
                            value: progressValue,
                            backgroundColor: widget.theme.iconColor.withValues(
                              alpha: 0.10,
                            ),
                            valueColor: AlwaysStoppedAnimation<Color>(
                              widget.theme.iconColor.withValues(alpha: 0.88),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
                Center(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: widget.theme.backgroundColor.withValues(
                        alpha: 0.35,
                      ),
                      borderRadius: BorderRadius.circular(999),
                      border: Border.all(
                        color: widget.theme.borderColor.withValues(alpha: 0.6),
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 6,
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          SizedBox(
                            height: 14,
                            width: 14,
                            child: CircularProgressIndicator(
                              strokeWidth: 2,
                              value: progressValue,
                              color: widget.theme.iconColor,
                            ),
                          ),
                          const SizedBox(width: 8),
                          Text(
                            'Loading image',
                            style: widget.theme.textStyle.copyWith(
                              fontSize:
                                  (widget.theme.textStyle.fontSize ?? 11) +
                                      0.25,
                              color: widget.theme.textStyle.color?.withValues(
                                alpha: 0.88,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ],
            ),
          );
        },
        errorBuilder: (context, error, stackTrace) {
          return DecoratedBox(
            decoration: BoxDecoration(color: errorBackgroundColor),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 240),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 10,
                  ),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.broken_image_outlined,
                        color: widget.theme.iconColor.withValues(alpha: 0.86),
                        size: 28,
                      ),
                      const SizedBox(height: 6),
                      Text(
                        'Image failed to load',
                        textAlign: TextAlign.center,
                        style: widget.theme.textStyle.copyWith(
                          fontWeight: FontWeight.w600,
                          color: widget.theme.textStyle.color?.withValues(
                            alpha: 0.9,
                          ),
                        ),
                      ),
                      const SizedBox(height: 8),
                      Material(
                        color: Colors.transparent,
                        child: InkWell(
                          key: kSovereignInlineImagePreviewRetryKey,
                          borderRadius: BorderRadius.circular(
                            (widget.theme.actionBorderRadius).clamp(6.0, 12.0),
                          ),
                          onTap: _retryInlineImagePreview,
                          child: Ink(
                            decoration: BoxDecoration(
                              color: widget.theme.actionBackgroundColor,
                              borderRadius: BorderRadius.circular(
                                (widget.theme.actionBorderRadius).clamp(
                                  6.0,
                                  12.0,
                                ),
                              ),
                              border: widget.theme.actionBorderColor == null
                                  ? null
                                  : Border.all(
                                      color: widget.theme.actionBorderColor!,
                                    ),
                            ),
                            child: Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 6,
                              ),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(
                                    Icons.refresh_rounded,
                                    size: 14,
                                    color: widget.theme.iconColor,
                                  ),
                                  const SizedBox(width: 4),
                                  Text('Retry', style: widget.theme.textStyle),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          );
        },
      );

      final imageBody = SizedBox(
        key: kSovereignInlineImagePreviewImageAreaKey,
        height: previewHeight,
        child: imageWidget,
      );
      if (!isStandalone || widget.onOpen == null) {
        return imageBody;
      }
      return Material(
        color: Colors.transparent,
        child: InkWell(onTap: widget.onOpen, child: imageBody),
      );
    }

    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: ConstrainedBox(
        constraints: BoxConstraints(
          minWidth: previewWidth,
          maxWidth: previewWidth,
          maxHeight: previewMaxHeight,
        ),
        child: DecoratedBox(
          key: kSovereignInlineImagePreviewKey,
          decoration: BoxDecoration(
            borderRadius: radius,
            border: Border.all(color: previewBorderColor),
            color: previewBackgroundColor,
          ),
          child: ClipRRect(
            borderRadius: radius,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                buildImageSurface(),
                if (altText.isNotEmpty || showUrl)
                  Container(
                    key: kSovereignInlineImagePreviewCaptionKey,
                    margin: const EdgeInsets.fromLTRB(8, 8, 8, 8),
                    padding: widget.theme.imageCaptionPadding,
                    decoration: BoxDecoration(
                      color: captionBackgroundColor,
                      borderRadius: BorderRadius.circular(
                        (widget.theme.actionBorderRadius + 1).clamp(8.0, 14.0),
                      ),
                      border: Border.all(color: captionBorderColor),
                    ),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        if (altText.isNotEmpty)
                          Text(
                            altText,
                            maxLines: isStandalone ? 2 : 1,
                            overflow: TextOverflow.ellipsis,
                            style: captionTextStyle,
                          ),
                        if (showUrl) ...[
                          if (altText.isNotEmpty) const SizedBox(height: 4),
                          Text(
                            imageUrl,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: imageUrlTextStyle,
                          ),
                        ],
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

bool _isPreviewableNetworkImageUrl(String url) {
  final uri = Uri.tryParse(url.trim());
  if (uri == null || !uri.hasScheme || uri.host.isEmpty) return false;
  return uri.scheme == 'http' || uri.scheme == 'https';
}

Future<void> copyInlineTargetUrlToClipboard(String? url) async {
  if (url == null || url.isEmpty) return;
  await Clipboard.setData(ClipboardData(text: url));
}
