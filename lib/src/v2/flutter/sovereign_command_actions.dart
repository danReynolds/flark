import 'package:flutter/widgets.dart';

import '../core/core.dart';
import 'sovereign_flutter_controller.dart';

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
      actions: {FlarkCommandIntent: FlarkCommandAction(controller: controller)},
      child: child,
    );
  }
}
