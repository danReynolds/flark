import 'package:flutter/widgets.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_input_engine.dart';
import 'flark_flutter_controller.dart';

abstract interface class FlarkCommandInvocation {
  FlarkEditorRuntimeResult invoke(FlarkFlutterController controller);
}

final class FlarkTypedCommandInvocation<TPayload>
    implements FlarkCommandInvocation {
  const FlarkTypedCommandInvocation({
    required this.command,
    required this.payload,
  });

  final FlarkCommand<TPayload> command;
  final TPayload payload;

  @override
  FlarkEditorRuntimeResult invoke(FlarkFlutterController controller) {
    return controller.dispatch(command: command, payload: payload);
  }
}

final class FlarkCommandIntent extends Intent {
  const FlarkCommandIntent(this.invocation);

  final FlarkCommandInvocation invocation;
}

final class FlarkCommandAction extends Action<FlarkCommandIntent> {
  FlarkCommandAction({required this.controller});

  final FlarkFlutterController controller;

  @override
  Object? invoke(FlarkCommandIntent intent) {
    return intent.invocation.invoke(controller);
  }
}

/// Intent that indents (or, with [outdent], outdents) the list item under the
/// caret by one level.
final class FlarkIndentListIntent extends Intent {
  const FlarkIndentListIntent({this.outdent = false});

  final bool outdent;
}

/// Action for [FlarkIndentListIntent].
///
/// [isEnabled] only reports true when the caret is inside a list item that can
/// actually be re-indented, so binding Tab to this intent leaves Tab in
/// ordinary text free to traverse focus or insert as the platform expects.
final class FlarkIndentListAction extends Action<FlarkIndentListIntent> {
  FlarkIndentListAction({required this.controller});

  final FlarkFlutterController controller;

  @override
  bool isEnabled(FlarkIndentListIntent intent) {
    final edit = intent.outdent
        ? FlarkMarkdownInputEngine.outdent(
            markdown: controller.markdown,
            selection: controller.selection,
          )
        : FlarkMarkdownInputEngine.indent(
            markdown: controller.markdown,
            selection: controller.selection,
          );
    return edit != null;
  }

  @override
  Object? invoke(FlarkIndentListIntent intent) {
    return controller.dispatch(
      command: FlarkMarkdownInputCommands.handleTab,
      payload: FlarkHandleTabPayload(outdent: intent.outdent),
    );
  }
}

/// Intent that moves the line(s) under the selection up or (with [down]) down.
final class FlarkMoveLinesIntent extends Intent {
  const FlarkMoveLinesIntent({this.down = false});

  final bool down;
}

/// Action for [FlarkMoveLinesIntent], gated so the shortcut falls through to
/// normal caret movement at the document boundary (nothing to move).
final class FlarkMoveLinesAction extends Action<FlarkMoveLinesIntent> {
  FlarkMoveLinesAction({required this.controller});

  final FlarkFlutterController controller;

  @override
  bool isEnabled(FlarkMoveLinesIntent intent) {
    return FlarkMarkdownInputEngine.moveLines(
          markdown: controller.markdown,
          selection: controller.selection,
          down: intent.down,
        ) !=
        null;
  }

  @override
  Object? invoke(FlarkMoveLinesIntent intent) {
    return controller.dispatch(
      command: FlarkMarkdownInputCommands.moveLines,
      payload: FlarkMoveLinesPayload(down: intent.down),
    );
  }
}

final class FlarkCommandActions extends StatelessWidget {
  const FlarkCommandActions({
    super.key,
    required this.controller,
    required this.child,
  });

  final FlarkFlutterController controller;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Actions(
      actions: {
        FlarkCommandIntent: FlarkCommandAction(controller: controller),
        FlarkIndentListIntent: FlarkIndentListAction(controller: controller),
        FlarkMoveLinesIntent: FlarkMoveLinesAction(controller: controller),
      },
      child: child,
    );
  }
}
