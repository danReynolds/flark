import 'package:flark/flark_adapter.dart';

/// Whether [row] is the exact parser-owned atom selected by [query].
///
/// The first atomic product vertical is deliberately limited to a top-level
/// `Document -> ThematicBreak` path. Nested atoms remain fail-closed until the
/// renderer can compose their atomic presentation with container chrome.
bool flarkV3MatchesTopLevelThematicBreakAuthority(
  FlarkV3RecursiveGreenPointQuery query,
  FlarkV3RecursiveGreenRenderableRow? row,
) {
  if (row == null ||
      row.kind != FlarkV3RecursiveGreenKind.thematicBreak ||
      row.presentationKind !=
          FlarkV3RecursiveGreenRowPresentationKind.thematicBreak ||
      !row.selected ||
      row.inlineCapable ||
      !row.literal ||
      row.editCapability != FlarkV3RecursiveGreenRowEditCapability.contiguous ||
      row.editableSource == null ||
      query.ownerIndex != 1 ||
      query.ancestry.length != 2 ||
      row.path.length != 1 ||
      query.ancestry.first.kind != FlarkV3RecursiveGreenKind.document ||
      query.owner.frameId != row.frameId ||
      query.owner.kind != row.kind ||
      query.inlineFacts != null ||
      !_isCollapsedAtStart(
        row.editableSource!,
        row.presentationPhysicalSource,
      ) ||
      !_sourceSpanContainsSpan(row.presentationPhysicalSource, query.source)) {
    return false;
  }
  final rowOwner = row.path.single;
  return rowOwner.frameId == query.owner.frameId &&
      rowOwner.kind == query.owner.kind &&
      rowOwner.isRowOwner;
}

bool _isCollapsedAtStart(
  FlarkV3SourceSpan collapsed,
  FlarkV3SourceSpan physical,
) =>
    collapsed.startUtf8 == collapsed.endUtf8 &&
    collapsed.startUtf16 == collapsed.endUtf16 &&
    collapsed.startUtf8 == physical.startUtf8 &&
    collapsed.startUtf16 == physical.startUtf16;

bool _sourceSpanContainsSpan(
  FlarkV3SourceSpan outer,
  FlarkV3SourceSpan inner,
) =>
    inner.startUtf8 >= outer.startUtf8 &&
    inner.endUtf8 <= outer.endUtf8 &&
    inner.startUtf16 >= outer.startUtf16 &&
    inner.endUtf16 <= outer.endUtf16;
