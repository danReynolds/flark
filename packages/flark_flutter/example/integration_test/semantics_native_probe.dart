// Attended native diagnostic. Run with flutter run --profile -d macos
// --target=integration_test/semantics_native_probe.dart, inspect the native AX
// tree, and activate each Start control. Uses the normal application binding;
// the widget-test binding's forced semantics and leak accounting are unsuitable
// for a platform-owned semantics handle first activated during a test.
import 'dart:convert';
import 'package:flark_dogfood/backend.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(_Probe(await loadBackend()));
}

class _Probe extends StatefulWidget {
  const _Probe(this.backend);
  final FlarkParseBackend backend;
  @override
  State<_Probe> createState() => _ProbeState();
}

class _ProbeState extends State<_Probe> {
  static const hosts = ['TextField', 'Flark'];
  static const source =
      '# Document\n\nParagraph with **bold** and [link](https://dart.dev).\n\n';
  final results = <Map<String, Object>>[];
  final transitions = <String>[];
  late final AppLifecycleListener lifecycle;
  TextEditingController? plain;
  FlarkController? flark;
  bool running = false;
  String? failure;
  int generation = 0;

  @override
  void initState() {
    super.initState();
    lifecycle = AppLifecycleListener(
      onStateChange: (state) {
        if (running) transitions.add(state.name);
      },
    );
    debugPrint('NATIVE_AX_READY TextField');
  }

  void requireNative() {
    if (!mounted) throw StateError('Probe was unmounted');
    final binding = WidgetsBinding.instance;
    if (binding.lifecycleState != AppLifecycleState.resumed ||
        !binding.framesEnabled ||
        !binding.platformDispatcher.semanticsEnabled ||
        transitions.any((state) => state != 'resumed')) {
      throw StateError(
        'Native accessibility and uninterrupted foreground required',
      );
    }
  }

  Future<void> nextFrame() async {
    await WidgetsBinding.instance.endOfFrame.timeout(
      const Duration(seconds: 5),
    );
    requireNative();
  }

  Future<void> runHost() async {
    if (running || failure != null || results.length == hosts.length) return;
    final host = hosts[results.length];
    transitions.clear();
    var cycles = 0, edits = 0;
    setState(() => running = true);
    debugPrint('NATIVE_AX_START $host');
    try {
      requireNative();
      for (var cycle = 0; cycle < 50; cycle++) {
        var expected = source * 32;
        setState(() {
          generation++;
          if (host == 'TextField') {
            plain = TextEditingController(text: expected);
          } else {
            flark = FlarkController(
              FlarkEditor(widget.backend, text: expected),
            );
          }
        });
        await nextFrame();
        for (var edit = 0; edit < 20; edit++) {
          expected += 'x';
          if (plain case final controller?) {
            controller.value = TextEditingValue(
              text: expected,
              selection: TextSelection.collapsed(offset: expected.length),
            );
          } else {
            flark!.command(SetSelection.caret(flark!.text.length));
            if (!flark!.command(const InsertText('x'))) {
              throw StateError('Flark declined insertion');
            }
          }
          await nextFrame();
          final actual = plain?.text ?? flark!.text;
          final caret =
              plain?.selection.extentOffset ?? flark!.editor.selection.extent;
          if (actual != expected || caret != expected.length) {
            throw StateError(
              '$host source or caret drift at cycle $cycle edit $edit',
            );
          }
          edits++;
        }
        final oldPlain = plain, oldFlark = flark;
        setState(() {
          plain = null;
          flark = null;
        });
        try {
          await nextFrame();
        } finally {
          oldPlain?.dispose();
          oldFlark?.dispose();
        }
        cycles++;
      }
    } catch (error) {
      failure = error.toString();
    }
    if (!mounted) return;
    final result = <String, Object>{
      'host': host,
      'cycles': cycles,
      'edits': edits,
      'nativeSemanticsEnabled':
          WidgetsBinding.instance.platformDispatcher.semanticsEnabled,
      'lifecycle': WidgetsBinding.instance.lifecycleState?.name ?? 'unknown',
      'transitions': List<String>.of(transitions),
      'failure': ?failure,
    };
    debugPrint('NATIVE_AX_RESULT ${jsonEncode(result)}');
    setState(() {
      results.add(result);
      running = false;
    });
    if (failure == null && results.length < hosts.length) {
      debugPrint('NATIVE_AX_READY ${hosts[results.length]}');
    }
  }

  @override
  void dispose() {
    lifecycle.dispose();
    plain?.dispose();
    flark?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    home: Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          children: [
            Text(
              failure ??
                  (running
                      ? 'Running ${hosts[results.length]} accessibility probe'
                      : results.length == hosts.length
                      ? 'Native accessibility comparison complete'
                      : 'Inspect native accessibility, then start ${hosts[results.length]}'),
            ),
            if (!running && failure == null && results.length < hosts.length)
              ElevatedButton(
                onPressed: runHost,
                child: Text(
                  'Start ${hosts[results.length]} accessibility probe',
                ),
              ),
            Expanded(
              child: plain != null
                  ? TextField(
                      key: ValueKey(generation),
                      controller: plain,
                      autofocus: true,
                      maxLines: null,
                    )
                  : flark != null
                  ? FlarkEditorWidget(
                      key: ValueKey(generation),
                      controller: flark!,
                      autofocus: true,
                      showToolbar: false,
                    )
                  : const SizedBox(),
            ),
          ],
        ),
      ),
    ),
  );
}
