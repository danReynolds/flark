import 'package:flark/flark_adapter.dart';
import 'package:flutter/widgets.dart';

const EdgeInsets flarkV3CodeBlockPadding = EdgeInsets.symmetric(
  horizontal: 10,
  vertical: 8,
);

const Color flarkV3CodeBlockBackground = Color(0x0D000000);

/// Stable parser-selected code-block chrome shared by active and passive rows.
///
/// Keeping this wrapper in both states prevents a fenced-code activation from
/// changing the editable element's ancestry while still making its final
/// geometry byte-for-byte equivalent to the passive presentation.
final class FlarkV3CodeBlockChrome extends StatelessWidget {
  const FlarkV3CodeBlockChrome({
    super.key,
    required this.active,
    required this.child,
  });

  final bool active;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: active ? flarkV3CodeBlockBackground : null,
      ),
      child: Padding(
        padding: active ? flarkV3CodeBlockPadding : EdgeInsets.zero,
        child: child,
      ),
    );
  }
}

/// Stable composable chrome for one exact recursive-Green row ancestry.
///
/// List indentation, bullet gutters, and quote rails are painted as siblings
/// around one stable [child]. The widget consumes only typed parser facts; it
/// never reads or classifies Markdown source.
final class FlarkV3RecursiveGreenContainerChrome extends StatelessWidget {
  const FlarkV3RecursiveGreenContainerChrome({
    super.key,
    required this.path,
    required this.child,
    this.textStyle = const TextStyle(),
    this.markerColor = const Color(0xFF64748B),
    this.quoteRailColor = const Color(0xFFCBD5E1),
  });

  final List<FlarkV3RecursiveGreenRowPathFrame> path;
  final Widget child;
  final TextStyle textStyle;
  final Color markerColor;
  final Color quoteRailColor;

  @override
  Widget build(BuildContext context) {
    final atoms = <_FlarkV3GreenChromeAtom>[];
    FlarkV3RecursiveGreenListPathFact? currentList;
    for (final frame in path) {
      final fact = frame.fact;
      if (fact is FlarkV3RecursiveGreenListPathFact) {
        currentList = fact;
        continue;
      }
      if (fact is FlarkV3RecursiveGreenItemPathFact &&
          currentList?.style == FlarkV3RecursiveGreenListStyle.bullet) {
        atoms.add(_FlarkV3GreenChromeAtom.listItem(currentList));
        continue;
      }
      if (frame.kind == FlarkV3RecursiveGreenKind.blockQuote) {
        atoms.add(const _FlarkV3GreenChromeAtom.blockQuote());
      }
    }
    const atomWidth = 20.0;
    final totalInset = atoms.length * atomWidth;
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Padding(
          padding: EdgeInsets.only(left: totalInset),
          child: child,
        ),
        for (var index = 0; index < atoms.length; index += 1)
          if (atoms[index].quote)
            Positioned(
              left: index * atomWidth + 7,
              top: 0,
              bottom: 0,
              width: 3,
              child: ExcludeSemantics(
                child: IgnorePointer(
                  child: DecoratedBox(
                    key: ValueKey<Object>(('flark-v3-green-quote', index)),
                    decoration: BoxDecoration(color: quoteRailColor),
                  ),
                ),
              ),
            )
          else if (atoms[index].list case final list?)
            Positioned(
              left: index * atomWidth,
              top: 0,
              width: atomWidth,
              child: ExcludeSemantics(
                child: IgnorePointer(
                  child: Text(
                    key: ValueKey<Object>(('flark-v3-green-list', index)),
                    _greenListMarker(list),
                    textAlign: TextAlign.center,
                    style: textStyle.copyWith(color: markerColor),
                  ),
                ),
              ),
            ),
      ],
    );
  }
}

final class _FlarkV3GreenChromeAtom {
  const _FlarkV3GreenChromeAtom.blockQuote() : quote = true, list = null;

  const _FlarkV3GreenChromeAtom.listItem(this.list) : quote = false;

  final bool quote;
  final FlarkV3RecursiveGreenListPathFact? list;
}

String _greenListMarker(FlarkV3RecursiveGreenListPathFact list) =>
    switch (list.style) {
      FlarkV3RecursiveGreenListStyle.bullet => '\u2022',
      // This list fact does not identify each item's exact authored ordinal.
      // Ordered chrome stays fail-closed until the row contract names it.
      FlarkV3RecursiveGreenListStyle.ordered => '',
    };
