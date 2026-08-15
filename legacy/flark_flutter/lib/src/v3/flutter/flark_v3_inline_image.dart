import 'package:flark/flark_adapter.dart';
import 'package:flutter/widgets.dart';

/// App-facing paint data for one parser-certified v3 inline image.
///
/// [alt] is the marker-free projected label. [annotation] retains the
/// parser-cooked destination/title and exact canonical-source geometry. An
/// image can also sit inside [outerLink], but it never becomes a link merely
/// because Markdown image syntax also carries a destination.
@immutable
final class FlarkV3InlineImageSpec {
  const FlarkV3InlineImageSpec({
    required this.annotation,
    required this.alt,
    required this.outerLink,
    required this.constraints,
  });

  final FlarkV3InlineImageAnnotation annotation;
  final String alt;
  final FlarkV3InlineLinkAnnotation? outerLink;
  final BoxConstraints constraints;

  String get destination => annotation.destination;
  String? get title => annotation.title;
}

/// Builds the visual for one passive v3 inline image.
///
/// Flark supplies the semantics, link activation, and size bound around the
/// returned widget. The builder alone owns destination resolution; the
/// default presentation deliberately performs no network or file I/O.
typedef FlarkV3InlineImageBuilder =
    Widget Function(BuildContext context, FlarkV3InlineImageSpec spec);

/// Bounded passive visual for one parser-certified v3 inline image.
///
/// The default is a deterministic labelled chip rather than an implicit
/// network request. Applications can opt into any image provider through
/// [builder] without moving destination interpretation into Flutter.
final class FlarkV3InlineImage extends StatelessWidget {
  const FlarkV3InlineImage({
    super.key,
    required this.spec,
    this.builder,
    this.style = const TextStyle(fontSize: 14),
  });

  static const BoxConstraints inlineConstraints = BoxConstraints(
    maxWidth: 280,
    maxHeight: 96,
  );

  final FlarkV3InlineImageSpec spec;
  final FlarkV3InlineImageBuilder? builder;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final child = builder?.call(context, spec) ?? _fallback();
    return ConstrainedBox(constraints: spec.constraints, child: child);
  }

  Widget _fallback() {
    final label = spec.alt.isEmpty
        ? (spec.destination.isEmpty ? 'Image' : spec.destination)
        : spec.alt;
    return DecoratedBox(
      key: const ValueKey<String>('flark-v3-inline-image-fallback'),
      decoration: BoxDecoration(
        color: const Color(0x0D000000),
        border: Border.all(color: const Color(0x26000000)),
        borderRadius: const BorderRadius.all(Radius.circular(6)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            DecoratedBox(
              decoration: const BoxDecoration(
                color: Color(0x12000000),
                borderRadius: BorderRadius.all(Radius.circular(4)),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 2),
                child: Text(
                  'IMG',
                  style: style.copyWith(
                    fontSize: (style.fontSize ?? 14) - 3,
                    fontWeight: FontWeight.w700,
                    height: 1,
                  ),
                ),
              ),
            ),
            const SizedBox(width: 6),
            Flexible(
              child: Text(label, overflow: TextOverflow.ellipsis, style: style),
            ),
          ],
        ),
      ),
    );
  }
}
