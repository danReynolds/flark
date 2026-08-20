/// Flutter's custom live Markdown editing surface.
library;

// Re-export Core because public controller methods expose its viewport,
// selection, and semantic model types. A consumer of the supported Flutter
// barrel must not need a deep import merely to name those signatures.
export 'package:flark_core/flark_core.dart';

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
export 'src/render_surface.dart'
    show FlarkSurfacePaintObservation, FlarkSurfacePaintRowObservation;
export 'src/input_window.dart'
    show
        FlarkInputResyncReason,
        FlarkInputWindowShadow,
        FlarkInputWindowState,
        flarkWindowTextSha256;
