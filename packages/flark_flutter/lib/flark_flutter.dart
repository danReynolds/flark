/// Flutter input, layout and paint over the synchronous Flark kernel.
library;

export 'package:flark/flark.dart';
export 'src/code_font.dart';
export 'src/controller.dart';
export 'src/editor.dart';
export 'src/image_previews.dart' show FlarkImageProvider;
export 'src/source_view.dart';
export 'src/surface.dart' show FlarkPaintObservation, FlarkImageObservation;

export 'src/theme.dart';
export 'src/resource_controls.dart'
    show
        FlarkResourceSession,
        FlarkResourcePresenter,
        FlarkLinkActions,
        FlarkLinkPopover,
        FlarkLinkPopoverBuilder;
