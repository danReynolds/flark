/// Flutter's custom live Markdown editing surface.
library;

export 'package:flark_core/flark_core.dart'
    show
        FlarkSemanticTarget,
        FlarkSemanticTargetKind,
        FlarkSemanticTargetSyntax;

export 'src/controller.dart'
    show
        FlarkEditorController,
        FlarkEditorStatus,
        FlarkSemanticEditPerformance,
        FlarkSurfaceInlineStyle,
        FlarkSurfaceRow,
        FlarkSurfaceTextRun;
export 'src/editor.dart'
    show FlarkEditor, FlarkEditorDebugGeometry, FlarkEditorDebugHandle;
export 'src/markdown_view.dart' show FlarkMarkdownView;
export 'src/render_surface.dart' show FlarkSurfacePaintObservation;
export 'src/input_window.dart'
    show
        FlarkInputResyncReason,
        FlarkInputWindowShadow,
        FlarkInputWindowState,
        flarkWindowTextSha256;
