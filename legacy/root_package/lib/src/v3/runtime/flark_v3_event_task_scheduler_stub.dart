import 'dart:async';

void scheduleEventTask(void Function() callback) {
  Timer.run(callback);
}
