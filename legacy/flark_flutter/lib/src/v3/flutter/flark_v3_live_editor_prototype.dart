// Flutter presentation proof layered over the Dart-first document session.
import 'package:flark/flark_adapter.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'flark_v3_block_chrome.dart';
import 'flark_v3_flutter_live_controller.dart';
import 'flark_v3_list_item_editing.dart';

typedef FlarkV3PaintLayerBuilder =
    Widget Function(BuildContext context, FlarkV3FlutterPaintState state);

/// Resolves parser-certified heading typography from the editor base style.
///
/// [level] is always in the CommonMark range 1 through 6. The resolver changes
/// presentation only; source markers and heading classification remain owned
/// by the parser.
typedef FlarkV3HeadingTextStyleResolver =
    TextStyle Function(TextStyle baseStyle, int level);

/// Minimal parser-to-paint and real text-input integration gate for v3.
///
/// The editable subtree is deliberately stable across every host transition.
/// Its state opts Flutter's public [TextInput] connection into the granular
/// delta model and forwards those exact deltas to
/// [FlarkV3FlutterLiveController.applyTextEditingDeltas]. Full-value updates
/// used by autofill or a platform fallback are reduced to one bounded
/// replacement inside the sealed input island; they never diff the document.
final class FlarkV3LiveEditorPrototype extends StatefulWidget {
  const FlarkV3LiveEditorPrototype({
    super.key,
    required this.controller,
    required this.paintLayerBuilder,
    this.editableKey,
    this.focusNode,
    this.style = const TextStyle(fontSize: 14),
    this.fencedCodeStyle = const TextStyle(fontFamily: 'monospace'),
    this.blockQuoteStyle = const TextStyle(color: Color(0xFF4B5563)),
    this.blockQuoteRailColor = const Color(0xFFCBD5E1),
    this.blockQuoteRailWidth = 3,
    this.blockQuoteRailGap = 12,
    this.bulletListMarkerColor = const Color(0xFF64748B),
    this.headingStyleResolver = _defaultHeadingStyle,
    this.thematicBreakColor = const Color(0x4D000000),
    this.cursorColor = const Color(0xFF006ADC),
    this.backgroundCursorColor = const Color(0x00000000),
  }) : assert(blockQuoteRailWidth >= 0),
       assert(blockQuoteRailGap >= 0);

  final FlarkV3FlutterLiveController controller;
  final FlarkV3PaintLayerBuilder paintLayerBuilder;
  final Key? editableKey;
  final FocusNode? focusNode;
  final TextStyle style;

  /// Block-level style applied only while the parser-authorized input island
  /// is inside a fenced-code body.
  ///
  /// Inline syntax is not inspected in Flutter. The exact structural query
  /// selects this style, and callers may override it without changing source
  /// or projection behavior.
  final TextStyle fencedCodeStyle;

  /// Block-level style selected only for a parser-certified block quote.
  ///
  /// Inline emphasis/code inside the quote remains an independent
  /// parser-certified presentation layer; this style does not infer it.
  final TextStyle blockQuoteStyle;

  /// Paint and geometry for the quote rail surrounding the same EditableText.
  final Color blockQuoteRailColor;
  final double blockQuoteRailWidth;
  final double blockQuoteRailGap;

  /// Paint used only for a parser-certified tight list-item gutter.
  final Color bulletListMarkerColor;
  Color get listItemMarkerColor => bulletListMarkerColor;

  /// Block-level typography selected only from parser-authored heading facts.
  final FlarkV3HeadingTextStyleResolver headingStyleResolver;

  /// Paint used for a parser-certified atomic thematic break.
  final Color thematicBreakColor;

  final Color cursorColor;
  final Color backgroundCursorColor;

  @override
  State<FlarkV3LiveEditorPrototype> createState() =>
      _FlarkV3LiveEditorPrototypeState();
}

final class _FlarkV3LiveEditorPrototypeState
    extends State<FlarkV3LiveEditorPrototype> {
  FocusNode? _ownedFocusNode;

  FocusNode get _focusNode => widget.focusNode ?? _ownedFocusNode!;

  @override
  void initState() {
    super.initState();
    if (widget.focusNode == null) _ownedFocusNode = FocusNode();
    widget.controller.addListener(_handleControllerFrame);
  }

  @override
  void didUpdateWidget(FlarkV3LiveEditorPrototype oldWidget) {
    super.didUpdateWidget(oldWidget);
    final controllerChanged = !identical(
      oldWidget.controller,
      widget.controller,
    );
    if (controllerChanged) {
      oldWidget.controller.removeListener(_handleControllerFrame);
      widget.controller.addListener(_handleControllerFrame);
    }
    if (oldWidget.focusNode == null && widget.focusNode != null) {
      _ownedFocusNode?.dispose();
      _ownedFocusNode = null;
    } else if (oldWidget.focusNode != null && widget.focusNode == null) {
      _ownedFocusNode = FocusNode();
    }
  }

  void _handleControllerFrame() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleControllerFrame);
    _ownedFocusNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final paint = widget.controller.paintState;
    final allowSemantics =
        paint.accessibilitySemanticsValid &&
        widget.controller.semanticActionsValid;
    final allowHitTargets =
        paint.markdownHitTargetsValid && widget.controller.semanticActionsValid;

    final paintLayer = ExcludeSemantics(
      excluding: !allowSemantics,
      child: IgnorePointer(
        ignoring: !allowHitTargets,
        child: widget.paintLayerBuilder(context, paint),
      ),
    );
    final blockStyle = paint.blockStyleLease;
    final codeBody =
        blockStyle?.kind == FlarkV3FlutterBlockStyleKind.fencedCode ||
        blockStyle?.kind == FlarkV3FlutterBlockStyleKind.indentedCode;
    final quoteBody =
        blockStyle?.kind == FlarkV3FlutterBlockStyleKind.blockQuote;
    final listItemConfiguration = switch (blockStyle) {
      FlarkV3FlutterBlockStyleLease(
        kind: FlarkV3FlutterBlockStyleKind.tightListItem,
        :final tightListItemConfiguration?,
      ) =>
        tightListItemConfiguration,
      _ => null,
    };
    final headingLevel = switch (blockStyle) {
      FlarkV3FlutterBlockStyleLease(
        kind: FlarkV3FlutterBlockStyleKind.heading,
        :final headingLevel,
      ) =>
        headingLevel,
      _ => null,
    };
    final thematicBreak =
        paint.atomicBlockLease?.kind ==
        FlarkV3FlutterAtomicBlockKind.thematicBreak;
    final editableStyle = codeBody
        ? widget.style.merge(widget.fencedCodeStyle)
        : quoteBody
        ? widget.style.merge(widget.blockQuoteStyle)
        : headingLevel == null
        ? widget.style
        : widget.headingStyleResolver(widget.style, headingLevel);
    final editable = _FlarkV3DeltaEditableText(
      key: widget.editableKey,
      liveController: widget.controller,
      focusNode: _focusNode,
      style: editableStyle,
      cursorColor: widget.cursorColor,
      backgroundCursorColor: widget.backgroundCursorColor,
      selectionColor: widget.cursorColor.withValues(alpha: 0.25),
      keyboardType: TextInputType.multiline,
      textInputAction: TextInputAction.newline,
      maxLines: null,
    );

    final shortcutEditable = CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        if (thematicBreak) ...{
          const SingleActivator(LogicalKeyboardKey.backspace): () {
            widget.controller.deleteActiveAtomicBlock();
          },
          const SingleActivator(LogicalKeyboardKey.delete): () {
            widget.controller.deleteActiveAtomicBlock();
          },
        },
        if (listItemConfiguration != null &&
            widget.controller.canRemoveTightListItemPrefixAtCaret)
          const SingleActivator(LogicalKeyboardKey.backspace): () {
            widget.controller.removeTightListItemPrefixAtCaret();
          },
      },
      child: editable,
    );
    final codePresentedEditable = FlarkV3CodeBlockChrome(
      key: const Key('flark-v3-active-code-block-chrome'),
      active: codeBody,
      child: shortcutEditable,
    );
    final blockPresentedEditable = _FlarkV3BlockQuoteChrome(
      active: quoteBody,
      railColor: widget.blockQuoteRailColor,
      railWidth: widget.blockQuoteRailWidth,
      railGap: widget.blockQuoteRailGap,
      child: codePresentedEditable,
    );
    // Keep the EditableText at one stable element position while list
    // authority is demanded, superseded, or withheld for composition. The
    // gutter collapses to zero without configuration instead of reparenting
    // the platform text client between differently shaped trees.
    final legacyPresentedEditable = FlarkV3ListItemGutter(
      configuration: listItemConfiguration,
      markerColor: widget.listItemMarkerColor,
      markerTextStyle: widget.style,
      child: blockPresentedEditable,
    );
    // This outer wrapper is permanent, so installing or superseding a Green
    // row certificate cannot reparent the platform EditableText client.
    final presentedEditable = FlarkV3RecursiveGreenContainerChrome(
      path:
          widget.controller.recursiveGreenRow?.path ??
          const <FlarkV3RecursiveGreenRowPathFrame>[],
      textStyle: widget.style,
      markerColor: widget.listItemMarkerColor,
      quoteRailColor: widget.blockQuoteRailColor,
      child: legacyPresentedEditable,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        paintLayer,
        SizedBox(
          height: thematicBreak ? 24 : 0,
          child: thematicBreak
              ? ExcludeSemantics(
                  excluding: !allowSemantics,
                  child: Semantics(
                    label: 'Thematic break',
                    child: Center(
                      child: SizedBox(
                        key: const Key('flark-v3-thematic-break'),
                        width: double.infinity,
                        height: 1,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: widget.thematicBreakColor,
                          ),
                        ),
                      ),
                    ),
                  ),
                )
              : const SizedBox.shrink(),
        ),
        presentedEditable,
      ],
    );
  }
}

/// Keeps block-quote presentation out of the editable element topology.
///
/// Parser authority may be withheld for one frame during handoff or
/// composition. The stable [Stack] and [Padding] keep [child] in one element
/// position while the optional rail is added as a paint-only sibling.
final class _FlarkV3BlockQuoteChrome extends StatelessWidget {
  const _FlarkV3BlockQuoteChrome({
    required this.active,
    required this.railColor,
    required this.railWidth,
    required this.railGap,
    required this.child,
  });

  final bool active;
  final Color railColor;
  final double railWidth;
  final double railGap;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Padding(
          padding: EdgeInsets.only(left: active ? railGap : 0),
          child: child,
        ),
        if (active)
          Positioned.fill(
            child: ExcludeSemantics(
              child: IgnorePointer(
                child: Container(
                  key: const Key('flark-v3-block-quote-rail'),
                  decoration: BoxDecoration(
                    border: Border(
                      left: BorderSide(color: railColor, width: railWidth),
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

TextStyle _defaultHeadingStyle(TextStyle baseStyle, int level) {
  final scale = switch (level) {
    1 => 2.0,
    2 => 1.5,
    3 => 1.25,
    4 => 1.0,
    5 => 0.875,
    6 => 0.85,
    _ => throw RangeError.range(level, 1, 6, 'level'),
  };
  return baseStyle.copyWith(
    fontSize: (baseStyle.fontSize ?? 14) * scale,
    fontWeight: FontWeight.w700,
  );
}

/// An [EditableText] whose public state opts into Flutter's delta input model.
///
/// Subclassing keeps Flutter's selection, gesture, caret, semantics, autofill,
/// and platform lifecycle machinery intact. Only the text-input authority seam
/// changes: platform mutations reach the Flark source transaction before the
/// shared [TextEditingController] is updated.
final class _FlarkV3DeltaEditableText extends EditableText {
  _FlarkV3DeltaEditableText({
    super.key,
    required this.liveController,
    required super.focusNode,
    required super.style,
    required super.cursorColor,
    required super.backgroundCursorColor,
    required super.selectionColor,
    super.keyboardType = TextInputType.multiline,
    super.textInputAction = TextInputAction.newline,
    super.maxLines,
  }) : super(controller: liveController.editingController, readOnly: false);

  final FlarkV3FlutterLiveController liveController;

  @override
  EditableTextState createState() => _FlarkV3DeltaEditableTextState();
}

final class _FlarkV3DeltaEditableTextState extends EditableTextState
    with DeltaTextInputClient {
  FlarkV3FlutterLiveController get _liveController =>
      (widget as _FlarkV3DeltaEditableText).liveController;

  @override
  TextInputConfiguration get textInputConfiguration =>
      super.textInputConfiguration.copyWith(enableDeltaModel: true);

  @override
  void updateEditingValueWithDeltas(List<TextEditingDelta> textEditingDeltas) {
    _liveController.applyTextEditingDeltas(
      textEditingDeltas,
      editingValueAdopter: _adoptSourceValidatedPlatformValue,
    );
  }

  /// Autofill and platform fallbacks may still deliver a complete island
  /// value. The island is capped at 8 KiB, so reducing it to one splice is
  /// bounded independently of document size and does not classify Markdown.
  @override
  void updateEditingValue(TextEditingValue value) {
    _liveController.applyPlatformEditingValue(
      value,
      editingValueAdopter: _adoptSourceValidatedPlatformValue,
    );
  }

  void _adoptSourceValidatedPlatformValue(TextEditingValue value) {
    super.updateEditingValue(value);
  }
}
