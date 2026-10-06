/// Generated platform input sequences for the Flutter host. Each sequence
/// mounts [FlarkEditorWidget] beside another text field, over a document of
/// its own shape (lists, quotes, fences, tables, headings, links, emoji and
/// combining marks, CRLF and mixed line endings, documents past the input
/// window), and drives it with the platform events of one target: Android,
/// iOS and macOS input methods' delta batches, or a browser textarea's full
/// values on a desktop target, since tests never run with kIsWeb. Events
/// include typing, each platform's Backspace and Return, compositions that
/// grow, shrink, convert, commit in part or whole, or cancel, autocorrect,
/// stale and invalid values, hardware keys and selectors, pointer presses,
/// focus, connection and lifecycle changes, widget rebuilds, accessibility
/// edits, the link popover's own field and toolbar or application commands.
///
/// A model of the platform's own text buffer applies every value the host
/// sends it, as an input method does: the web model keeps a textarea's LF
/// line breaks and clamps the selection the way a browser does. After every
/// step:
///
///  * no exception or framework assertion was reported;
///  * each controller's selection is legal, and a composing range belongs to
///    an open kernel composition;
///  * the platform holds exactly the host's window of the document, with its
///    selection and composing range (no drift between them);
///  * an event with one logical meaning (text typed at the caret, Backspace,
///    Return, a selection the platform moved, a decidable key, selector,
///    toolbar command or correction) left the controller as that command
///    leaves a fresh kernel editor in the same state; a composition holds
///    its text as the platform does while it composes, one that commits
///    equals typing its text, and one that cancels leaves no trace but
///    what the input method committed of it;
///  * an application's exact source splice is accepted or refused as a
///    fresh kernel editor in the same state decides, and a refused one
///    leaves no trace;
///  * an accessibility edit of letters by letters shows the requested text
///    or refuses with a notice;
///  * a composition ends with the connection, focus or view that held it,
///    and input for another field or a replaced client never edits.
///
/// `FLARK_INPUT_SEED` and `FLARK_INPUT_ITERATIONS` (sequences per platform)
/// scale the run, `FLARK_INPUT_STEPS` the events per sequence. A failure
/// prints the platform, the sequence seed and its event log;
/// `FLARK_INPUT_SEQUENCE` replays that one sequence (select the platform with
/// `--plain-name`), `FLARK_INPUT_TRACE` prints the document around the caret
/// after every step and `FLARK_INPUT_STATS` counts events and oracle checks.
/// Minimize a failure into a directly named regression rather than storing a
/// replay.
library;

import 'dart:convert';
import 'dart:io' show Platform;
import 'dart:math';
import 'dart:ui' show SemanticsAction, SemanticsActionEvent;

import 'package:flark/code.dart' show CodeEditingDelegate;
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// The platform the sequence emulates. The web is emulated on a desktop
/// target: Flutter tests never run with kIsWeb, so the model supplies the
/// browser's behavior (full values, LF textarea, the Return action the
/// engine sends from the textarea's own keydown listener).
enum _Target {
  android(TargetPlatform.android),
  iOS(TargetPlatform.iOS),
  macOS(TargetPlatform.macOS),
  web(TargetPlatform.linux);

  const _Target(this.platform);
  final TargetPlatform platform;
  bool get apple => this == iOS || this == macOS;
  bool get touch => this == android || this == iOS;
}

const _documents = [
  '',
  'plain words in a paragraph',
  '# Notes\n\nSome **bold**, *em*, `code`, ~~gone~~ and [a link](https://x.y "t").\n',
  '- one\n- two\n  - nested item\n- [ ] task\n- [x] done\n\n1. first\n2. second\n',
  '> quoted line\n> > nested quote\n\nafter the quote',
  '```dart\nvoid main() {\n  print(1);\n}\n```\n\ntext after\n',
  '| a | b |\n| --- | :-: |\n| c | d |\n| e |\n\ntail',
  'emoji 👩🏽‍💻 and e\u0301 and 🇨🇦 flags, 中文字符',
  'Title\n=====\n\nSub\n---\n\n***\n\nend',
  '<b>html</b> <https://auto.link> www.example.com and ![img](u.png) and [ref][1]\n\n[1]: https://r.x\n',
  'line one  \nline two\\\nline three',
  '- [link](u) and **bold *both***\n  continuation\n\n    indented code\n',
];

/// Single characters an input method commits: ASCII Markdown syntax, a tab,
/// precomposed and combining accents, emoji with and without joiners, CJK.
final _typed = [
  ...'abzQ  *_`#->|[]()!\\1.~:<&"\'{}'.characters,
  '\t',
  'é',
  'e\u0301',
  '😀',
  '👩🏽‍💻',
  '中',
  // Supplementary characters that are not emoji: a CJK Extension B
  // character and a mathematical letter.
  '\u{20000}',
  '\u{1D4B3}',
];

/// Longer commits: a pasted clipboard, a predicted word.
const _chunks = [
  'hello ',
  '**p**',
  '- x\n- y',
  '> q',
  '```\nc\n```',
  '| a |\n| - |\n| b |',
  'two\nlines',
  'a\r\nb',
  '[l](u)',
  '日本語',
  'naïve ',
];

/// The values a composition passes through, as an input method shows them.
const _compositions = [
  ['n', 'ni', 'nih', 'niho', 'nihon', '日本'],
  ['h', 'he', 'hel', 'hell', 'hello'],
  ['´', 'é'],
  ['z', 'zh', 'zho', 'zhon', 'zhong', '中'],
  ['k', 'ka', 'か', 'かな', '仮名'],
  ['t', 'te', 'teh', 'the'],
];

/// Autocorrect replacements of a word.
const _corrections = {
  'teh': 'the',
  'alpha': 'Alpha',
  'words': 'worlds',
  'one': 'once',
  'line': 'lime',
  'i': 'I',
  'hello': 'hullo',
};

final _letters = RegExp(r'^[\p{L}\p{M}´]+$', unicode: true);
final _word = RegExp(r'[\p{L}\p{M}´]+', unicode: true);

/// Events run and oracle comparisons made, printed with FLARK_INPUT_STATS.
final _stats = <String, int>{};
void _count(String key) => _stats[key] = (_stats[key] ?? 0) + 1;

void main() {
  final backend = createParseBackend();
  final code = FlarkCodeMirror();
  final seed =
      int.tryParse(Platform.environment['FLARK_INPUT_SEED'] ?? '') ?? 2026;
  final iterations =
      int.tryParse(Platform.environment['FLARK_INPUT_ITERATIONS'] ?? '') ?? 3;
  final steps =
      int.tryParse(Platform.environment['FLARK_INPUT_STEPS'] ?? '') ?? 70;
  // Replays one sequence a failure printed, on the platform filtered by name.
  final only = int.tryParse(Platform.environment['FLARK_INPUT_SEQUENCE'] ?? '');
  final runs = only == null
      ? 'seed $seed, $iterations sequences'
      : 'sequence seed $only';
  for (final target in _Target.values) {
    testWidgets(
      'platform input sequences keep the host and kernel in step: '
      '${target.name} ($runs)',
      (tester) async {
        final semantics = tester.ensureSemantics();
        final master = Random(seed * 31 + target.index);
        for (var i = 0; i < (only == null ? iterations : 1); i++) {
          final s = only ?? master.nextInt(1 << 30);
          await _Sequence(tester, backend, code, target, s, steps).run();
        }
        semantics.dispose();
        if (Platform.environment['FLARK_INPUT_STATS'] != null) {
          final keys = _stats.keys.toList()..sort();
          // ignore: avoid_print
          print(
            '${target.name} stats: '
            '${[for (final k in keys) '$k=${_stats[k]}'].join(' ')}',
          );
          _stats.clear();
        }
      },
      variant: TargetPlatformVariant.only(target.platform),
      timeout: const Timeout(Duration(minutes: 30)),
    );
  }
}

/// The text buffer an input method keeps for its current client.
class _PlatformModel {
  int? client;
  bool multiline = false;
  TextEditingValue held = TextEditingValue.empty;
  int logIndex = 0;
  final clients = <int>[];

  /// Earlier values of the editor's buffer and the client that held them,
  /// for stale input.
  final history = <(int, TextEditingValue)>[];
}

class _Sequence {
  _Sequence(
    this.tester,
    this.backend,
    this.code,
    this.target,
    this.seed,
    this.steps,
  ) : r = Random(seed);
  final WidgetTester tester;
  final FlarkParseBackend backend;
  final CodeEditingDelegate code;
  final _Target target;
  final int seed, steps;
  final Random r;
  final log = <String>[];
  final platform = _PlatformModel();

  late FlarkController c, spare;
  final ownFocus = FocusNode(debugLabel: 'own editor focus');
  final otherFocus = FocusNode(debugLabel: 'other field');
  final otherText = TextEditingController(text: 'other');
  final popoverText = TextEditingController();
  bool readOnly = false, useOwnFocus = false, popoverField = false;
  bool showToolbar = true, mounted = false;
  int theme = 0;
  String? clipboard;
  int _labelStep = 0;

  /// The composition [_compose] has open: the state it began in, when a
  /// reference editor can take it, what it shows, and whether it replaced
  /// a selection.
  ({_State? pre, String shown, bool selected})? _preedit;

  /// History coalescing reads this clock, which advances with each event,
  /// so a replayed sequence groups its undo steps as it did.
  var _time = Duration.zero;

  static final _themes = <FlarkThemeData?>[
    null,
    FlarkThemeData(styles: {FlarkTextRole.body: const TextStyle(fontSize: 23)}),
    FlarkThemeData(metrics: {FlarkMetric.documentPadding: 40}),
  ];

  bool get web => target == _Target.web;

  // ------------------------------------------------------------ lifecycle

  FlarkController _controller() {
    var text = _documents[r.nextInt(_documents.length)];
    if (r.nextInt(3) == 0) {
      // Near or past the platform window, so input contexts rebase.
      final parts = [
        for (var i = 0; i < 30 + r.nextInt(70); i++)
          [
            'Paragraph $i with **bold** words to fill the line.',
            '- item $i with `code`',
            '> quote $i',
            '中文 $i 👩🏽‍💻 e\u0301',
          ][r.nextInt(4)],
      ];
      text = parts.join('\n\n');
    }
    if (r.nextInt(3) == 0) text = text.replaceAll('\n', '\r\n');
    final editor = FlarkEditor(
      backend,
      text: text,
      caret: r.nextInt(text.length + 1),
      codeEditing: r.nextBool() ? code : null,
      syncLimit: r.nextInt(6) == 0 ? 1200 : FlarkEditor.defaultSyncLimit,
      clock: () => _time,
    );
    final controller = FlarkController(editor);
    if (r.nextInt(4) == 0 && text.isNotEmpty) {
      final a = r.nextInt(text.length + 1), b = r.nextInt(text.length + 1);
      controller.command(SetSelection(a, b));
    }
    if (r.nextInt(12) == 0) controller.sourceMode(true);
    return controller;
  }

  Widget _app() => MaterialApp(
    home: Scaffold(
      // An application widget that rebuilds whenever the controller
      // notifies, as a status line does. The framework refuses that rebuild
      // while it builds or unmounts the editor.
      appBar: AppBar(
        title: ListenableBuilder(
          listenable: c,
          builder: (_, _) => Text(
            '${c.editor.composing ? 'composing' : 'idle'} '
            '${c.editor.revision}',
          ),
        ),
      ),
      body: Column(
        children: [
          SizedBox(
            height: 48,
            child: TextField(controller: otherText, focusNode: otherFocus),
          ),
          Expanded(
            child: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              readOnly: readOnly,
              focusNode: useOwnFocus ? ownFocus : null,
              theme: _themes[theme],
              showToolbar: showToolbar,
              showImagePreviews: false,
              onOpenLink: (_) {},
              presentResourceEditor: (context, session) async {
                log.add('  resource editor presented');
                if (r.nextBool()) {
                  session.save(
                    destination: 'https://e.x/${r.nextInt(9)}',
                    label: session.label,
                  );
                }
              },
              linkPopoverBuilder: popoverField
                  ? (_, actions) => Material(
                      child: SizedBox(
                        width: 240,
                        child: TextField(controller: popoverText),
                      ),
                    )
                  : null,
            ),
          ),
        ],
      ),
    ),
  );

  Future<void> run() async {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        switch (call.method) {
          case 'Clipboard.setData':
            clipboard = (call.arguments as Map)['text'] as String?;
          case 'Clipboard.getData':
            return clipboard == null ? null : {'text': clipboard};
          case 'Clipboard.hasStrings':
            return {'value': clipboard != null};
        }
        return null;
      },
    );
    platform.logIndex = tester.testTextInput.log.length;
    c = _controller();
    spare = _controller();
    log.add(
      'document ${jsonEncode(c.text)} ${c.editor.selection}'
      '${c.editor.sourceMode ? ' source' : ''}',
    );
    try {
      await tester.pumpWidget(_app());
      mounted = true;
      await tester.pump();
      _drain();
      _check('mount');
      for (var step = 0; step < steps; step++) {
        _labelStep = step;
        await _event();
      }
      await _unmount();
    } catch (error) {
      // ignore: avoid_print
      print(
        'platform input failure: ${target.name} sequence seed $seed '
        '(FLARK_INPUT_SEED derives it)\n'
        'document ${jsonEncode(c.text)} ${c.editor.selection}\n'
        'platform ${_describe(platform.held)} client ${platform.client}\n'
        '  ${log.join('\n  ')}',
      );
      rethrow;
    } finally {
      if (mounted) {
        try {
          await tester.pumpWidget(const SizedBox());
          await tester.pump();
        } catch (_) {}
      }
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      );
      c.dispose();
      spare.dispose();
      ownFocus.dispose();
      otherFocus.dispose();
      otherText.dispose();
      popoverText.dispose();
    }
  }

  Future<void> _unmount() async {
    log.add('unmount');
    await tester.pumpWidget(const SizedBox());
    mounted = false;
    await tester.pump();
    _drain();
    _check('unmount');
    for (final controller in [c, spare]) {
      expect(
        controller.editor.composing,
        isFalse,
        reason: 'a composition outlived the view that held it',
      );
    }
  }

  // --------------------------------------------------------------- events

  Future<void> _event() async {
    final weights = switch (target) {
      _Target.android => const {
        'compose': 22,
        'recompose': 6,
        'invalid': 1,
        'type': 12,
        'backspace': 10,
        'return': 4,
        'autocorrect': 5,
        'select': 6,
        'stale': 4,
        'touch': 7,
        'key': 3,
        'toolbar': 8,
        'focus': 3,
        'close': 2,
        'lifecycle': 2,
        'rebuild': 4,
        'semantics': 4,
        'existing': 1,
        'burst': 5,
      },
      _Target.iOS => const {
        'type': 22,
        'invalid': 1,
        'compose': 10,
        'backspace': 10,
        'return': 5,
        'autocorrect': 5,
        'select': 6,
        'stale': 4,
        'touch': 7,
        'key': 3,
        'toolbar': 8,
        'focus': 3,
        'close': 3,
        'lifecycle': 2,
        'rebuild': 4,
        'semantics': 4,
        'existing': 1,
        'burst': 5,
      },
      _Target.macOS => const {
        'type': 20,
        'popover': 3,
        'invalid': 1,
        'key': 22,
        'compose': 6,
        'selector': 4,
        'mouse': 8,
        'autocorrect': 2,
        'select': 3,
        'stale': 3,
        'toolbar': 8,
        'focus': 4,
        'lifecycle': 3,
        'rebuild': 5,
        'semantics': 4,
        'close': 1,
        'existing': 1,
        'burst': 5,
      },
      _Target.web => const {
        'type': 22,
        'popover': 3,
        'invalid': 1,
        'key': 22,
        'compose': 8,
        'backspace': 4,
        'mouse': 8,
        'select': 4,
        'autocorrect': 2,
        'stale': 2,
        'toolbar': 8,
        'focus': 5,
        'lifecycle': 3,
        'rebuild': 5,
        'semantics': 4,
        'burst': 5,
      },
    };
    final total = weights.values.fold(0, (a, b) => a + b);
    var pick = r.nextInt(total);
    var kind = weights.keys.first;
    for (final entry in weights.entries) {
      if (pick < entry.value) {
        kind = entry.key;
        break;
      }
      pick -= entry.value;
    }
    _time += const Duration(milliseconds: 400);
    _count('event $kind');
    if (c.text.length > 1100) _count('event on a windowed document');
    switch (kind) {
      case 'compose':
        await _compose();
      case 'type':
        await _type(_randomText());
      case 'backspace':
        await _backspace();
      case 'return':
        await _return();
      case 'autocorrect':
        await _autocorrect();
      case 'select':
        await _platformSelect();
      case 'stale':
        await _stale();
      case 'touch':
        await _pointer(touch: true);
      case 'mouse':
        await _pointer(touch: false);
      case 'key':
        await _hardwareKey();
      case 'selector':
        await _selectors();
      case 'toolbar':
        await _toolbar();
      case 'focus':
        await _focus();
      case 'close':
        await _close();
      case 'lifecycle':
        await _lifecycle();
      case 'rebuild':
        await _rebuild();
      case 'semantics':
        await _semantics();
      case 'existing':
        await _requestExisting();
      case 'burst':
        await _burst();
      case 'recompose':
        await _recompose();
      case 'invalid':
        await _invalid();
      case 'popover':
        await _popover();
    }
  }

  String _randomText() {
    if (r.nextInt(8) == 0) return _chunks[r.nextInt(_chunks.length)];
    return _typed[r.nextInt(_typed.length)];
  }

  /// Whether the platform's current client is the editor's.
  bool get _editorInput =>
      mounted &&
      platform.client != null &&
      platform.multiline &&
      platform.held.selection.isValid;

  bool get _editorFocused {
    final primary = FocusManager.instance.primaryFocus;
    return primary != null &&
        (identical(primary, ownFocus) || primary.debugLabel == 'Flark editor');
  }

  /// Text typed at the platform's selection. On the web and macOS a key
  /// event precedes it; neither handles a printable key.
  Future<void> _type(String text, {bool settle = true}) async {
    if (!_editorInput) return _idle();
    if (_platformComposing) return _idle();
    final held = platform.held;
    final s = held.selection.start, e = held.selection.end;
    log.add('type ${jsonEncode(text)} at $s..$e');
    final ref = _reference();
    if ((web || target == _Target.macOS) && text.length == 1) {
      final key = _keyFor(text);
      if (key != null) {
        final handled = await tester.sendKeyEvent(key);
        _drain();
        if (handled) {
          fail('${_label()}: a printable key was handled by the editor');
        }
      }
    }
    if (!_editorInput) return _settle(settle);
    final now = platform.held;
    final next = TextEditingValue(
      text: now.text.replaceRange(s, e, text),
      selection: TextSelection.collapsed(offset: s + text.length),
    );
    await _deliver(now, next, s, e, text);
    await _settle(settle);
    // A textarea delivers the text with LF line breaks. Text typed over an
    // identical selection changes nothing a value can show.
    final typed = web
        ? _textarea(next).text.substring(s, s + _lf(text).length)
        : text;
    if (now.text.substring(s, e) == typed) return;
    _expect(ref, [typed == '\n' ? const Newline() : InsertText(typed)]);
  }

  LogicalKeyboardKey? _keyFor(String text) {
    final unit = text.codeUnitAt(0);
    if (unit >= 0x61 && unit <= 0x7a) {
      return LogicalKeyboardKey(LogicalKeyboardKey.keyA.keyId + unit - 0x61);
    }
    if (text == ' ') return LogicalKeyboardKey.space;
    if (unit >= 0x30 && unit <= 0x39) {
      return LogicalKeyboardKey(LogicalKeyboardKey.digit0.keyId + unit - 0x30);
    }
    return null;
  }

  /// Backspace. Android and the web delete a whole grapheme; iOS deletes
  /// one code unit unless the character before the caret starts with an
  /// emoji, which can split a combining sequence or a surrogate pair. A
  /// selection is deleted.
  Future<void> _backspace({bool settle = true}) async {
    if (!_editorInput || _platformComposing) return _idle();
    final held = platform.held;
    final ref = _reference();
    if (target == _Target.android && r.nextBool()) {
      // An IME's KEYCODE_DEL reaches the framework as a key first.
      log.add('backspace key');
      await _sendKey(LogicalKeyboardKey.backspace);
      await _settle(settle);
      _expect(ref, const [DeleteBackward()]);
      return;
    }
    var s = held.selection.start;
    final e = held.selection.end;
    var whole = true;
    if (s == e) {
      if (s == 0) return _idle();
      final grapheme = CharacterRange.at(held.text, s - 1);
      final start = grapheme.stringBeforeLength;
      final cluster = held.text.substring(start, s);
      if (target == _Target.iOS && !_emoji(cluster)) {
        s = s - 1;
        // Half a surrogate pair is no text: it still means Backspace. Half
        // a combining sequence is the platform's own edit.
        final unit = held.text.codeUnitAt(s);
        whole = s == start || (unit >= 0xdc00 && unit <= 0xdfff);
      } else {
        s = start;
      }
    }
    log.add('backspace deletes $s..$e');
    final next = TextEditingValue(
      text: held.text.replaceRange(s, e, ''),
      selection: TextSelection.collapsed(offset: s),
    );
    await _deliver(held, next, s, e, '');
    await _settle(settle);
    if (whole) _expect(ref, const [DeleteBackward()]);
  }

  /// Roughly ICU's Emoji property of the cluster's first code point, which
  /// the iOS engine reads.
  static bool _emoji(String cluster) {
    final rune = cluster.runes.first;
    return (rune >= 0x1f000 && rune <= 0x1faff) ||
        (rune >= 0x2600 && rune <= 0x27bf) ||
        '#*0123456789\u00a9\u00ae'.runes.contains(rune);
  }

  /// Return as each platform's soft keyboard or keyboard delivers it.
  Future<void> _return() async {
    if (!_editorInput || _platformComposing) return _idle();
    switch (target) {
      case _Target.android:
        // Gboard commits a line break as text in a multiline field.
        await _type('\n');
      case _Target.iOS:
        // UIKit asks shouldChangeTextInRange before inserting "\n": the
        // engine sends the newline action, returns YES, and the inserted
        // line break follows as a delta against the text before it.
        // Autocorrect may fix the word before the caret in the same batch.
        final held = platform.held;
        final s = held.selection.start, e = held.selection.end;
        final word = s == e && r.nextBool() ? _wordBefore(held) : null;
        log.add(
          'iOS return at $s..$e'
          '${word == null ? '' : ' correcting ${jsonEncode(word.$3)}'}',
        );
        var ref = _reference();
        final window = _window();
        final edits = <Map<String, Object?>>[];
        var text = held.text, caret = s;
        if (word != null) {
          final (from, to, original) = word;
          final fixed = _corrected(original);
          caret += fixed.length - original.length;
          final corrected = TextEditingValue(
            text: text.replaceRange(from, to, fixed),
            selection: TextSelection.collapsed(offset: caret),
          );
          edits.add(_delta(text, from, to, fixed, corrected));
          text = corrected.text;
          // Only a plain word's correction has one meaning.
          final a = window?.source(from), b = window?.source(to);
          if (ref == null ||
              a == null ||
              b == null ||
              ref.sourceMode ||
              ref.document.ownersOfContent(a, b).isNotEmpty) {
            ref = null;
          } else {
            ref.apply(ReplaceRange(a, b, fixed));
          }
        }
        await _sendIosReturn(held, edits, text, caret, ref);
      case _Target.macOS:
      case _Target.web:
        await _enterKey(shift: r.nextInt(5) == 0);
    }
  }

  /// UIKit's Return: the newline action, then one delta batch of any
  /// autocorrection and the line break it inserted.
  Future<void> _sendIosReturn(
    TextEditingValue held,
    List<Map<String, Object?>> edits,
    String text,
    int caret,
    FlarkEditor? ref,
  ) async {
    final s = caret, e = held.selection.end + text.length - held.text.length;
    final next = TextEditingValue(
      text: text.replaceRange(s, e, '\n'),
      selection: TextSelection.collapsed(offset: s + 1),
    );
    platform.held = next;
    await _platformCall('TextInputClient.performAction', [
      platform.client,
      'TextInputAction.newline',
    ]);
    _drain();
    await _sendDeltas([...edits, _delta(text, s, e, '\n', next)]);
    _drain();
    await _settle(true);
    // A line break typed over a selected one changes nothing a value can
    // show.
    if (text.substring(s, e) == '\n') return;
    _expect(ref, const [Newline()]);
  }

  /// The letters before a collapsed platform caret, when it ends a word.
  (int, int, String)? _wordBefore(TextEditingValue held) {
    final end = held.selection.extentOffset;
    if (end < held.text.length && _isWordUnit(held.text.codeUnitAt(end))) {
      return null;
    }
    var start = end;
    while (start > 0 && _isWordUnit(held.text.codeUnitAt(start - 1))) {
      start--;
    }
    return start == end ? null : (start, end, held.text.substring(start, end));
  }

  static String _corrected(String word) =>
      _corrections[word] ??
      (word.length > 1
          ? '${word[0].toUpperCase()}${word.substring(1)}'
          : word.toUpperCase());

  /// A hardware Return. On the web, the engine's textarea listener sends
  /// the newline action for every Return keydown, after Flutter's own key
  /// handling saw the key; the textarea inserts a line break only when
  /// Flutter left the key unhandled.
  Future<void> _enterKey({bool shift = false}) async {
    final ref = _keyReference();
    final intent = _keyIntent(LogicalKeyboardKey.enter, shift: shift);
    log.add('enter key shift=$shift');
    final handled = await _sendKey(LogicalKeyboardKey.enter, shift: shift);
    if (web && _editorInput) {
      await _platformCall('TextInputClient.performAction', [
        platform.client,
        'TextInputAction.newline',
      ]);
      _drain();
      if (!handled && _editorInput && !_platformComposing) {
        final held = platform.held;
        final s = held.selection.start, e = held.selection.end;
        await _deliver(
          held,
          TextEditingValue(
            text: held.text.replaceRange(s, e, '\n'),
            selection: TextSelection.collapsed(offset: s + 1),
          ),
          s,
          e,
          '\n',
        );
      }
    }
    await _settle(true);
    if (intent != null) _expect(ref, intent);
  }

  /// Autocorrect replaces the word before the caret: with the caret at its
  /// end, with a space typed in the same value, or after a space the
  /// platform already delivered.
  Future<void> _autocorrect() async {
    if (!_editorInput || _platformComposing) return _idle();
    var held = platform.held;
    if (!held.selection.isCollapsed) return _idle();
    final caret = held.selection.extentOffset;
    var end = caret;
    final spaced = end > 0 && held.text.codeUnitAt(end - 1) == 0x20;
    if (spaced) end--;
    var start = end;
    while (start > 0 && _isWordUnit(held.text.codeUnitAt(start - 1))) {
      start--;
    }
    if (start == end) return _type(_randomText());
    final word = held.text.substring(start, end);
    final replacement =
        _corrections[word] ?? (word.length > 1 ? word.toUpperCase() : 'Ab');
    if (replacement == word) return _idle();
    final mode = spaced ? 2 : r.nextInt(2);
    log.add(
      'autocorrect ${jsonEncode(word)} -> ${jsonEncode(replacement)} '
      'at $start..$end mode $mode',
    );
    final ref = _reference();
    final window = _window();
    final from = window?.source(start);
    final to = window?.source(end);
    final String suffix = mode == 1 ? ' ' : '';
    final next = TextEditingValue(
      text: held.text.replaceRange(start, end, '$replacement$suffix'),
      selection: TextSelection.collapsed(
        offset: mode == 2
            ? caret + replacement.length - word.length
            : start + replacement.length + suffix.length,
      ),
    );
    await _deliver(held, next, start, end, '$replacement$suffix');
    await _settle(true);
    held = platform.held;
    if (ref == null || from == null || to == null) return;
    // Only plain words have one meaning: the same replacement made by the
    // kernel, with the caret where the platform put it.
    if (ref.sourceMode || ref.document.ownersOfContent(from, to).isNotEmpty) {
      return;
    }
    // A full value carries no record of the text it edited, and its minimal
    // difference ends before a space the platform already delivered. The
    // host reads that as the caret's correction, as a delta would say.
    ref.apply(ReplaceRange(from, to, replacement));
    if (mode == 1) ref.apply(const InsertText(' '));
    if (mode == 2) ref.apply(SetSelection.caret(ref.selection.extent + 1));
    _compare(ref, 'autocorrect');
  }

  static bool _isWordUnit(int unit) =>
      (unit >= 0x61 && unit <= 0x7a) || (unit >= 0x41 && unit <= 0x5a);

  /// The platform moves its selection: a caret or a range, on grapheme
  /// boundaries of the text it holds.
  Future<void> _platformSelect({bool settle = true}) async {
    if (!_editorInput || _platformComposing) return _idle();
    final held = platform.held;
    final boundaries = _boundaries(held.text);
    final base = boundaries[r.nextInt(boundaries.length)];
    final extent = r.nextInt(3) == 0
        ? boundaries[r.nextInt(boundaries.length)]
        : base;
    log.add('platform selects $base..$extent');
    final ref = _reference();
    final window = _window();
    final next = held.copyWith(
      selection: TextSelection(baseOffset: base, extentOffset: extent),
    );
    platform.held = next;
    if (web) {
      await _sendValue(next);
    } else {
      await _sendDeltas([_nonText(held.text, next)]);
    }
    _drain();
    await _settle(settle);
    if (window != null && ref != null) {
      _expect(ref, [SetSelection(window.source(base), window.source(extent))]);
    }
  }

  /// The grapheme boundaries within each run of letters in [value] that
  /// holds more than one grapheme, from [boundaries], all of [value]'s.
  static List<List<int>> _words(String value, List<int> boundaries) {
    final words = <List<int>>[];
    var k = 0;
    for (final word in _word.allMatches(value)) {
      while (k < boundaries.length && boundaries[k] < word.start) {
        k++;
      }
      final inside = [
        for (var j = k; j < boundaries.length && boundaries[j] <= word.end; j++)
          boundaries[j],
      ];
      if (inside.length > 1) words.add(inside);
    }
    return words;
  }

  static List<int> _boundaries(String text) {
    final out = <int>[0];
    var offset = 0;
    for (final g in text.characters) {
      offset += g.length;
      out.add(offset);
    }
    return out;
  }

  /// Input that crossed the host's correction: a delta against an older
  /// buffer, a message for a client the host replaced, or (web) a full value
  /// computed from an older buffer.
  Future<void> _stale() async {
    if (!_editorInput || _platformComposing) return _idle();
    final held = platform.held;
    final older = [
      for (final (client, v) in platform.history)
        if (v.text != held.text &&
            v.selection.isValid &&
            (!web || client == platform.client))
          v,
    ];
    final before = (c.text, c.editor.selection);
    if (r.nextBool() && platform.clients.length > 1) {
      final old = platform.clients[r.nextInt(platform.clients.length - 1)];
      if (old == platform.client) return _idle();
      final at = held.selection.start;
      final next = TextEditingValue(
        text: held.text.replaceRange(at, held.selection.end, 'S'),
        selection: TextSelection.collapsed(offset: at + 1),
      );
      log.add('stale client $old types at $at');
      if (web) {
        await _sendValue(next, client: old);
      } else {
        await _sendDeltas([
          _delta(held.text, at, held.selection.end, 'S', next),
        ], client: old);
      }
      _drain();
      await _settle(true);
      expect(
        (c.text, c.editor.selection),
        before,
        reason: '${_label()}: input for a replaced client changed the document',
      );
      return;
    }
    if (older.isEmpty) return _idle();
    final old = older[r.nextInt(older.length)];
    final at = old.selection.start;
    final next = TextEditingValue(
      text: old.text.replaceRange(at, old.selection.end, 'S'),
      selection: TextSelection.collapsed(offset: at + 1),
    );
    if (web) {
      // A full value carries no record of the text it edited, and the
      // textarea holds what it reports. A browser reports the textarea's
      // current state, so this only checks that the two stay in step.
      log.add('stale value typed at $at of an older buffer');
      platform.held = _textarea(next);
      await _sendValue(platform.held);
      _drain();
      await _settle(true);
      return;
    }
    log.add('stale delta typed at $at of an older buffer');
    await _sendDeltas([_delta(old.text, at, old.selection.end, 'S', next)]);
    _drain();
    await _settle(true);
    expect(
      (c.text, c.editor.selection),
      before,
      reason: '${_label()}: a delta against older text was applied',
    );
  }

  // ---------------------------------------------------------- composition

  bool get _platformComposing =>
      platform.held.composing.isValid && !platform.held.composing.isCollapsed;

  /// One composition, from its first preedit to a commit or a cancel, with
  /// occasional interruptions by the host's own events between updates.
  Future<void> _compose() async {
    if (!_editorInput || _platformComposing) return _idle();
    _preedit = null;
    final held = platform.held;
    final values = _compositions[r.nextInt(_compositions.length)];
    final plain = values.every(_letters.hasMatch);
    final e = c.editor;
    final pre = _reference() == null
        ? null
        : _State(e.source, e.selection, e.sourceMode, e.typingContext);
    final preHistory = c.editor.history.canUndo;
    final selected = !held.selection.isCollapsed;
    var at = held.selection.start;
    var replaceEnd = held.selection.end;
    log.add('compose ${jsonEncode(values)} at $at..$replaceEnd');
    var shown = '';
    // What the input method committed of its composition, before the text
    // it still composes: it stays.
    var kept = '';
    var oracle = pre != null && plain;
    for (var i = 0; i < values.length; i++) {
      if (!_editorInput || (i > 0 && !_platformComposing)) {
        log.add('  composition ended');
        return;
      }
      final now = platform.held;
      if (i > 0) {
        at = now.composing.start;
        replaceEnd = now.composing.end;
      }
      var value = values[i];
      if (r.nextInt(8) == 0 && shown.isNotEmpty) {
        // The user deletes the last composed character.
        value = shown.characters.skipLast(1).string;
        if (value.isEmpty) value = values[i];
      }
      final next = TextEditingValue(
        text: now.text.replaceRange(at, replaceEnd, value),
        selection: TextSelection.collapsed(offset: at + value.length),
        composing: TextRange(start: at, end: at + value.length),
      );
      log.add('  preedit ${jsonEncode(value)} over $at..$replaceEnd');
      await _deliver(now, next, at, replaceEnd, value);
      await _settle(r.nextInt(3) != 0);
      if (!_editorInput || !_platformComposing) {
        // The host refused the preedit and resynchronized the platform. The
        // kernel must refuse typing that text there too.
        if (oracle && i == 0) {
          final typed = _reference(from: pre);
          if (typed != null && typed.apply(InsertText(value))) {
            fail(
              '${_label()}: the host refused preedit ${jsonEncode(value)}, '
              'which typing places as ${jsonEncode(typed.source)}',
            );
          }
          _compare(_reference(from: pre), 'refused preedit');
        }
        log.add('  composition ended by the host');
        return;
      }
      // What the platform shows: a preedit the kernel refused leaves the
      // previous one.
      var region = platform.held.composing;
      shown = platform.held.text.substring(region.start, region.end);
      _preedit = (pre: pre, shown: '$kept$shown', selected: selected);
      if (oracle) {
        _compareComposition(pre, selected, '$kept$shown', 'composing');
        if (!c.editor.composing || c.value.composing == TextRange.empty) {
          fail('${_label()}: the host is not composing ${jsonEncode(shown)}');
        }
      }
      if (shown.characters.length > 1 && r.nextInt(6) == 0) {
        // The input method commits the start of what it composes and goes on
        // composing the rest, in one update: a Korean syllable as the next
        // one's first letter is typed, a Japanese clause converted ahead of
        // the rest. The composition goes on; what it committed stays.
        final head = shown.characters
            .take(1 + r.nextInt(shown.characters.length - 1))
            .string;
        region = TextRange(start: region.start + head.length, end: region.end);
        final moved = platform.held.copyWith(composing: region);
        log.add('  commits ${jsonEncode(head)} and composes on');
        _count('partial commit');
        platform.held = moved;
        if (web) {
          await _sendValue(moved);
        } else {
          await _sendDeltas([_nonText(moved.text, moved)]);
        }
        _drain();
        await _settle(true);
        kept += head;
        shown = shown.substring(head.length);
        _preedit = (pre: pre, shown: '$kept$shown', selected: selected);
        if (oracle) {
          _compareComposition(pre, selected, '$kept$shown', 'composing');
          expect(
            c.editor.composing && c.value.composing != TextRange.empty,
            isTrue,
            reason: '${_label()}: a partial commit ended the composition',
          );
        }
      }
      if (shown.length > 1 && r.nextInt(8) == 0) {
        // The input method moves its caret within the composed text.
        final inside = region.start + 1 + r.nextInt(shown.length - 1);
        final moved = platform.held.copyWith(
          selection: TextSelection.collapsed(offset: inside),
        );
        log.add('  caret moves to $inside within the composition');
        platform.held = moved;
        if (web) {
          await _sendValue(moved);
        } else {
          await _sendDeltas([_nonText(moved.text, moved)]);
        }
        _drain();
        await _settle(true);
        if (oracle) {
          final composed = _reference(from: pre)?..beginComposition();
          if (composed != null) {
            if ('$kept$shown'.isNotEmpty) {
              composed.apply(InsertText('$kept$shown'));
            }
            expect(c.text, composed.source, reason: '${_label()}: caret move');
          }
          expect(
            c.editor.composing && _platformComposing,
            isTrue,
            reason: '${_label()}: moving the caret ended the composition',
          );
        }
        oracle = false;
      }
      if (r.nextInt(10) == 0) {
        // Something else happens while the user composes.
        oracle = false;
        log.add('  interrupted');
        await _interrupt();
        if (!_platformComposing || !c.editor.composing) {
          log.add('  composition ended by the interruption');
          return;
        }
      }
    }
    if (!_editorInput || !_platformComposing) return;
    final now = platform.held;
    final region = now.composing;
    final ending = r.nextInt(10);
    if (ending < 2) {
      // The input method removes its text and ends the composition.
      log.add('  cancel');
      final next = TextEditingValue(
        text: now.text.replaceRange(region.start, region.end, ''),
        selection: TextSelection.collapsed(offset: region.start),
      );
      await _deliver(now, next, region.start, region.end, '');
      await _settle(true);
      if (oracle) {
        // What the input method committed stays.
        _compareComposition(pre, selected, kept, 'cancel');
        if (!selected && kept.isEmpty) {
          expect(
            c.editor.history.canUndo,
            preHistory,
            reason: '${_label()}: a cancelled composition changed history',
          );
        }
      }
    } else if (ending < 3 &&
        (target == _Target.macOS || target == _Target.web)) {
      // Escape: Flutter sees the key before the input method.
      log.add('  escape');
      await _sendKey(LogicalKeyboardKey.escape);
      await _settle(true);
      if (oracle) {
        // What the input method committed stays.
        kept.isEmpty
            ? _compare(_reference(from: pre), 'escape cancels')
            : _compareComposition(pre, selected, kept, 'escape');
      }
    } else {
      final trailing = target == _Target.android && ending >= 8
          ? (ending == 8 ? ' ' : '\n')
          : '';
      // Gboard may commit its correction of the composed word.
      final fixed = (target == _Target.android || web) && r.nextInt(3) == 0
          ? _corrected(shown)
          : shown;
      log.add(
        '  commit${fixed == shown ? '' : ' corrected to ${jsonEncode(fixed)}'}'
        '${trailing.isEmpty ? '' : ' with ${jsonEncode(trailing)}'}',
      );
      final end = region.start + fixed.length;
      final committed = TextEditingValue(
        text: now.text.replaceRange(region.start, region.end, fixed),
        selection: TextSelection.collapsed(offset: end),
      );
      final next = TextEditingValue(
        text: committed.text.replaceRange(end, end, trailing),
        selection: TextSelection.collapsed(offset: end + trailing.length),
      );
      platform.held = next;
      if (web) {
        await _sendValue(next);
      } else {
        // Android commits the word and its trailing text in one batch.
        await _sendDeltas([
          fixed == shown
              ? _nonText(now.text, committed)
              : _delta(now.text, region.start, region.end, fixed, committed),
          if (trailing.isNotEmpty)
            _delta(committed.text, end, end, trailing, next),
        ]);
      }
      shown = fixed;
      _drain();
      await _settle(true);
      if (oracle) {
        if (trailing == '\n') {
          final ref = _reference(from: pre);
          ref?.apply(InsertText('$kept$shown'));
          ref?.apply(const Newline());
          _compare(ref, 'commit with Return');
        } else if (trailing == ' ') {
          final ref = _reference(from: pre)?..apply(InsertText('$kept$shown '));
          _compare(ref, 'commit with a space');
        } else {
          _compareComposition(pre, selected, '$kept$shown', 'commit');
        }
        expect(
          c.editor.composing,
          isFalse,
          reason: '${_label()}: commit left the kernel composing',
        );
        // A composition that changed the source is one undo step.
        if (trailing.isEmpty && c.text != pre!.source && r.nextInt(3) == 0) {
          log.add('  undo the composition');
          c.command(const Undo());
          await _settle(true);
          _compare(_reference(from: pre), 'undo of a committed composition');
        }
      }
    }
  }

  /// Gboard composes the word before its caret again when the caret comes
  /// to rest after it: a composing region over existing text, letters typed
  /// into it, then a commit.
  Future<void> _recompose() async {
    if (!_editorInput || _platformComposing) return _idle();
    _preedit = null;
    final held = platform.held;
    final word = held.selection.isCollapsed ? _wordBefore(held) : null;
    if (word == null) return _compose();
    final (from, to, text) = word;
    log.add('recompose ${jsonEncode(text)} at $from..$to');
    var ref = _reference();
    final window = _window();
    final a = window?.source(from), b = window?.source(to);
    if (ref == null ||
        a == null ||
        b == null ||
        ref.sourceMode ||
        ref.document.ownersOfContent(a, b).isNotEmpty) {
      ref = null;
    }
    final before = (c.text, c.editor.selection);
    final region = held.copyWith(
      composing: TextRange(start: from, end: to),
    );
    platform.held = region;
    await _sendDeltas([_nonText(held.text, region)]);
    _drain();
    await _settle(true);
    expect(
      (c.text, c.editor.selection),
      before,
      reason: '${_label()}: composing existing text changed the document',
    );
    var suffix = '';
    for (var i = 1 + r.nextInt(3); i > 0; i--) {
      if (!_editorInput || !_platformComposing) return;
      final now = platform.held, composing = now.composing;
      final letter = 'xyz'[r.nextInt(3)];
      final grown = now.text.substring(composing.start, composing.end) + letter;
      suffix += letter;
      final next = TextEditingValue(
        text: now.text.replaceRange(composing.start, composing.end, grown),
        selection: TextSelection.collapsed(
          offset: composing.start + grown.length,
        ),
        composing: TextRange(
          start: composing.start,
          end: composing.start + grown.length,
        ),
      );
      log.add('  preedit ${jsonEncode(grown)}');
      // Android replaces the whole composing region.
      await _deliver(now, next, composing.start, composing.end, grown);
      await _settle(true);
    }
    if (!_editorInput || !_platformComposing) return;
    final now = platform.held;
    final committed = now.copyWith(composing: TextRange.empty);
    log.add('  commit');
    platform.held = committed;
    await _sendDeltas([_nonText(now.text, committed)]);
    _drain();
    await _settle(true);
    if (ref != null) {
      ref.apply(InsertText(suffix));
      _compare(ref, 'recomposed word');
    }
  }

  /// A value whose selection the platform lost (-1), which describes no
  /// edit: the document stays, and the platform is resynchronized.
  Future<void> _invalid() async {
    if (!_editorInput || _platformComposing) return _idle();
    final held = platform.held;
    final before = (c.text, c.editor.selection);
    final next = held.copyWith(
      selection: const TextSelection.collapsed(offset: -1),
    );
    log.add('platform reports no selection');
    platform.held = next;
    if (web) {
      await _sendValue(next);
    } else {
      await _sendDeltas([_nonText(held.text, next)]);
    }
    _drain();
    await _settle(true);
    expect(
      (c.text, c.editor.selection),
      before,
      reason: '${_label()}: a value without a selection edited the document',
    );
  }

  /// The link popover's own text field: Shift+F10 on a link moves focus into
  /// it, its input reaches it and not the document, and Escape returns.
  Future<void> _popover() async {
    if (!mounted || readOnly || c.editor.sourceMode || c.editor.composing) {
      return _idle();
    }
    final links = [
      for (final resource in c.editor.document.resources)
        if (!resource.isImage && resource.contentEnd > resource.contentStart)
          resource,
    ];
    if (links.isEmpty) return _hardwareKey();
    final link = links[r.nextInt(links.length)];
    if (!popoverField) {
      popoverField = true;
      await tester.pumpWidget(_app());
      await tester.pump();
      _drain();
    }
    if (!_editorFocused) await _refocus();
    log.add('link popover for ${jsonEncode(link.destination)}');
    c.command(SetSelection.caret(link.contentStart + 1));
    await tester.pump();
    _drain();
    await _sendKey(LogicalKeyboardKey.f10, shift: true);
    await tester.pump();
    await tester.pump();
    _drain();
    await _settle(false);
    final before = (c.text, c.editor.selection);
    if (platform.client != null && !platform.multiline) {
      final held = platform.held;
      final next = TextEditingValue(
        text: '${held.text}p',
        selection: TextSelection.collapsed(offset: held.text.length + 1),
      );
      log.add('  type in the popover field');
      platform.held = next;
      await _sendValue(next);
      _drain();
      // A key the field leaves unhandled bubbles to the editor.
      await _sendKey(LogicalKeyboardKey.keyH);
      await tester.pump();
      _drain();
      await _settle(false);
      expect(
        (c.text, c.editor.selection),
        before,
        reason: '${_label()}: input for the popover field reached the editor',
      );
      expect(
        platform.multiline,
        isFalse,
        reason: '${_label()}: the editor took the popover field\'s input',
      );
    }
    log.add('  escape');
    await _sendKey(LogicalKeyboardKey.escape);
    await tester.pump();
    _drain();
    await _settle(true);
    expect(
      (c.text, c.editor.selection),
      before,
      reason: '${_label()}: closing the popover changed the document',
    );
  }

  /// Compares the controller with the state [pre] reached by typing
  /// [shown] in place of its selection.
  void _compareComposition(
    _State? pre,
    bool selected,
    String shown,
    String what,
  ) {
    final ref = _reference(from: pre);
    if (ref == null) return;
    // While the platform composes, the kernel holds its text as it is; the
    // commit types it.
    if (what == 'composing') ref.beginComposition();
    if (shown.isNotEmpty) {
      ref.apply(InsertText(shown));
    } else if (selected) {
      ref.apply(const DeleteBackward());
    }
    _compare(ref, '$what ${jsonEncode(shown)}');
  }

  /// A host event while the platform composes.
  Future<void> _interrupt() async {
    switch (r.nextInt(8)) {
      case 7:
        await _semantics();
      case 0:
        await _toolbar();
      case 1:
        await _pointer(touch: target.touch);
      case 2:
        await _close();
      case 3:
        await _focus();
      case 4:
        await _rebuild();
      case 5:
        await _lifecycle();
      default:
        await _platformSelect();
    }
  }

  // ------------------------------------------------------------------ keys

  Future<void> _hardwareKey() async {
    if (!mounted) return _idle();
    if (_platformComposing) return _idle();
    final keys = [
      LogicalKeyboardKey.arrowLeft,
      LogicalKeyboardKey.arrowRight,
      LogicalKeyboardKey.arrowUp,
      LogicalKeyboardKey.arrowDown,
      LogicalKeyboardKey.backspace,
      LogicalKeyboardKey.delete,
      LogicalKeyboardKey.enter,
      LogicalKeyboardKey.tab,
      LogicalKeyboardKey.home,
      LogicalKeyboardKey.end,
      LogicalKeyboardKey.escape,
      LogicalKeyboardKey.keyA,
      LogicalKeyboardKey.keyB,
      LogicalKeyboardKey.keyI,
      LogicalKeyboardKey.keyZ,
      LogicalKeyboardKey.keyC,
      LogicalKeyboardKey.keyX,
      LogicalKeyboardKey.keyV,
      LogicalKeyboardKey.keyK,
      LogicalKeyboardKey.f10,
    ];
    final key = keys[r.nextInt(keys.length)];
    final shift = r.nextInt(3) == 0;
    final primaryKey = target.apple
        ? LogicalKeyboardKey.metaLeft
        : LogicalKeyboardKey.controlLeft;
    final letter = key.keyLabel.length == 1;
    final primary =
        letter || (key != LogicalKeyboardKey.f10 && r.nextInt(5) == 0);
    final alt = !primary && r.nextInt(6) == 0;
    if (key == LogicalKeyboardKey.enter && !primary && !alt) {
      return _enterKey(shift: shift);
    }
    final ref = _keyReference();
    final intent = _keyIntent(
      key,
      shift: shift,
      alt: alt,
      meta: primary && target.apple,
      control: primary && !target.apple,
    );
    log.add(
      'key ${key.keyLabel}${shift ? ' +shift' : ''}${alt ? ' +alt' : ''}'
      '${primary ? ' +primary' : ''}',
    );
    await _sendKey(
      key,
      shift: shift,
      alt: alt,
      modifier: primary ? primaryKey : null,
    );
    await _settle(true);
    if (intent != null) _expect(ref, intent);
  }

  Future<bool> _sendKey(
    LogicalKeyboardKey key, {
    bool shift = false,
    bool alt = false,
    LogicalKeyboardKey? modifier,
  }) async {
    if (modifier != null) await tester.sendKeyDownEvent(modifier);
    if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    if (alt) await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    final handled = await tester.sendKeyEvent(key);
    if (alt) await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    if (modifier != null) await tester.sendKeyUpEvent(modifier);
    _drain();
    return handled;
  }

  /// A reference for a key, when the key reaches the editor with a
  /// meaning of its own.
  FlarkEditor? _keyReference() {
    if (!_editorFocused) _count('no key reference: unfocused');
    if (readOnly) _count('no key reference: read only');
    if (_popoverOpen) _count('no key reference: popover');
    return _editorFocused && !readOnly && !_popoverOpen ? _reference() : null;
  }

  bool get _popoverOpen =>
      find.byType(FlarkLinkPopover).evaluate().isNotEmpty ||
      (popoverField &&
          find
              .byWidgetPredicate(
                (w) => w is TextField && w.controller == popoverText,
              )
              .evaluate()
              .isNotEmpty);

  /// The kernel commands a key means, or null when it depends on glyph
  /// geometry, history, the clipboard's timing or a scope the reference
  /// cannot share.
  List<FlarkCommand>? _keyIntent(
    LogicalKeyboardKey key, {
    bool shift = false,
    bool alt = false,
    bool control = false,
    bool meta = false,
  }) {
    final e = c.editor;
    if (!_editorFocused || readOnly || _popoverOpen) return null;
    if (e.composing) return null;
    final apple = target.apple;
    final primary = meta || control;
    final horizontal =
        key == LogicalKeyboardKey.arrowLeft ||
        key == LogicalKeyboardKey.arrowRight;
    final direction = key == LogicalKeyboardKey.arrowRight
        ? MoveDirection.forward
        : MoveDirection.backward;
    if (key == LogicalKeyboardKey.escape) return const [];
    if (key == LogicalKeyboardKey.f10) return null;
    if ((alt || control) &&
        (key == LogicalKeyboardKey.backspace ||
            key == LogicalKeyboardKey.delete)) {
      return [
        key == LogicalKeyboardKey.backspace
            ? const DeleteBackward(word: true)
            : const DeleteForward(word: true),
      ];
    }
    if (apple && meta) {
      if (key == LogicalKeyboardKey.backspace ||
          key == LogicalKeyboardKey.delete) {
        return null;
      }
    }
    if (!apple && control && horizontal) {
      return [MoveCaret(direction, unit: MoveUnit.word, extend: shift)];
    }
    if (primary) {
      final start =
          (meta && key == LogicalKeyboardKey.arrowUp) ||
          (control && key == LogicalKeyboardKey.home);
      final end =
          (meta && key == LogicalKeyboardKey.arrowDown) ||
          (control && key == LogicalKeyboardKey.end);
      if (start || end) {
        final at = end ? e.source.length : 0;
        return [SetSelection(shift ? e.selection.base : at, at)];
      }
      if (key == LogicalKeyboardKey.keyB) {
        return const [ToggleStyle(Style.strong)];
      }
      if (key == LogicalKeyboardKey.keyI) {
        return const [ToggleStyle(Style.emphasis)];
      }
      if (key == LogicalKeyboardKey.keyC) return const [];
      if (key == LogicalKeyboardKey.keyX) {
        return e.selection.isCollapsed ? const [] : const [DeleteBackward()];
      }
      if (key == LogicalKeyboardKey.keyV) {
        return clipboard == null ? const [] : [Paste(clipboard!)];
      }
      if (key == LogicalKeyboardKey.backspace ||
          key == LogicalKeyboardKey.delete ||
          key == LogicalKeyboardKey.enter ||
          key == LogicalKeyboardKey.tab) {
        return const [];
      }
      return null;
    }
    if (key == LogicalKeyboardKey.enter) return [Newline(paragraph: shift)];
    if (key == LogicalKeyboardKey.backspace) return const [DeleteBackward()];
    if (key == LogicalKeyboardKey.delete) return const [DeleteForward()];
    if (horizontal) {
      return [
        MoveCaret(
          direction,
          unit: alt ? MoveUnit.word : MoveUnit.grapheme,
          extend: shift,
        ),
      ];
    }
    if (key == LogicalKeyboardKey.tab) {
      if (alt) return null;
      if (!e.sourceMode && e.document.caretRow.kind == RowKind.tableCell) {
        return [MoveTableCell(backward: shift)];
      }
      return [shift ? const Outdent() : const Indent()];
    }
    return null;
  }

  /// Cocoa selectors for keys Flutter left unhandled.
  Future<void> _selectors() async {
    if (!_editorInput) return _idle();
    const selectors = {
      'insertNewline:': [Newline()],
      'deleteBackward:': [DeleteBackward()],
      'deleteForward:': [DeleteForward()],
      'moveLeft:': [MoveCaret(MoveDirection.backward)],
      'moveRight:': [MoveCaret(MoveDirection.forward)],
      'moveLeftAndModifySelection:': [
        MoveCaret(MoveDirection.backward, extend: true),
      ],
      'moveRightAndModifySelection:': [
        MoveCaret(MoveDirection.forward, extend: true),
      ],
      'cancelOperation:': <FlarkCommand>[],
      'insertTab:': <FlarkCommand>[],
      'moveUp:': null,
      'deleteToBeginningOfLine:': null,
      'selectAll:': null,
      'undo:': null,
    };
    final name = selectors.keys.elementAt(r.nextInt(selectors.length));
    final ref = c.editor.composing ? null : _reference();
    log.add('selector $name');
    await _platformCall('TextInputClient.performSelectors', [
      platform.client,
      [name],
    ]);
    _drain();
    await _settle(true);
    final intent = selectors[name];
    if (intent != null) _expect(ref, intent);
  }

  // --------------------------------------------------------------- pointer

  Future<void> _pointer({required bool touch}) async {
    if (!mounted) return _idle();
    final surface = find.byType(FlarkSurface);
    if (surface.evaluate().isEmpty) return _idle();
    final view = tester.getRect(_viewport);
    Offset point() => Offset(
      view.left + 4 + r.nextDouble() * (view.width - 8),
      view.top + 4 + r.nextDouble() * (view.height - 8),
    );
    final before = c.text;
    // A press ends a composition, which commits its text, typed.
    final committed = _committedSource();
    final kind = touch ? PointerDeviceKind.touch : PointerDeviceKind.mouse;
    final at = point();
    final gesture = r.nextInt(4);
    log.add(
      '${touch ? 'touch' : 'mouse'} ${['tap', 'double tap', 'drag', touch ? 'long press' : 'shift click'][gesture]} '
      'at ${at.dx.round()},${at.dy.round()}',
    );
    switch (gesture) {
      case 0:
        await tester.tapAt(at, kind: kind);
      case 1:
        await tester.tapAt(at, kind: kind);
        await tester.pump(const Duration(milliseconds: 50));
        await tester.tapAt(at, kind: kind);
      case 2:
        final g = await tester.startGesture(at, kind: kind);
        await g.moveTo(point());
        await tester.pump();
        await g.moveTo(point());
        await g.up();
      case 3:
        if (touch) {
          await tester.longPressAt(at);
        } else {
          await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
          await tester.tapAt(at, kind: kind);
          await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
        }
    }
    _drain();
    await tester.pump(kDoubleTapTimeout);
    _drain();
    await _settle(true);
    if (committed != null &&
        c.text != before &&
        c.text != committed &&
        !_taskToggle(before, c.text) &&
        !_taskToggle(committed, c.text)) {
      fail(
        '${_label()}: a press changed the document: '
        '${jsonEncode(before)} -> ${jsonEncode(c.text)}',
      );
    }
  }

  /// The editor's scroll viewport.
  Finder get _viewport => find
      .ancestor(
        of: find.byType(FlarkSurface),
        matching: find.byType(Scrollable),
      )
      .first;

  static bool _taskToggle(String before, String after) {
    if (before.length != after.length) return false;
    var changed = -1;
    for (var i = 0; i < before.length; i++) {
      if (before.codeUnitAt(i) != after.codeUnitAt(i)) {
        if (changed >= 0) return false;
        changed = i;
      }
    }
    return changed > 0 &&
        before[changed - 1] == '[' &&
        ' xX'.contains(before[changed]) &&
        ' xX'.contains(after[changed]);
  }

  // ------------------------------------------------------------ host side

  /// A toolbar or application command through the controller.
  Future<void> _toolbar() async {
    if (!mounted) return _idle();
    final e = c.editor;
    final len = e.source.length;
    final commands = <FlarkCommand>[
      ToggleStyle(
        [Style.strong, Style.emphasis, Style.code, Style.strikethrough][r
            .nextInt(4)],
      ),
      SetStyle(Style.strong, enabled: r.nextBool()),
      SetHeadingLevel(r.nextInt(7)),
      const Undo(),
      const Redo(),
      const Indent(),
      const Outdent(),
      InsertText(_randomText()),
      Paste(_chunks[r.nextInt(_chunks.length)]),
      SetSelection(r.nextInt(len + 1), r.nextInt(len + 1)),
      SetSelection.caret(r.nextInt(len + 1)),
      const SelectAll(),
      const SetLink('https://example.com/l'),
      const RemoveLink(),
      const ToggleTask(),
      const SetCodeLanguage('dart'),
      const Newline(),
      const DeleteBackward(),
      MoveCaret(
        r.nextBool() ? MoveDirection.forward : MoveDirection.backward,
        unit: MoveUnit.values[r.nextInt(MoveUnit.values.length)],
        extend: r.nextBool(),
      ),
    ];
    final pick = r.nextInt(commands.length + 5);
    if (pick == commands.length + 3) {
      // The application loads another document into the same controller.
      final text = _documents[r.nextInt(_documents.length)];
      log.add('application loads ${jsonEncode(text)}');
      final loaded = c.editor.loadMarkdown(text);
      await _settle(true);
      if (loaded) {
        expect(
          (c.text, c.editor.composing, c.value.composing),
          (text, false, TextRange.empty),
          reason: '${_label()}: loading a document',
        );
      }
      return;
    }
    if (pick == commands.length + 4) {
      // The application splices exact source, as a sync or merge does.
      final a = r.nextInt(len + 1), b = r.nextInt(len + 1);
      final from = min(a, b), to = max(a, b);
      final text = _randomText();
      log.add('application splices $from..$to with ${jsonEncode(text)}');
      final ref = _reference(ignoreComposition: true);
      final before = (c.text, c.editor.selection, c.editor.composing);
      final spliced = c.editor.replaceSourceRange(from, to, text);
      final after = (c.text, c.editor.selection, c.editor.composing);
      await _settle(true);
      // The kernel admits a splice before it commits a composition: a
      // refused one leaves no trace, and a composition in progress changes
      // nothing a kernel editor in the same state decides.
      if (!spliced) {
        expect(after, before, reason: '${_label()}: a refused splice');
      }
      if (ref == null) return;
      if (ref.replaceSourceRange(from, to, text) != spliced) {
        fail(
          '${_label()}: the host ${spliced ? 'accepted' : 'refused'} a '
          'splice a kernel editor in the same state '
          '${spliced ? 'refuses' : 'accepts'}',
        );
      }
      if (spliced) _compare(ref, 'application splice');
      return;
    }
    if (pick == commands.length) {
      log.add('toolbar source mode ${!e.sourceMode}');
      final source = c.text, selection = e.selection;
      // Switching ends a composition, which commits its text, typed.
      final committed = _committedSource();
      c.sourceMode(!e.sourceMode);
      await _settle(true);
      expect(c.editor.composing, isFalse, reason: _label());
      if (committed != null) {
        expect(
          c.text,
          committed,
          reason: '${_label()}: switching modes changed the document',
        );
      }
      if (c.editor.sourceMode && c.text == source) {
        expect(c.editor.selection, selection, reason: _label());
      }
      return;
    }
    if (pick == commands.length + 1) {
      final cancel = r.nextBool();
      log.add('controller finishComposition(cancel: $cancel)');
      c.finishComposition(cancel: cancel);
      await _settle(true);
      expect(c.editor.composing, isFalse, reason: _label());
      return;
    }
    if (pick == commands.length + 2) {
      // A link edit through the host's resource editor (Cmd/Ctrl+K).
      log.add('edit link');
      await _sendKey(
        LogicalKeyboardKey.keyK,
        modifier: target.apple
            ? LogicalKeyboardKey.metaLeft
            : LogicalKeyboardKey.controlLeft,
      );
      await _settle(true);
      return;
    }
    final command = commands[pick];
    final composing = e.composing;
    final ref = composing
        ? _committedReference()
        : _reference(ignoreComposition: true);
    final source = c.text, selection = e.selection;
    log.add('toolbar ${_name(command)}${composing ? ' while composing' : ''}');
    final accepted = c.command(command);
    await _settle(true);
    if (command is Undo || command is Redo || command is SelectAll) return;
    if (ref == null) return;
    if (composing && !accepted) {
      expect(
        (c.text, c.editor.selection),
        (source, selection),
        reason: '${_label()}: a refused command changed the composition',
      );
      return;
    }
    ref.apply(command);
    _compare(ref, 'toolbar ${_name(command)}');
  }

  Future<void> _focus() async {
    if (!mounted) return _idle();
    if (_editorFocused || FocusManager.instance.primaryFocus == null) {
      final composing = c.editor.composing;
      log.add(
        'focus moves to the other field${composing ? ' while composing' : ''}',
      );
      otherFocus.requestFocus();
      await tester.pump();
      _drain();
      await _settle(true);
      expect(
        c.editor.composing,
        isFalse,
        reason: '${_label()}: losing focus left the kernel composing',
      );
      if (platform.client != null && !platform.multiline && r.nextBool()) {
        // Typing there must not reach the document.
        final before = (c.text, c.editor.selection);
        final held = platform.held;
        final next = TextEditingValue(
          text: '${held.text}q',
          selection: TextSelection.collapsed(offset: held.text.length + 1),
        );
        log.add('  type in the other field');
        platform.held = next;
        await _sendValue(next);
        _drain();
        await _settle(true);
        expect(
          (c.text, c.editor.selection),
          before,
          reason: '${_label()}: input for another field reached the editor',
        );
      }
      if (r.nextInt(5) < 3) await _refocus();
      return;
    }
    await _refocus();
  }

  Future<void> _refocus() async {
    log.add('focus returns to the editor');
    final press = !(useOwnFocus && r.nextBool());
    if (!press) {
      ownFocus.requestFocus();
      await tester.pump();
    } else {
      // A press while the view still flings only stops it, as on a device,
      // so the user presses once it rests.
      final position = tester.state<ScrollableState>(_viewport).position;
      for (var i = 0; i < 40 && position.isScrollingNotifier.value; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      // Past the ends of lines, clear of a scrollbar at the edge.
      final view = tester.getRect(_viewport);
      await tester.tapAt(
        Offset(view.right - 48, view.center.dy),
        kind: target.touch ? PointerDeviceKind.touch : PointerDeviceKind.mouse,
      );
      await tester.pump(kDoubleTapTimeout);
    }
    _drain();
    await _settle(true);
    if (!readOnly) {
      expect(
        _editorFocused,
        isTrue,
        reason:
            '${_label()}: ${press ? 'a press' : 'a focus request'} left the '
            'focus on ${FocusManager.instance.primaryFocus}',
      );
      expect(
        platform.client != null && platform.multiline,
        isTrue,
        reason: '${_label()}: the focused editor holds no input connection',
      );
    }
  }

  Future<void> _close() async {
    if (!mounted || platform.client == null) return _idle();
    final editor = platform.multiline;
    log.add(
      'platform closes the connection${c.editor.composing ? ' while composing' : ''}',
    );
    final source = c.text, composing = c.editor.composing;
    // The composition the connection held commits its text, typed.
    final committed = composing ? _committedReference() : null;
    await _platformCall('TextInputClient.onConnectionClosed', [
      platform.client,
    ]);
    platform.client = null;
    platform.held = TextEditingValue.empty;
    _drain();
    await _settle(true);
    if (editor) {
      expect(
        c.editor.composing,
        isFalse,
        reason: '${_label()}: the composition outlived its connection',
      );
      if (composing) {
        _compare(committed, 'commit as the connection closes');
      } else {
        expect(c.text, source, reason: _label());
      }
    }
  }

  Future<void> _lifecycle() async {
    if (!mounted) return _idle();
    final away = target.touch
        ? AppLifecycleState.paused
        : (r.nextBool()
              ? AppLifecycleState.inactive
              : AppLifecycleState.hidden);
    log.add('app ${away.name} then resumed');
    if (web && platform.client != null) {
      // Leaving blurs the textarea, which closes the connection.
      await _platformCall('TextInputClient.onConnectionClosed', [
        platform.client,
      ]);
      platform.client = null;
      platform.held = TextEditingValue.empty;
    }
    await _setLifecycle(away);
    await tester.pump();
    _drain();
    _check('${_label()} away');
    await _setLifecycle(AppLifecycleState.resumed);
    await tester.pump();
    _drain();
    await _settle(true);
  }

  Future<void> _setLifecycle(AppLifecycleState state) =>
      tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SystemChannels.lifecycle.name,
        SystemChannels.lifecycle.codec.encodeMessage(state.toString()),
        (_) {},
      );

  Future<void> _rebuild() async {
    if (!mounted) return _idle();
    final previous = c;
    switch (readOnly && r.nextInt(3) != 0 ? 0 : r.nextInt(6)) {
      case 0:
        readOnly = !readOnly;
        log.add('rebuild readOnly=$readOnly');
      case 1:
        theme = (theme + 1 + r.nextInt(_themes.length - 1)) % _themes.length;
        log.add('rebuild theme $theme');
      case 2:
        final shown = spare;
        spare = c;
        c = shown;
        log.add(
          'rebuild with the other controller: ${jsonEncode(c.text.length > 80 ? '${c.text.substring(0, 80)}…' : c.text)}',
        );
      case 3:
        useOwnFocus = !useOwnFocus;
        log.add('rebuild own focus node=$useOwnFocus');
      case 4:
        popoverField = !popoverField;
        log.add('rebuild popover field=$popoverField');
      default:
        showToolbar = !showToolbar;
        log.add('rebuild toolbar=$showToolbar');
    }
    final composing = previous.editor.composing;
    await tester.pumpWidget(_app());
    await tester.pump();
    _drain();
    await _settle(true);
    if (readOnly || !identical(previous, c)) {
      expect(
        previous.editor.composing,
        isFalse,
        reason:
            '${_label()}: the composition (open: $composing) outlived the '
            'connection the rebuild closed',
      );
    }
    if (useOwnFocus && !_editorFocused && r.nextBool()) {
      ownFocus.requestFocus();
      await tester.pump();
      _drain();
      await _settle(true);
    }
  }

  Future<void> _semantics() async {
    if (!mounted) return _idle();
    final finder = find.byType(FlarkSurface);
    if (finder.evaluate().isEmpty) return _idle();
    await tester.pump();
    _drain();
    final node = tester.getSemantics(finder);
    final value = node.getSemanticsData().value;
    if (r.nextBool()) {
      final boundaries = _boundaries(value);
      final a = boundaries[r.nextInt(boundaries.length)];
      final b = r.nextBool() ? a : boundaries[r.nextInt(boundaries.length)];
      log.add('accessibility selects $a..$b');
      tester.binding.performSemanticsAction(
        SemanticsActionEvent(
          viewId: tester.view.viewId,
          nodeId: node.id,
          type: SemanticsAction.setSelection,
          arguments: {'base': a, 'extent': b},
        ),
      );
    } else {
      final boundaries = _boundaries(value);
      var a = boundaries[r.nextInt(boundaries.length)], b = a;
      final shape = r.nextInt(4);
      final words = shape == 0
          ? _words(value, boundaries)
          : const <List<int>>[];
      final String text;
      if (words.isNotEmpty) {
        // Part of a word, replaced by letters or deleted, as "replace X with
        // Y" or a correction names it.
        final word = words[r.nextInt(words.length)];
        final from = r.nextInt(word.length - 1);
        a = word[from];
        b = word[from + 1 + r.nextInt(word.length - 1 - from)];
        text = const ['', 'x', 'ab', 'é', 'Zz', '中'][r.nextInt(6)];
      } else {
        if (shape == 1) b = boundaries[r.nextInt(boundaries.length)];
        if (b < a) (a, b) = (b, a);
        text = r.nextInt(4) == 0
            ? ''
            : r.nextBool()
            ? const ['x', 'ab', 'é', 'Zz'][r.nextInt(4)]
            : _randomText();
      }
      log.add('accessibility sets text: $a..$b -> ${jsonEncode(text)}');
      // An edit ends a composition, which commits its text, typed; a refused
      // one leaves it composing.
      final composed = c.text, source = _committedSource();
      final requested = value.replaceRange(a, b, text);
      // A notice left from an earlier event explains nothing here.
      c.notice = null;
      tester.binding.performSemanticsAction(
        SemanticsActionEvent(
          viewId: tester.view.viewId,
          nodeId: node.id,
          type: SemanticsAction.setText,
          arguments: requested,
        ),
      );
      _drain();
      await _settle(true);
      // Letters replacing letters mean what they say: the document shows the
      // requested text, but for the delimiter padding a table cell's text
      // ends with, which is not painted (an emptied cell has none), or
      // refuses with a notice. Letters inserted may also come with the
      // structure a missing table cell needs around them, which can change
      // how Markdown reads its neighbors (a line typed into the blank line
      // before a link definition). Other text can start or end Markdown
      // structure.
      final removed = value.substring(a, b);
      if (source != null &&
          _letters.hasMatch('$text${removed}x') &&
          !c.editor.sourceMode &&
          !readOnly &&
          mounted) {
        await tester.pump();
        final shown = tester.getSemantics(finder).getSemanticsData().value;
        final (cut, added) = _difference(source, c.text);
        final cell =
            !c.editor.sourceMode &&
            c.editor.document.caretRow.kind == RowKind.tableCell;
        String unpadded(String value) =>
            value.replaceAll(RegExp(r'[ \t]+(?=\n|$)'), '');
        final explained =
            shown == requested ||
            (cell && unpadded(shown) == unpadded(requested)) ||
            ((c.text == source || c.text == composed) && c.notice != null) ||
            (a == b && c.text != source && cut.isEmpty && added.contains(text));
        if (!explained) {
          fail(
            '${_label()}: accessibility asked for ${jsonEncode(requested)}, '
            'the document shows ${jsonEncode(shown)} '
            '(source ${jsonEncode(c.text)}, notice ${c.notice})',
          );
        }
        _count('oracle accessibility ${a == b ? 'insertion' : 'replacement'}');
      }
      return;
    }
    _drain();
    await _settle(true);
  }

  Future<void> _requestExisting() async {
    if (!mounted || platform.client == null) return _idle();
    log.add('platform requests the existing input state');
    await _platformCall('TextInputClient.requestExistingInputState', null);
    _drain();
    await _settle(true);
  }

  /// Several platform events before the next frame.
  Future<void> _burst() async {
    if (target == _Target.android && r.nextBool()) return _batch();
    log.add('burst');
    for (var i = 1 + r.nextInt(4); i > 0; i--) {
      switch (r.nextInt(4)) {
        case 0:
          await _backspace(settle: false);
        case 1:
          await _platformSelect(settle: false);
        default:
          await _type(_typed[r.nextInt(_typed.length)], settle: false);
      }
    }
    await _settle(true);
  }

  /// Characters an input method commits inside one batch edit: one delta
  /// batch, applied as one value.
  Future<void> _batch() async {
    if (!_editorInput || _platformComposing) return _idle();
    final ref = _reference();
    var value = platform.held;
    final deltas = <Map<String, Object?>>[];
    final typed = <String>[];
    for (var i = 2 + r.nextInt(3); i > 0; i--) {
      final text = _typed[r.nextInt(_typed.length)];
      final s = value.selection.start, e = value.selection.end;
      final next = TextEditingValue(
        text: value.text.replaceRange(s, e, text),
        selection: TextSelection.collapsed(offset: s + text.length),
      );
      deltas.add(_delta(value.text, s, e, text, next));
      typed.add(text);
      value = next;
    }
    log.add('batch ${jsonEncode(typed)}');
    platform.held = value;
    await _sendDeltas(deltas);
    _drain();
    await _settle(true);
    // Typed together, without a line break, they are typed text.
    if (!typed.contains('\n')) _expect(ref, [InsertText(typed.join())]);
  }

  Future<void> _idle() async {
    await _settle(true);
  }

  // ------------------------------------------------------- the platform

  Future<void> _platformCall(String method, Object? args) =>
      tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SystemChannels.textInput.name,
        SystemChannels.textInput.codec.encodeMethodCall(
          MethodCall(method, args),
        ),
        (_) {},
      );

  Future<void> _sendValue(TextEditingValue value, {int? client}) =>
      _platformCall('TextInputClient.updateEditingState', [
        client ?? platform.client,
        value.toJSON(),
      ]);

  Future<void> _sendDeltas(List<Map<String, Object?>> deltas, {int? client}) =>
      _platformCall('TextInputClient.updateEditingStateWithDeltas', [
        client ?? platform.client,
        {'deltas': deltas},
      ]);

  static Map<String, Object?> _delta(
    String oldText,
    int start,
    int end,
    String text,
    TextEditingValue after,
  ) => {
    'oldText': oldText,
    'deltaText': text,
    'deltaStart': start,
    'deltaEnd': end,
    'selectionBase': after.selection.baseOffset,
    'selectionExtent': after.selection.extentOffset,
    'selectionAffinity': 'TextAffinity.downstream',
    'selectionIsDirectional': false,
    'composingBase': after.composing.isValid ? after.composing.start : -1,
    'composingExtent': after.composing.isValid ? after.composing.end : -1,
  };

  static Map<String, Object?> _nonText(String text, TextEditingValue after) =>
      _delta(text, -1, -1, '', after);

  /// The platform applies its own edit, then tells the host: a delta batch
  /// natively, the textarea's full value on the web.
  Future<void> _deliver(
    TextEditingValue before,
    TextEditingValue after,
    int start,
    int end,
    String text,
  ) async {
    platform.held = after;
    if (web) {
      platform.held = _textarea(after);
      await _sendValue(platform.held);
    } else {
      await _sendDeltas([_delta(before.text, start, end, text, after)]);
    }
    _drain();
  }

  /// A textarea keeps LF line breaks only and clamps a selection set past
  /// its end, as Chrome does.
  static TextEditingValue _textarea(TextEditingValue value) {
    if (!value.text.contains('\r')) return value;
    final text = value.text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    int clamp(int offset) => offset < 0 ? offset : min(offset, text.length);
    return TextEditingValue(
      text: text,
      selection: TextSelection(
        baseOffset: clamp(value.selection.baseOffset),
        extentOffset: clamp(value.selection.extentOffset),
      ),
      composing: value.composing.isValid
          ? TextRange(
              start: clamp(value.composing.start),
              end: clamp(value.composing.end),
            )
          : TextRange.empty,
    );
  }

  /// Applies what the host told the platform since the last look.
  void _drain() {
    final calls = tester.testTextInput.log;
    var cleared = false;
    for (; platform.logIndex < calls.length; platform.logIndex++) {
      final call = calls[platform.logIndex];
      switch (call.method) {
        case 'TextInput.setClient':
          // The host replaced its connection at once: a rebased input
          // context, a rebuild, or a request for the existing state.
          if (cleared) _count('editor reconnection');
          final args = call.arguments as List;
          platform.client = args[0] as int;
          final config = args[1] as Map;
          platform.multiline =
              (config['inputType'] as Map)['name'] == 'TextInputType.multiline';
          platform.held = TextEditingValue.empty;
          if (platform.multiline) {
            platform.clients.add(platform.client!);
            _count('editor connection');
          }
        case 'TextInput.clearClient':
          cleared = platform.multiline;
          platform.client = null;
          platform.held = TextEditingValue.empty;
        case 'TextInput.setEditingState':
          var value = TextEditingValue.fromJSON(
            Map<String, dynamic>.from(call.arguments as Map),
          );
          if (web) value = _textarea(value);
          if (platform.multiline && platform.held.selection.isValid) {
            platform.history.add((platform.client!, platform.held));
            if (platform.history.length > 8) platform.history.removeAt(0);
          }
          platform.held = value;
      }
    }
  }

  // ---------------------------------------------------------------- oracle

  String _label() => 'step $_labelStep (${log.isEmpty ? '' : log.last.trim()})';

  Future<void> _settle(bool pump) async {
    if (pump) {
      await tester.pump();
      _drain();
    }
    _check(_label());
  }

  void _check(String label) {
    if (Platform.environment['FLARK_INPUT_TRACE'] != null) {
      final e = c.editor, at = e.selection.extent;
      final from = max(0, at - 24), to = min(e.source.length, at + 24);
      final held = platform.held;
      // ignore: avoid_print
      print(
        '$label | ${e.selection} ${jsonEncode(e.source.substring(from, to))} '
        '| platform ${held.selection.baseOffset}..${held.selection.extentOffset}'
        '${e.composing ? ' composing' : ''}${c.notice == null ? '' : ' notice: ${c.notice}'}',
      );
    }
    final error = tester.takeException();
    if (error != null) fail('$label: reported $error');
    _checkController(c, label);
    _checkController(spare, label);
    _checkPlatform(label);
  }

  void _checkController(FlarkController controller, String label) {
    final e = controller.editor;
    final text = e.source, sel = e.selection;
    bool inside(int o) => o >= 0 && o <= text.length;
    expect(inside(sel.base) && inside(sel.extent), isTrue, reason: label);
    if (!e.sourceMode) {
      final doc = e.document;
      final whole =
          !sel.isCollapsed && sel.start == 0 && sel.end == text.length;
      // A caret in an unwritten cell names the cell by its index.
      final unwritten = doc.projection.isMissingCell(sel.tableCell);
      expect(
        whole || unwritten || doc.isLegal(sel.base),
        isTrue,
        reason: '$label: base $sel legal',
      );
      expect(
        whole || unwritten || doc.isLegal(sel.extent),
        isTrue,
        reason: '$label: extent $sel legal',
      );
    }
    final composing = controller.value.composing;
    if (composing.isValid) {
      expect(
        e.composing,
        isTrue,
        reason: '$label: a composing range $composing without a composition',
      );
      expect(
        composing.start >= 0 &&
            composing.start <= composing.end &&
            composing.end <= text.length,
        isTrue,
        reason: '$label: composing $composing within the source',
      );
    }
  }

  /// The platform holds the host's window of the document: LF text, the
  /// document's selection and composing range in its coordinates.
  void _checkPlatform(String label) {
    if (!mounted || platform.client == null || !platform.multiline) return;
    final window = _window();
    if (window == null) {
      fail(
        '$label: the platform holds ${_describe(platform.held)}, which is no '
        'window of the document ${jsonEncode(c.text)} at ${_describe(c.value)}',
      );
    }
  }

  /// Where the platform's buffer lies in the document, or null when it
  /// does not mirror it.
  _Window? _window() {
    final held = platform.held;
    if (!held.selection.isValid) return null;
    final value = c.value;
    final crs = _crlfs(c.text);
    final lf = c.text.replaceAll('\r\n', '\n');
    final start =
        _toLf(crs, value.selection.extentOffset) - held.selection.extentOffset;
    if (start < 0 || start + held.text.length > lf.length) return null;
    if (lf.substring(start, start + held.text.length) != held.text) {
      return null;
    }
    if (held.selection.baseOffset + start !=
        _toLf(crs, value.selection.baseOffset)) {
      return null;
    }
    final composing = value.composing.isValid && !value.composing.isCollapsed
        ? TextRange(
            start: _toLf(crs, value.composing.start) - start,
            end: _toLf(crs, value.composing.end) - start,
          )
        : TextRange.empty;
    final heldComposing = held.composing.isValid && !held.composing.isCollapsed
        ? held.composing
        : TextRange.empty;
    if (composing != heldComposing) return null;
    return _Window(start, crs);
  }

  /// A reference editor in the state the open composition began in, with
  /// what it shows typed, as its commit types it. Null without a reference
  /// or a composition [_compose] opened.
  FlarkEditor? _typedPreedit() {
    final preedit = _preedit, pre = preedit?.pre;
    final typed = pre == null ? null : _reference(from: pre);
    if (typed == null) return null;
    if (preedit!.shown.isNotEmpty) {
      typed.apply(InsertText(preedit.shown));
    } else if (preedit.selected) {
      typed.apply(const DeleteBackward());
    }
    return typed;
  }

  /// The source the open composition's commit leaves, or the source when
  /// none is open. Null when no reference can tell.
  String? _committedSource() =>
      c.editor.composing ? _typedPreedit()?.source : c.text;

  /// A reference editor in the state the open composition's commit leaves:
  /// the composed state, where typing its text reads it so too (the
  /// selection stays where the platform left it), or the typed one.
  FlarkEditor? _committedReference() {
    final typed = _typedPreedit();
    if (typed == null) return null;
    return typed.source == c.text ? _reference(ignoreComposition: true) : typed;
  }

  /// A fresh kernel editor in the controller's state, or null when the
  /// state holds something a fresh editor cannot share: a composition, an
  /// unwritten table cell, a pending style or a selection it legalizes
  /// differently.
  FlarkEditor? _reference({_State? from, bool ignoreComposition = false}) {
    final e = c.editor;
    final state =
        from ??
        (e.composing && !ignoreComposition
            ? null
            : _State(e.source, e.selection, e.sourceMode, e.typingContext));
    if (state == null) {
      _count('no reference: composing');
      return null;
    }
    if (state.selection.tableCell != null) {
      _count('no reference: table cell');
      return null;
    }
    final ref = FlarkEditor(
      backend,
      text: state.source,
      caret: state.selection.extent,
      codeEditing: e.codeEditing,
      syncLimit: e.syncLimit,
      liveLimits: e.liveLimits,
      sourceLimit: e.sourceLimit,
      clock: () => _time,
    );
    if (state.sourceMode && !ref.sourceMode) ref.setSourceMode(true);
    ref.apply(SetSelection(state.selection.base, state.selection.extent));
    if (ref.selection != state.selection ||
        ref.sourceMode != state.sourceMode ||
        ref.typingContext != state.typingContext) {
      _count(
        'no reference: ${ref.selection != state.selection
            ? 'selection'
            : ref.sourceMode != state.sourceMode
            ? 'mode'
            : 'pending'}',
      );
      return null;
    }
    return ref;
  }

  void _expect(FlarkEditor? ref, List<FlarkCommand> commands) {
    if (ref == null) return;
    for (final command in commands) {
      ref.apply(command);
    }
    _compare(ref, commands.map(_name).join(', '));
  }

  void _compare(FlarkEditor? ref, String what) {
    if (ref == null) return;
    _count('oracle ${what.split(RegExp(r'[ (]')).first}');
    final e = c.editor;
    if (e.source != ref.source ||
        e.selection != ref.selection ||
        e.typingContext != ref.typingContext) {
      fail(
        '${_label()}: expected $what to leave ${jsonEncode(ref.source)} '
        '${ref.selection} context ${ref.typingContext}, the host left '
        '${jsonEncode(e.source)} ${e.selection} context ${e.typingContext}'
        '${c.notice == null ? '' : ' (notice: ${c.notice})'}',
      );
    }
  }

  static String _describe(TextEditingValue v) =>
      '${jsonEncode(v.text)} selection ${v.selection.baseOffset}..'
      '${v.selection.extentOffset} composing ${v.composing.start}..'
      '${v.composing.end}';

  static String _name(FlarkCommand command) => switch (command) {
    InsertText(:final text) => 'InsertText(${jsonEncode(text)})',
    Paste(:final text) => 'Paste(${jsonEncode(text)})',
    DeleteBackward(:final word) => 'DeleteBackward(word: $word)',
    DeleteForward(:final word) => 'DeleteForward(word: $word)',
    Newline(:final paragraph) => 'Newline(paragraph: $paragraph)',
    ReplaceRange(:final start, :final end, :final text) =>
      'ReplaceRange($start, $end, ${jsonEncode(text)})',
    SetSelection(:final base, :final extent) => 'SetSelection($base, $extent)',
    MoveCaret(:final direction, :final unit, :final extend) =>
      'MoveCaret(${direction.name}, ${unit.name}, extend: $extend)',
    ToggleStyle(:final style) => 'ToggleStyle($style)',
    SetStyle(:final style, :final enabled) => 'SetStyle($style, $enabled)',
    SetHeadingLevel(:final level) => 'SetHeadingLevel($level)',
    MoveTableCell(:final backward) => 'MoveTableCell(backward: $backward)',
    _ => command.runtimeType.toString(),
  };
}

String _lf(String text) => text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');

/// What [after] removed from [before] and inserted in its place, between
/// their common prefix and suffix.
(String, String) _difference(String before, String after) {
  var start = 0;
  while (start < before.length &&
      start < after.length &&
      before.codeUnitAt(start) == after.codeUnitAt(start)) {
    start++;
  }
  var end = 0;
  while (end < before.length - start &&
      end < after.length - start &&
      before.codeUnitAt(before.length - 1 - end) ==
          after.codeUnitAt(after.length - 1 - end)) {
    end++;
  }
  return (
    before.substring(start, before.length - end),
    after.substring(start, after.length - end),
  );
}

/// The state a reference editor starts from.
class _State {
  const _State(
    this.source,
    this.selection,
    this.sourceMode,
    this.typingContext,
  );
  final String source;
  final FlarkSelection selection;
  final bool sourceMode;
  final int typingContext;
}

/// The platform buffer's place in the document: its first LF offset, and
/// the document's CRLF positions.
class _Window {
  _Window(this.start, this.crs);
  final int start;
  final List<int> crs;

  /// The source offset of [local], an offset in the platform's buffer. An
  /// offset at a line break stays before its CR.
  int source(int local) => _fromLf(crs, start + local);
}

List<int> _crlfs(String text) => [
  for (var i = text.indexOf('\r\n'); i >= 0; i = text.indexOf('\r\n', i + 2)) i,
];

int _toLf(List<int> crs, int offset) {
  var n = 0;
  while (n < crs.length && crs[n] < offset) {
    n++;
  }
  return offset - n;
}

int _fromLf(List<int> crs, int offset) {
  var n = 0;
  while (n < crs.length && crs[n] - n < offset) {
    n++;
  }
  return offset + n;
}
