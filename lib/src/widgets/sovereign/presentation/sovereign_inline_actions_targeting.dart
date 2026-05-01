import 'package:flutter/material.dart';
import 'package:sovereign_editor/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/logic/sovereign_style_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

enum SovereignInlineActionsTargetKind { link, image }

@immutable
class SovereignInlineActionsTarget {
  const SovereignInlineActionsTarget({
    required this.kind,
    required this.linkKind,
    required this.fullStart,
    required this.fullEnd,
    required this.displayStart,
    required this.displayEnd,
    required this.urlStart,
    required this.urlEnd,
  });

  factory SovereignInlineActionsTarget.fromLink(SovereignLinkMatch link) {
    return SovereignInlineActionsTarget(
      kind: SovereignInlineActionsTargetKind.link,
      linkKind: link.kind,
      fullStart: link.fullStart,
      fullEnd: link.fullEnd,
      displayStart: link.displayStart,
      displayEnd: link.displayEnd,
      urlStart: link.urlStart,
      urlEnd: link.urlEnd,
    );
  }

  factory SovereignInlineActionsTarget.fromImage(SovereignImageMatch image) {
    return SovereignInlineActionsTarget(
      kind: SovereignInlineActionsTargetKind.image,
      linkKind: null,
      fullStart: image.fullStart,
      fullEnd: image.fullEnd,
      displayStart: image.altStart,
      displayEnd: image.altEnd,
      urlStart: image.urlStart,
      urlEnd: image.urlEnd,
    );
  }

  final SovereignInlineActionsTargetKind kind;
  final SovereignLinkMatchKind? linkKind;
  final int fullStart;
  final int fullEnd;
  final int displayStart;
  final int displayEnd;
  final int urlStart;
  final int urlEnd;

  bool get isImage => kind == SovereignInlineActionsTargetKind.image;

  String labelText(String text) => text.substring(displayStart, displayEnd);
  String urlText(String text) => text.substring(urlStart, urlEnd);
}

@immutable
class SovereignInlineActionsOverlayPlacement {
  const SovereignInlineActionsOverlayPlacement({
    required this.left,
    required this.top,
    required this.maxWidth,
  });

  final double left;
  final double top;
  final double maxWidth;
}

@immutable
class SovereignResolvedInlineActionsTarget {
  const SovereignResolvedInlineActionsTarget({
    required this.target,
    required this.textSnapshot,
    required this.resolvedUrl,
  });

  final SovereignInlineActionsTarget target;
  final String textSnapshot;
  final String resolvedUrl;
}

SovereignResolvedInlineActionsTarget?
    resolveSovereignInlineActionsTargetAtCaret(String text, int caret) {
  if (caret < 0 || caret > text.length) return null;
  final image = SovereignStyleScanner.imageAtCaret(text, caret);
  if (image != null) {
    final target = SovereignInlineActionsTarget.fromImage(image);
    return SovereignResolvedInlineActionsTarget(
      target: target,
      textSnapshot: text,
      resolvedUrl: target.urlText(text).trim(),
    );
  }
  final link = SovereignStyleScanner.linkAtCaret(text, caret);
  if (link == null) return null;
  final target = SovereignInlineActionsTarget.fromLink(link);
  final resolved = link.kind == SovereignLinkMatchKind.reference
      ? (SovereignStyleScanner.resolveReferenceLinkUrl(text, link) ?? '')
      : link.urlText(text);
  return SovereignResolvedInlineActionsTarget(
    target: target,
    textSnapshot: text,
    resolvedUrl: resolved.trim(),
  );
}

SovereignLinkMatch? sovereignInlineTargetAsLinkMatch(
  SovereignInlineActionsTarget target,
) {
  final kind = target.linkKind;
  if (target.isImage || kind == null) return null;
  final isReference = kind == SovereignLinkMatchKind.reference;
  return SovereignLinkMatch(
    kind: kind,
    fullStart: target.fullStart,
    fullEnd: target.fullEnd,
    displayStart: target.displayStart,
    displayEnd: target.displayEnd,
    urlStart: target.urlStart,
    urlEnd: target.urlEnd,
    referenceLabelStart: isReference ? target.urlStart : null,
    referenceLabelEnd: isReference ? target.urlEnd : null,
  );
}

bool isStandaloneInlineImageTarget(
  String text,
  SovereignInlineActionsTarget target,
) {
  if (!target.isImage) return false;
  if (target.fullStart < 0 || target.fullEnd > text.length) return false;
  final lineStart = (() {
    if (target.fullStart <= 0) return 0;
    final prevNewline = text.lastIndexOf('\n', target.fullStart - 1);
    return prevNewline == -1 ? 0 : prevNewline + 1;
  })();
  final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
  final lineEnd = (lineEndWithBreak > lineStart &&
          text.codeUnitAt(lineEndWithBreak - 1) == 10)
      ? lineEndWithBreak - 1
      : lineEndWithBreak;
  if (lineEnd <= lineStart) return false;
  final line = text.substring(lineStart, lineEnd).trim();
  if (line.isEmpty) return false;
  return line == text.substring(target.fullStart, target.fullEnd);
}

SovereignInlineActionsOverlayPlacement computeSovereignInlineActionsPlacement({
  required SovereignLinkActionsTheme theme,
  required Rect hostRect,
  required double anchorX,
  required double anchorTop,
  required double anchorBottom,
  required double estimatedWidth,
  required double estimatedHeight,
  required double maxWidthCap,
  required bool preferAbove,
}) {
  final maxWidth = (hostRect.width - theme.overlayViewportHorizontalPadding)
      .clamp(theme.overlayMinWidth, maxWidthCap);

  final minLeft = hostRect.left + theme.editorEdgePadding;
  final maxLeft = (hostRect.right - estimatedWidth - theme.editorEdgePadding)
      .clamp(minLeft, double.infinity);
  final left = anchorX.clamp(minLeft, maxLeft);

  final aboveTop = anchorTop - theme.overlayAboveCaretOffset;
  final belowTop = anchorBottom + theme.overlayBelowCaretOffset;
  final minTop = hostRect.top + theme.overlayVerticalPadding;
  final maxTop =
      (hostRect.bottom - estimatedHeight - theme.overlayVerticalPadding).clamp(
    minTop,
    double.infinity,
  );
  final preferredTop = preferAbove
      ? (aboveTop >= minTop ? aboveTop : belowTop)
      : (belowTop + estimatedHeight <=
              hostRect.bottom - theme.overlayVerticalPadding
          ? belowTop
          : aboveTop);
  final top = preferredTop.clamp(minTop, maxTop);

  return SovereignInlineActionsOverlayPlacement(
    left: left.toDouble(),
    top: top.toDouble(),
    maxWidth: maxWidth.toDouble(),
  );
}
