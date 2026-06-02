import 'package:flutter/widgets.dart';

import '../core/core.dart';
import 'sovereign_flutter_controller.dart';

abstract interface class SovereignCommandInvocation {
  SovereignEditorRuntimeResult invoke(SovereignFlutterController controller);
}

final class SovereignTypedCommandInvocation<TPayload>
    implements SovereignCommandInvocation {
  const SovereignTypedCommandInvocation({
    required this.command,
    required this.payload,
  });

  final SovereignCommand<TPayload> command;
  final TPayload payload;

  @override
  SovereignEditorRuntimeResult invoke(SovereignFlutterController controller) {
    return controller.dispatch(command: command, payload: payload);
  }
}

final class SovereignCommandIntent extends Intent {
  const SovereignCommandIntent(this.invocation);

  final SovereignCommandInvocation invocation;
}

final class SovereignCommandAction extends Action<SovereignCommandIntent> {
  SovereignCommandAction({
    required this.controller,
  });

  final SovereignFlutterController controller;

  @override
  Object? invoke(SovereignCommandIntent intent) {
    return intent.invocation.invoke(controller);
  }
}

final class SovereignCommandActions extends StatelessWidget {
  const SovereignCommandActions({
    super.key,
    required this.controller,
    required this.child,
  });

  final SovereignFlutterController controller;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Actions(
      actions: {
        SovereignCommandIntent: SovereignCommandAction(
          controller: controller,
        ),
      },
      child: child,
    );
  }
}
