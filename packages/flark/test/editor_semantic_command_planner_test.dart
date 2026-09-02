import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  const planner = FlarkEditorSemanticCommandPlanner();

  group('semantic command planner', () {
    test('admits Return only from parser-authored row capability', () {
      final capable = _row(
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          insertParagraphBreak: true,
        ),
      );
      final literal = _row(
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          insertParagraphBreak: true,
          insertParagraphBreakAsLiteral: true,
        ),
      );

      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: capable,
          source: 'abc',
          caret: 1,
        )?.intent,
        FlarkCoreEditIntentV1.insertParagraphBreak,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: literal,
          source: 'abc',
          caret: 1,
        ),
        isNull,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: _row(),
          source: 'abc',
          caret: 1,
        ),
        isNull,
      );
    });

    test('physical source-line capability owns only an exact line start', () {
      final row = _row(
        sourceLength: 3,
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          insertParagraphBreakAtPhysicalLineStart: true,
        ),
      );

      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: row,
          source: 'a\nb',
          caret: 2,
        ),
        isNotNull,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: row,
          source: 'a\nb',
          caret: 1,
        ),
        isNull,
      );
    });

    test('Backspace uses projected segment and inline owner boundaries', () {
      final projected = _row(
        sourceLength: 5,
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          deleteBackwardAtProjectionStart: true,
        ),
        projectionSegments: const [
          FlarkProjectionSegment(
            sourceBytes: FlarkSourceRange(2, 3),
            sourceUtf16: FlarkSourceRange(2, 3),
          ),
        ],
      );
      final inline = _row(
        sourceLength: 5,
        inlineFacts: const [
          FlarkInlineFact(
            kind: FlarkInlineFactKind.strong,
            flags: 0x80,
            sourceBytes: FlarkSourceRange(0, 5),
            sourceUtf16: FlarkSourceRange(0, 5),
            contentBytes: FlarkSourceRange(2, 3),
            contentUtf16: FlarkSourceRange(2, 3),
          ),
        ],
      );

      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteBackward,
          row: projected,
          source: '**a**',
          caret: 2,
        )?.intent,
        FlarkCoreEditIntentV1.deleteBackward,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteBackward,
          row: inline,
          source: '**a**',
          caret: 3,
        )?.intent,
        FlarkCoreEditIntentV1.deleteBackward,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteBackward,
          row: inline,
          source: '**a**',
          caret: 4,
        ),
        isNull,
      );
    });

    test('Delete uses a current editable start or inline owner boundary', () {
      final forward = _row(
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          deleteForwardAtEditableStart: true,
        ),
      );
      final inline = _row(
        sourceLength: 5,
        inlineFacts: const [
          FlarkInlineFact(
            kind: FlarkInlineFactKind.strong,
            flags: 0x80,
            sourceBytes: FlarkSourceRange(0, 5),
            sourceUtf16: FlarkSourceRange(0, 5),
            contentBytes: FlarkSourceRange(2, 3),
            contentUtf16: FlarkSourceRange(2, 3),
          ),
        ],
      );

      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteForward,
          row: forward,
          source: 'abc',
          caret: 0,
        )?.intent,
        FlarkCoreEditIntentV1.deleteForward,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteForward,
          row: inline,
          source: '**a**',
          caret: 2,
        )?.intent,
        FlarkCoreEditIntentV1.deleteForward,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteForward,
          row: forward,
          source: 'abc',
          caret: 1,
        ),
        isNull,
      );
    });

    test('barriers and non-collapsed selections deny every command', () {
      final row = _row(
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          insertParagraphBreak: true,
          deleteBackwardAtEditableStart: true,
          deleteForwardAtEditableStart: true,
        ),
      );
      for (final command in FlarkEditorSemanticCommand.values) {
        expect(
          _plan(
            planner,
            command,
            row: row,
            source: 'abc',
            caret: 0,
            publicationBarrier: true,
          ),
          isNull,
        );
        expect(
          _plan(
            planner,
            command,
            row: row,
            source: 'abc',
            caret: 1,
            selection: const FlarkTextSelection(baseOffset: 0, extentOffset: 1),
          ),
          isNull,
        );
      }
    });

    test('denies contradictory host caret and row observations', () {
      final capable = _row(
        semanticCapabilities: const FlarkViewportRowSemanticCapabilities(
          insertParagraphBreak: true,
        ),
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: capable,
          source: 'abc',
          caret: 1,
          selection: const FlarkTextSelection.collapsed(offset: 2),
        ),
        isNull,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: capable,
          source: 'abc',
          caret: 1,
          activeOrdinal: 1,
        ),
        isNull,
      );
    });

    test('retains the semantic lane at an editor-owned neutral start', () {
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.deleteBackward,
          row: null,
          source: '',
          caret: 0,
          activeOrdinal: -1,
        )?.intent,
        FlarkCoreEditIntentV1.deleteBackward,
      );
      expect(
        _plan(
          planner,
          FlarkEditorSemanticCommand.insertParagraphBreak,
          row: null,
          source: '',
          caret: 0,
          activeOrdinal: -1,
        )?.intent,
        FlarkCoreEditIntentV1.insertParagraphBreak,
      );
    });
  });
}

FlarkEditorSemanticCommandAdmission? _plan(
  FlarkEditorSemanticCommandPlanner planner,
  FlarkEditorSemanticCommand command, {
  required FlarkViewportRow? row,
  required String source,
  required int caret,
  FlarkTextSelection? selection,
  int? activeOrdinal = 0,
  bool semanticEditActive = false,
  bool publicationBarrier = false,
}) {
  final inputSelection =
      selection ?? FlarkTextSelection.collapsed(offset: caret);
  final projector = FlarkSurfaceProjector(
    pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
    visibleUtf16Start: 0,
    visibleSource: source,
    inputGlobalUtf16Start: 0,
    inputValue: FlarkEditorInputValue(text: source, selection: inputSelection),
    activeOrdinal: activeOrdinal,
    selectionBaseUtf16: inputSelection.baseOffset,
    selectionExtentUtf16: inputSelection.extentOffset,
    crossRowSelection: !inputSelection.isCollapsed,
    semanticViewportCurrent: true,
    certificationRevisionCurrent: true,
    certificationRanges: const [],
    optimisticRanges: FlarkOptimisticRangeMap(),
  );
  return planner.plan(
    FlarkEditorSemanticCommandPlanningRequest(
      command: command,
      projector: projector,
      row: row,
      localCaretUtf16: caret,
      semanticEditActive: semanticEditActive,
      publicationCertificationBarrierActive: publicationBarrier,
    ),
  );
}

FlarkViewportRow _row({
  int sourceLength = 3,
  FlarkViewportRowSemanticCapabilities semanticCapabilities =
      FlarkViewportRowSemanticCapabilities.none,
  List<FlarkProjectionSegment>? projectionSegments,
  List<FlarkInlineFact> inlineFacts = const [],
}) => FlarkViewportRow(
  ordinal: 0,
  kind: 5,
  sourceBytes: FlarkSourceRange(0, sourceLength),
  sourceUtf16: FlarkSourceRange(0, sourceLength),
  editableBytes: FlarkSourceRange(0, sourceLength),
  editableUtf16: FlarkSourceRange(0, sourceLength),
  editCapability: FlarkViewportRowEditCapability.contiguous,
  semanticCapabilities: semanticCapabilities,
  headingLevel: null,
  headingStyle: null,
  listItem: null,
  blockQuote: null,
  codeBlock: null,
  thematicBreak: false,
  pathDepth: 0,
  inlineFacts: inlineFacts,
  projectionSegments: projectionSegments,
);
