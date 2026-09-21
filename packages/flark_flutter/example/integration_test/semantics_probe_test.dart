// Native semantics diagnostic, not input/performance qualification.
// Compare engine AXTree output for a standard TextField and Flark while each
// is repeatedly replaced with accessibility enabled. No user preferences.
import 'package:flark_dogfood/backend.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
  testWidgets(
    'native semantics replacement comparison',
    (t) async {
      final backend = await loadBackend();
      final handle = binding.ensureSemantics();
      const source =
          '# Document\n\nParagraph with **bold** and [link](https://dart.dev).\n\n';
      for (final host in ['TextField', 'Flark']) {
        debugPrint('SEMANTICS_PROBE_START $host');
        for (var cycle = 0; cycle < 50; cycle++) {
          final text = source * 32;
          final plain = TextEditingController(text: text);
          final flark = FlarkController(FlarkEditor(backend, text: text));
          await t.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: host == 'TextField'
                    ? TextField(
                        controller: plain,
                        autofocus: true,
                        maxLines: null,
                      )
                    : FlarkEditorWidget(
                        controller: flark,
                        autofocus: true,
                        showToolbar: false,
                      ),
              ),
            ),
          );
          await t.pump(const Duration(milliseconds: 20));
          for (var i = 0; i < 20; i++) {
            if (host == 'TextField') {
              plain.value = TextEditingValue(
                text: '${plain.text}x',
                selection: TextSelection.collapsed(
                  offset: plain.text.length + 1,
                ),
              );
            } else {
              flark.command(SetSelection.caret(flark.text.length));
              flark.command(const InsertText('x'));
            }
            await t.pump(const Duration(milliseconds: 5));
          }
          await t.pumpWidget(const SizedBox());
          await t.pump(const Duration(milliseconds: 20));
          plain.dispose();
          flark.dispose();
        }
        debugPrint('SEMANTICS_PROBE_END $host');
      }
      handle.dispose();
      await t.pump(const Duration(milliseconds: 100));
    },
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
