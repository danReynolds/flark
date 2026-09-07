import 'package:flark/flark.dart';
import 'package:flark_dogfood/main.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  testWidgets('the workbench opens a real parser-backed editing surface', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    await tester.pumpWidget(
      DogfoodApp(backend: createParseBackend(), preferences: prefs),
    );
    await tester.pump();
    expect(find.text('flark'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
