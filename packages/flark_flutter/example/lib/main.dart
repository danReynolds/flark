import 'package:url_launcher/url_launcher.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:ui' show AppExitResponse;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/code.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'backend.dart';
import 'qualification.dart';
import 'theme_playground.dart';

const tour = '''# A place to think

Flark is a live Markdown notebook. Write naturally, and your Markdown stays with you.

## Try the editing loop

Move through **bold**, *emphasis*, ~~strikethrough~~ and `inline code`. Delete a styled word, then keep typing. Use ⌘B or ⌘I to change formatting.

- Return continues a list
- Return again on an empty item leaves it
- [ ] A task to finish
- [x] A task completed

> A quote can hold **formatted words**.
> Return continues the quote.

## A small table

| Idea | State |
| --- | --- |
| Clear source | **Always** |
| Fast feedback | In progress |

```dart
final thought = 'Keep it simple';
```

---

[Markdown reference](https://commonmark.org/help/)

Your edits are saved locally. The Source button opens exact Markdown for edits that need it.
''';

String dense(int bytes) {
  final b = StringBuffer();
  for (var i = 0; b.length < bytes; i++) {
    b.write(
      '## Section $i\n\nSome **strong words** with *emphasis*, `code`, and a [link](https://example.com). A paragraph to write in.\n\n- first item\n- [x] another item\n\n> a short quote\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n',
    );
  }
  return b.toString().substring(0, bytes);
}

final presets = <String, String>{
  'Draft': '',
  'Tour': tour,
  'Dense 16 KiB': dense(16 * 1024),
  'Dense 32 KiB': dense(32 * 1024),
  'Long line': 'word ' * 1000,
};

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    runApp(
      DogfoodApp(
        backend: await loadBackend(),
        code: await FlarkTreeSitter.load(),
        preferences: await SharedPreferences.getInstance(),
      ),
    );
  } catch (error) {
    runApp(
      MaterialApp(
        home: Scaffold(
          body: Center(child: SelectableText('Flark could not start: $error')),
        ),
      ),
    );
  }
}

class DogfoodApp extends StatelessWidget {
  const DogfoodApp({
    super.key,
    required this.backend,
    required this.preferences,
    this.code,
    this.onPaint,
  });
  final FlarkParseBackend backend;
  final FlarkTreeSitter? code;
  final SharedPreferences preferences;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    title: 'Flark — Live Markdown',
    initialRoute: Uri.base.queryParameters['theme'] == '1' ? '/theme' : '/',
    routes: {
      '/theme': (_) => ThemePlayground(backend: backend, code: code),
    },
    theme: ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff3a628f)),
      useMaterial3: true,
      scaffoldBackgroundColor: const Color(0xffedf0f5),
    ),
    home: _DesktopMenus(
      child: Workbench(
        backend: backend,
        code: code,
        preferences: preferences,
        onPaint: onPaint,
      ),
    ),
  );
}

class Workbench extends StatefulWidget {
  const Workbench({
    super.key,
    required this.backend,
    required this.preferences,
    this.code,
    this.onPaint,
  });
  final FlarkParseBackend backend;
  final FlarkTreeSitter? code;
  final SharedPreferences preferences;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  @override
  State<Workbench> createState() => _WorkbenchState();
}

class _WorkbenchState extends State<Workbench> {
  late FlarkController c;
  String active = 'Tour';
  final Map<String, String> _pendingSaves = {};
  bool _saving = false, inspect = false;
  String? _saveError;
  String? _lastQueuedSource;
  Completer<void>? _saveCompletion;
  late final AppLifecycleListener _lifecycle;
  @override
  void initState() {
    super.initState();
    active = widget.preferences.getString('v5.active') ?? 'Tour';
    _load(active);
    _lifecycle = AppLifecycleListener(
      onExitRequested: () async {
        c.finishComposition();
        await _save();
        return _pendingSaves.isEmpty
            ? AppExitResponse.exit
            : AppExitResponse.cancel;
      },
    );
  }

  void _load(String key) {
    final source =
        _pendingSaves[key] ??
        widget.preferences.getString('v5.source.$key') ??
        presets[key] ??
        '';
    final editor = FlarkEditor(
      widget.backend,
      codeEditing: widget.code,
      text: source,
      caret: 0,
      syncLimit: candidateLiveBytes,
      liveLimits: candidateLiveLimits,
      sourceLimit: candidateSourceBytes,
    );
    c = FlarkController(
      editor,
      codeColors: widget.code == null ? null : FlarkCodeColors(editor),
    );
    _lastQueuedSource = source;
    c.addListener(_changed);
  }

  void _changed() {
    if (c.text != _lastQueuedSource) {
      _lastQueuedSource = c.text;
      _pendingSaves[active] = c.text;
      unawaited(_save());
    }
    if (mounted) setState(() {});
  }

  Future<void> _save() async {
    if (_saving) return _saveCompletion!.future;
    _saving = true;
    _saveCompletion = Completer<void>();
    try {
      while (_pendingSaves.isNotEmpty) {
        final entry = _pendingSaves.entries.first;
        final success = await widget.preferences.setString(
          'v5.source.${entry.key}',
          entry.value,
        );
        if (!success) throw StateError('local storage rejected the write');
        if (_pendingSaves[entry.key] == entry.value) {
          _pendingSaves.remove(entry.key);
        }
      }
      _saveError = null;
    } catch (_) {
      _saveError = 'Could not save locally. Copy your Markdown before closing.';
    }
    _saving = false;
    _saveCompletion!.complete();
    _saveCompletion = null;
    if (mounted) setState(() {});
  }

  void _switch(String key) {
    c.finishComposition();
    c.removeListener(_changed);
    final old = c;
    setState(() {
      active = key;
      _load(key);
    });
    unawaited(widget.preferences.setString('v5.active', key));
    WidgetsBinding.instance.addPostFrameCallback((_) => old.dispose());
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    c.removeListener(_changed);
    c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    body: SafeArea(
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 16),
            child: Row(
              children: [
                const Icon(Icons.edit_note, size: 30),
                const SizedBox(width: 8),
                if (MediaQuery.sizeOf(context).width >= 500)
                  const Text(
                    'flark',
                    style: TextStyle(
                      fontSize: 25,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -.8,
                    ),
                  ),
                const SizedBox(width: 12),
                Expanded(
                  child: DropdownButton<String>(
                    isExpanded: true,
                    value: active,
                    underline: const SizedBox(),
                    items: presets.keys
                        .map(
                          (key) =>
                              DropdownMenuItem(value: key, child: Text(key)),
                        )
                        .toList(),
                    onChanged: (value) {
                      if (value != null) _switch(value);
                    },
                  ),
                ),
                const SizedBox(width: 12),
                IconButton(
                  tooltip: 'Theme playground',
                  onPressed: () => Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) => ThemePlayground(
                        backend: widget.backend,
                        code: widget.code,
                      ),
                    ),
                  ),
                  icon: const Icon(Icons.palette_outlined),
                ),
                IconButton(
                  tooltip: 'Inspect Markdown',
                  onPressed: () => setState(() => inspect = !inspect),
                  icon: Icon(inspect ? Icons.code_off : Icons.code),
                ),
                IconButton(
                  tooltip: 'Copy Markdown',
                  onPressed: () =>
                      Clipboard.setData(ClipboardData(text: c.text)),
                  icon: const Icon(Icons.copy_outlined, size: 20),
                ),
              ],
            ),
          ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: ColoredBox(
                  color: Colors.white,
                  child: Row(
                    children: [
                      Expanded(
                        child: FlarkEditorWidget(
                          key: ValueKey(active),
                          controller: c,
                          autofocus: true,
                          baseUri: kIsWeb ? Uri.base : null,
                          onOpenLink: (uri) async {
                            try {
                              if (await launchUrl(
                                uri,
                                mode: LaunchMode.externalApplication,
                              )) {
                                return;
                              }
                            } catch (_) {}
                            if (context.mounted) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text('Could not open link.'),
                                ),
                              );
                            }
                          },
                          onPaint: widget.onPaint,
                        ),
                      ),
                      if (inspect &&
                          MediaQuery.sizeOf(context).width > 650) ...[
                        const VerticalDivider(width: 1),
                        Expanded(
                          child: Padding(
                            padding: const EdgeInsets.all(20),
                            child: FlarkSourceView(controller: c),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 0, 24, 12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    _saveError ?? (_saving ? 'Saving…' : 'Saved locally'),
                    style: TextStyle(
                      fontSize: 12,
                      color: _saveError == null
                          ? const Color(0xff68778e)
                          : Colors.red,
                    ),
                  ),
                ),
                const SizedBox(width: 16),
                Text(
                  '${utf8.encode(c.text).length} bytes · ${c.editor.sourceMode ? 'Source' : 'Rendered'}',
                  style: const TextStyle(
                    fontSize: 12,
                    color: Color(0xff68778e),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    ),
  );
}

class _DesktopMenus extends StatelessWidget {
  const _DesktopMenus({required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.macOS) return child;
    return PlatformMenuBar(
      menus: const [
        PlatformMenu(
          label: 'Flark',
          menus: [
            PlatformProvidedMenuItem(type: PlatformProvidedMenuItemType.about),
            PlatformProvidedMenuItem(type: PlatformProvidedMenuItemType.quit),
          ],
        ),
        PlatformMenu(
          label: 'Edit',
          menus: [
            PlatformMenuItem(
              label: 'Undo',
              shortcut: SingleActivator(LogicalKeyboardKey.keyZ, meta: true),
              onSelectedIntent: UndoTextIntent(SelectionChangedCause.keyboard),
            ),
            PlatformMenuItem(
              label: 'Redo',
              shortcut: SingleActivator(
                LogicalKeyboardKey.keyZ,
                meta: true,
                shift: true,
              ),
              onSelectedIntent: RedoTextIntent(SelectionChangedCause.keyboard),
            ),
            PlatformMenuItem(
              label: 'Cut',
              shortcut: SingleActivator(LogicalKeyboardKey.keyX, meta: true),
              onSelectedIntent: CopySelectionTextIntent.cut(
                SelectionChangedCause.keyboard,
              ),
            ),
            PlatformMenuItem(
              label: 'Copy',
              shortcut: SingleActivator(LogicalKeyboardKey.keyC, meta: true),
              onSelectedIntent: CopySelectionTextIntent.copy,
            ),
            PlatformMenuItem(
              label: 'Paste',
              shortcut: SingleActivator(LogicalKeyboardKey.keyV, meta: true),
              onSelectedIntent: PasteTextIntent(SelectionChangedCause.keyboard),
            ),
            PlatformMenuItem(
              label: 'Select All',
              shortcut: SingleActivator(LogicalKeyboardKey.keyA, meta: true),
              onSelectedIntent: SelectAllTextIntent(
                SelectionChangedCause.keyboard,
              ),
            ),
          ],
        ),
      ],
      child: child,
    );
  }
}
