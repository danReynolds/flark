/// Flutter's custom live Markdown editing surface.
library;

// Re-export Core because public controller methods expose its viewport,
// selection, and semantic model types. A consumer of the supported Flutter
// barrel must not need a deep import merely to name those signatures.
export 'package:flark/flark.dart';

export 'src/controller.dart'
    show
        FlarkEditorController,
        FlarkSemanticEditPerformance,
        FlarkSourceEditPerformance,
        FlarkSourceEditPerformanceKind;
export 'src/editor.dart'
    show FlarkEditor, FlarkEditorDebugGeometry, FlarkEditorDebugHandle;
export 'src/markdown_view.dart' show FlarkMarkdownView;
export 'src/render_surface.dart'
    show
        FlarkSurfacePaintObservation,
        FlarkSurfacePaintRowObservation,
        FlarkSurfacePaintRunObservation;
export 'src/input_window.dart'
    show
        FlarkInputResyncReason,
        FlarkInputWindowShadow,
        FlarkInputWindowState,
        flarkWindowTextSha256;
