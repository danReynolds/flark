import 'dart:async';
import 'dart:ui' show AppExitResponse;
import 'package:flark_dogfood/main.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:shared_preferences_platform_interface/shared_preferences_platform_interface.dart';

class _DelayedStore extends InMemorySharedPreferencesStore {
  _DelayedStore() : super.empty();
  final gate = Completer<bool>();
  final writes = <String>[];
  @override
  Future<bool> setValue(String valueType, String key, Object value) async {
    if (key.endsWith('v5.source.Tour')) {
      writes.add(value as String);
      if (!await gate.future) return false;
    }
    return super.setValue(valueType, key, value);
  }
}

void main() {
  for (final succeeds in [true, false]) {
    testWidgets(
      'exit awaits the latest accepted draft and cancels on save failure: $succeeds',
      (tester) async {
        SharedPreferences.setMockInitialValues({});
        final store = _DelayedStore();
        SharedPreferencesStorePlatform.instance = store;
        final preferences = await SharedPreferences.getInstance();
        await tester.pumpWidget(
          DogfoodApp(backend: createParseBackend(), preferences: preferences),
        );
        await tester.pump();
        final c = tester
            .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
            .controller;
        expect(c.command(const InsertText('a')), isTrue);
        expect(c.command(const InsertText('b')), isTrue);
        final latest = c.text;
        c.command(SetSelection.caret(c.text.length));
        expect(
          store.writes,
          hasLength(1),
          reason:
              'selection does not enqueue another save and writes stay serial',
        );
        var finished = false;
        final exit = tester.binding.handleRequestAppExit().then((response) {
          finished = true;
          return response;
        });
        await tester.pump();
        expect(finished, isFalse);
        store.gate.complete(succeeds);
        await tester.pump();
        expect(
          await exit,
          succeeds ? AppExitResponse.exit : AppExitResponse.cancel,
        );
        if (succeeds) {
          expect((await store.getAll())['flutter.v5.source.Tour'], latest);
          expect(store.writes, hasLength(2));
        } else {
          expect(find.textContaining('Could not save locally'), findsOneWidget);
        }
        await tester.pumpWidget(const SizedBox());
        SharedPreferences.setMockInitialValues({});
      },
    );
  }
}
