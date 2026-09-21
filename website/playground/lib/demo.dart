import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

const sampleMarkdown = '''# A little room to think.

This is a real **Flark editor**. Click anywhere and make it yours.

- [x] Try a task checkbox
- [ ] Select a word and make it bold

> Your document stays Markdown.

## A small plan

| Idea | Status |
| --- | --- |
| Write something | In progress |
| Make it yours | Up next |

```dart
final idea = 'Start here';
```

Keep writing here.
''';

enum Starter {
  welcome('Welcome', sampleMarkdown),
  meeting('Meeting notes', meetingMarkdown),
  blank('Blank page', '');

  const Starter(this.label, this.markdown);
  final String label;
  final String markdown;
}

const meetingMarkdown = '''# Monday, with a little more clarity.

**Design sync** · 10:00 AM · The whole team

## What we are here to do

Make the first five minutes feel effortless. Less setup, more getting somewhere.

> A good starting point is an invitation, not an instruction manual.

## The plan

| Topic | Owner | Next step |
| --- | --- | --- |
| First impressions | Design | Sketch the welcome screen |
| A working prototype | Engineering | Put it in people's hands |
| What we learned | Everyone | Leave a note below |

## Before we go

- [x] Agree on the problem
- [ ] Share a rough version
- [ ] Try it with someone new

## Your notes

What would you change?
''';

class HomepageDemo extends StatefulWidget {
  const HomepageDemo({
    super.key,
    required this.backend,
    required this.brightness,
    required this.onOpenLink,
  });

  final FlarkParseBackend backend;
  final ValueListenable<Brightness> brightness;
  final ValueChanged<Uri> onOpenLink;

  @override
  State<HomepageDemo> createState() => _HomepageDemoState();
}

class _HomepageDemoState extends State<HomepageDemo> {
  final focusNode = FocusNode();
  Starter starter = Starter.welcome;
  final controllers = <Starter, FlarkController>{};
  FlarkController get controller => controllers.putIfAbsent(
    starter,
    () => FlarkController(FlarkEditor(widget.backend, text: starter.markdown)),
  );

  @override
  void dispose() {
    for (final controller in controllers.values) {
      controller.dispose();
    }
    focusNode.dispose();
    super.dispose();
  }

  void chooseStarter(Starter next) {
    if (next == starter) return;
    setState(() => starter = next);
    focusNode.requestFocus();
  }

  void reset() {
    controller.command(
      ReplaceRange(0, controller.text.length, starter.markdown),
    );
    controller.command(const SetSelection.caret(0));
    controller.sourceMode(false);
  }

  Future<void> copy(BuildContext context) async {
    try {
      await Clipboard.setData(ClipboardData(text: controller.text));
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(const SnackBar(content: Text('Markdown copied')));
      }
    } catch (_) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'Copy is unavailable. Use Source to select your Markdown.',
            ),
          ),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<Brightness>(
    valueListenable: widget.brightness,
    builder: (_, brightness, _) {
      final colors = ColorScheme.fromSeed(
        seedColor: const Color(0xff478bc9),
        brightness: brightness,
        surface: brightness == Brightness.dark
            ? const Color(0xff141a23)
            : Colors.white,
      );
      return MaterialApp(
        debugShowCheckedModeBanner: false,
        title: 'Live Flark editor',
        theme: ThemeData(
          colorScheme: colors.copyWith(surfaceContainerLow: colors.surface),
          scaffoldBackgroundColor: colors.surface,
          useMaterial3: true,
        ),
        home: Builder(
          builder: (context) => Scaffold(
            body: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                DecoratedBox(
                  decoration: BoxDecoration(
                    border: Border(
                      bottom: BorderSide(
                        color: colors.outlineVariant.withValues(alpha: .45),
                      ),
                    ),
                  ),
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 1000),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 16,
                          vertical: 8,
                        ),
                        child: LayoutBuilder(
                          builder: (context, constraints) => Row(
                            children: [
                              Expanded(
                                child: Wrap(
                                  spacing: 4,
                                  children: [
                                    for (final item in Starter.values)
                                      Semantics(
                                        selected: starter == item,
                                        child: TextButton(
                                          onPressed: () => chooseStarter(item),
                                          style: TextButton.styleFrom(
                                            foregroundColor: starter == item
                                                ? colors.primary
                                                : colors.onSurfaceVariant,
                                            backgroundColor: starter == item
                                                ? colors.primary.withValues(
                                                    alpha: .1,
                                                  )
                                                : null,
                                            textStyle: const TextStyle(
                                              fontSize: 12,
                                              fontWeight: FontWeight.w500,
                                            ),
                                            padding: const EdgeInsets.symmetric(
                                              horizontal: 12,
                                              vertical: 12,
                                            ),
                                            shape: RoundedRectangleBorder(
                                              borderRadius:
                                                  BorderRadius.circular(6),
                                            ),
                                          ),
                                          child: Text(item.label),
                                        ),
                                      ),
                                  ],
                                ),
                              ),
                              if (constraints.maxWidth > 600)
                                Text(
                                  'FLUTTER DEMO',
                                  style: TextStyle(
                                    fontSize: 10,
                                    letterSpacing: 1.4,
                                    color: colors.onSurfaceVariant,
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                Expanded(
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 1000),
                      child: LayoutBuilder(
                        builder: (_, constraints) => FlarkEditorWidget(
                          controller: controller,
                          focusNode: focusNode,
                          // Mounting the demo must not steal focus from the page.
                          autofocus: false,
                          onOpenLink: widget.onOpenLink,
                          baseUri: Uri.base,
                          theme: FlarkThemeData(
                            styles: {
                              FlarkTextRole.body: TextStyle(
                                fontSize: constraints.maxWidth > 600 ? 18 : 16,
                                height: 1.6,
                              ),
                              FlarkTextRole.heading1: TextStyle(
                                fontSize: constraints.maxWidth > 600 ? 36 : 28,
                                height: 1.25,
                              ),
                              FlarkTextRole.heading2: const TextStyle(
                                fontSize: 24,
                                height: 1.3,
                              ),
                            },
                            colors: {FlarkColorRole.canvas: colors.surface},
                            metrics: {
                              FlarkMetric.documentPadding:
                                  constraints.maxWidth > 600 ? 40 : 22,
                            },
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                DecoratedBox(
                  decoration: BoxDecoration(
                    border: Border(
                      top: BorderSide(color: colors.outlineVariant),
                    ),
                  ),
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 1000),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 16),
                        child: LayoutBuilder(
                          builder: (_, constraints) => OverflowBar(
                            alignment: MainAxisAlignment.spaceBetween,
                            overflowAlignment: OverflowBarAlignment.end,
                            children: [
                              TextButton(
                                onPressed: reset,
                                child: const Text('Reset sample'),
                              ),
                              if (constraints.maxWidth > 600)
                                AnimatedBuilder(
                                  animation: controller,
                                  builder: (_, _) => Text(
                                    '${controller.text.length} characters · Session only',
                                    style: TextStyle(
                                      fontSize: 11,
                                      color: colors.onSurfaceVariant,
                                    ),
                                  ),
                                ),
                              TextButton.icon(
                                onPressed: () => copy(context),
                                icon: const Icon(Icons.copy_outlined, size: 16),
                                label: const Text('Copy Markdown'),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}
