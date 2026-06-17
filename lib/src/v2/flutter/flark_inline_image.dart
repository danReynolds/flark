import 'package:flutter/widgets.dart';

import '../core/transaction/flark_source_range.dart';
import 'flark_markdown_theme.dart';

/// Everything an app needs to turn an inline image into a widget.
///
/// Passed to a [FlarkInlineImageBuilder] so an app can resolve [url] however it
/// likes — `Image.network`, `Image.file`, an asset, a cached/authenticated
/// provider — while honoring [constraints] so the result fits the editor line.
@immutable
final class FlarkInlineImageSpec {
  const FlarkInlineImageSpec({
    required this.url,
    required this.alt,
    required this.title,
    required this.sourceRange,
    required this.isStandalone,
    required this.constraints,
  });

  /// The image source (`![alt](url)`'s `url`).
  final String url;

  /// The alt text (`![alt](url)`'s `alt`), or null when absent.
  final String? alt;

  /// The optional title (`![alt](url "title")`'s `title`).
  final String? title;

  /// The full `![alt](url)` span in the source document.
  final FlarkSourceRange sourceRange;

  /// Whether the image is the only content on its line (renders block-style,
  /// content-width) versus sitting inline among other text (renders small).
  final bool isStandalone;

  /// The size bounds the rendered image should fit within.
  final BoxConstraints constraints;
}

/// Builds the widget shown for an inline image. Return any widget — typically an
/// [Image] — sized to fit `spec.constraints`. Provided via
/// `FlarkMarkdownInteractionConfig.imageBuilder`; when null, Flark renders
/// http(s) images with [Image.network] and falls back to a labelled card.
typedef FlarkInlineImageBuilder =
    Widget Function(BuildContext context, FlarkInlineImageSpec spec);

/// Renders an inline image inside the live editor and the read-only preview.
///
/// Delegates to an app-supplied [builder] when present; otherwise auto-loads
/// http(s) URLs via [Image.network] with a reserved loading placeholder and a
/// graceful fallback to a labelled card on error (or for non-network URLs).
final class FlarkInlineImage extends StatelessWidget {
  const FlarkInlineImage({
    super.key,
    required this.spec,
    this.builder,
  });

  final FlarkInlineImageSpec spec;
  final FlarkInlineImageBuilder? builder;

  static const double _inlineMaxHeight = 96;
  static const double _inlineMaxWidth = 280;
  static const double _standaloneMaxHeight = 360;
  static const double _standaloneMaxWidth = 560;

  /// The size bounds Flark applies for an image rendered in [context], given
  /// whether it is [isStandalone]. Exposed so a [FlarkInlineImageBuilder] can
  /// reuse the same bounds it receives on [FlarkInlineImageSpec.constraints].
  static BoxConstraints constraintsFor({required bool isStandalone}) {
    return isStandalone
        ? const BoxConstraints(
            maxWidth: _standaloneMaxWidth,
            maxHeight: _standaloneMaxHeight,
          )
        : const BoxConstraints(
            maxWidth: _inlineMaxWidth,
            maxHeight: _inlineMaxHeight,
          );
  }

  @override
  Widget build(BuildContext context) {
    final child = builder?.call(context, spec) ?? _defaultImage(context);
    return ConstrainedBox(constraints: spec.constraints, child: child);
  }

  Widget _defaultImage(BuildContext context) {
    final url = spec.url;
    final isNetwork = url.startsWith('http://') || url.startsWith('https://');
    if (!isNetwork) return _fallbackCard(context);
    return Image.network(
      url,
      fit: BoxFit.contain,
      loadingBuilder: (context, child, progress) {
        if (progress == null) return child;
        return _placeholder(context);
      },
      errorBuilder: (context, error, stackTrace) => _fallbackCard(context),
    );
  }

  Widget _placeholder(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    return _box(
      context,
      background: theme.cardBackgroundColor,
      border: theme.borderColor,
      child: Text(
        spec.alt?.isNotEmpty == true ? spec.alt! : 'Loading image…',
        overflow: TextOverflow.ellipsis,
        style: DefaultTextStyle.of(
          context,
        ).style.copyWith(color: theme.captionTextColor),
      ),
    );
  }

  Widget _fallbackCard(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final style = DefaultTextStyle.of(context).style;
    final label = spec.alt?.isNotEmpty == true ? spec.alt! : spec.url;
    return _box(
      context,
      background: theme.cardBackgroundColor,
      border: theme.borderColor,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _chip(context, 'IMG', theme, style),
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              label,
              overflow: TextOverflow.ellipsis,
              style: style.copyWith(color: theme.captionTextColor),
            ),
          ),
        ],
      ),
    );
  }

  Widget _chip(
    BuildContext context,
    String text,
    FlarkMarkdownThemeData theme,
    TextStyle style,
  ) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: theme.chipBackgroundColor,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 2),
        child: Text(
          text,
          style: style.copyWith(
            color: theme.chromeLabelColor,
            fontSize: (style.fontSize ?? 14) - 3,
            fontWeight: FontWeight.w700,
            height: 1,
          ),
        ),
      ),
    );
  }

  Widget _box(
    BuildContext context, {
    required Color background,
    required Color border,
    required Widget child,
  }) {
    return DecoratedBox(
      key: const Key('FlarkInlineImageFallback'),
      decoration: BoxDecoration(
        color: background,
        border: Border.all(color: border),
        borderRadius: const BorderRadius.all(Radius.circular(6)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: child,
      ),
    );
  }
}
