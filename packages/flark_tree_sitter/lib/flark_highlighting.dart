import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'flark_tree_sitter.dart';
import 'highlight_worker.dart';

typedef _Key = (String, CodeLanguage);
typedef CodeColorRequest =
    Future<CodeAnalysis?> Function(
      String source, {
      required CodeLanguage language,
    });

/// A single editor's optional decoration lane. Exact analyses belong to one
/// text and language. Until the current text's analysis arrives, a changed
/// fence paints its previous colors shifted through the edit
/// ([shiftCodeHighlight]) instead of flashing plain. Colors never move glyphs,
/// and selection, document revision and history never change here. Cache and
/// pending work are bounded independently of source size.
final class FlarkCodeHighlighting {
  FlarkCodeHighlighting(this.editor, {Uri? workerUri, Uri? wasmUri}) {
    editor.addListener(_refresh);
    _refresh();
    unawaited(_start(workerUri, wasmUri));
  }

  FlarkCodeHighlighting.withWorker(
    this.editor,
    CodeColorRequest request,
    void Function() dispose,
  ) : _request = request,
      _disposeWorker = dispose {
    editor.addListener(_refresh);
    _refresh();
  }

  /// Borrows the editor; owns the worker and disposes it exactly once.
  FlarkCodeHighlighting.fromWorker(
    FlarkEditor editor,
    CodeHighlightWorker worker,
  ) : this.withWorker(editor, worker.analyze, worker.dispose);

  static const _maxSnippets = 32, _maxUnits = 65536;

  final FlarkEditor editor;
  final _listeners = <void Function()>{};
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final listener in List.of(_listeners)) {
      if (_listeners.contains(listener)) listener();
    }
  }

  void _closeWorker() {
    final dispose = _disposeWorker;
    _disposeWorker = null;
    dispose?.call();
  }

  CodeColorRequest? _request;
  void Function()? _disposeWorker;
  final _cache = <_Key, _Decoration>{};

  /// Provisional colors for wanted keys that have no exact result yet.
  final _shifted = <_Key, CodeHighlight>{};

  /// The key each code row showed at the last refresh, to find the colors an
  /// edited fence had before its text changed.
  var _rowKeys = <int, _Key>{};
  final _unavailable = <_Key>{};
  List<_Key> _wanted = [];
  List<int>? _visibleRows;
  _Key? _pending;
  var _serial = 0, _cachedUnits = 0;
  bool _disposed = false, _failed = false;
  int _revision = 0;
  int get revision => _revision;
  Object? _failure;
  Object? get failure => _failure;

  Future<void> _start(Uri? workerUri, Uri? wasmUri) async {
    try {
      final worker = await CodeHighlightWorker.start(
        workerUri: workerUri,
        wasmUri: wasmUri,
      );
      if (_disposed) {
        worker.dispose();
        return;
      }
      _request = worker.analyze;
      _disposeWorker = worker.dispose;
      _schedule();
    } catch (error) {
      if (!_disposed) {
        _failure = error;
        _failed = true;
        _notify();
      }
    }
  }

  String _language(String text, String info) {
    return editor.codeEditing?.resolveLanguage(text, info) ??
        codeLanguageName(info);
  }

  _Key? _key(String text, String info) {
    if (text.length > CodeAnalyzer.maxCodeUnits) return null;
    final selected = codeLanguage(_language(text, info));
    return selected == CodeLanguage.plain ? null : (text, selected);
  }

  /// Called after layout/scroll so visible fences outrank offscreen work. Until
  /// a surface reports its viewport, warm the active fence and bounded document.
  void setVisibleRows(Iterable<int> rows) {
    final next = rows.toList();
    if (_disposed || _sameRows(next, _visibleRows)) return;
    _visibleRows = next;
    _refresh();
  }

  _Decoration? _decoration(String text, String info) {
    final key = _key(text, info);
    final value = _cache.remove(key);
    if (value != null) _cache[key!] = value;
    return value;
  }

  /// Only an exact source/language pair can supply token ranges to paint.
  CodeAnalysis? analysis(String text, String info) =>
      _decoration(text, info)?.analysis;

  CodeHighlight highlight(String text, String info) {
    final key = _key(text, info);
    return colors(text, info) ??
        CodeHighlight(key?.$2.name, [CodeToken(0, text.length, null)]);
  }

  /// Colors to paint now: the exact analysis of this text when available,
  /// else this fence's previous colors shifted through the edit, else null.
  CodeHighlight? colors(String text, String info) {
    final exact = _decoration(text, info)?.highlight;
    if (exact != null) return exact;
    final key = _key(text, info);
    return key == null ? null : _shifted[key];
  }

  static bool _sameRows(List<int> a, List<int>? b) {
    if (b == null || a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  void _refresh() {
    if (_disposed) return;
    final snapshot = editor.snapshot;
    final wanted = <_Key>[];
    final rowKeys = <int, _Key>{};
    if (snapshot is FlarkLiveSnapshot) {
      final active = snapshot.document.rowAt(snapshot.selection.extent);
      var units = 0;
      final rows = snapshot.projection.rows;
      for (final row in [
        active,
        ..._visibleRows == null
            ? rows
            : [
                for (final index in _visibleRows!)
                  if (index >= 0 && index < rows.length) rows[index],
              ],
      ]) {
        if (row.kind != RowKind.codeBlock) continue;
        final info = row.fenced
            ? snapshot.source.substring(row.codeInfoStart, row.codeInfoEnd)
            : '';
        final key = _key(row.text, info);
        if (key != null) rowKeys[row.index] = key;
        if (key == null || wanted.contains(key)) continue;
        if (wanted.length == _maxSnippets) break;
        if (units + row.text.length > _maxUnits) continue;
        wanted.add(key);
        units += row.text.length;
      }
    }
    // A fence whose text just changed keeps the colors it showed, shifted
    // through the edit, until the exact analysis of its new text arrives.
    for (final MapEntry(key: index, value: key) in rowKeys.entries) {
      if (_cache.containsKey(key) || _shifted.containsKey(key)) continue;
      final previous = _rowKeys[index];
      if (previous == null || previous == key || previous.$2 != key.$2) {
        continue;
      }
      final colors = _cache[previous]?.highlight ?? _shifted[previous];
      if (colors == null) continue;
      final shifted = shiftCodeHighlight(colors, previous.$1, key.$1);
      if (shifted != null) _shifted[key] = shifted;
    }
    _rowKeys = rowKeys;
    _wanted = wanted;
    _shifted.removeWhere((key, _) => !wanted.contains(key));
    _unavailable.removeWhere((key) => !wanted.contains(key));
    _schedule();
  }

  void _schedule() {
    if (_disposed || _failed || _request == null) return;
    final missing = _wanted.where(
      (key) => !_cache.containsKey(key) && !_unavailable.contains(key),
    );
    final next = missing.firstOrNull;
    if (next == _pending) return;
    final serial = ++_serial;
    _pending = next;
    if (next == null) return;
    unawaited(_run(next, serial));
  }

  Future<void> _run(_Key key, int serial) async {
    try {
      final analysis = await _request!(key.$1, language: key.$2);
      if (_disposed || serial != _serial) return;
      if (analysis == null) {
        // A refused/cancelled current job must not spin forever or prevent the
        // remaining snippets from receiving colors. A future edit may retry it.
        _unavailable.add(key);
        _pending = null;
        _schedule();
        return;
      }
      if (analysis.source != key.$1 || analysis.language != key.$2) {
        throw StateError('Color worker returned no matching analysis');
      }
      // A completed response has no authority over a replaced/deleted fence.
      if (!_wanted.contains(key)) return;
      _cache[key] = _Decoration(analysis);
      _shifted.remove(key);
      _cachedUnits += key.$1.length;
      while (_cache.length > _maxSnippets || _cachedUnits > _maxUnits) {
        final oldest = _cache.keys.firstWhere(
          (candidate) => !_wanted.contains(candidate),
          orElse: () => _cache.keys.first,
        );
        _cachedUnits -= oldest.$1.length;
        _cache.remove(oldest);
      }
      _pending = null;
      _revision++;
      _notify();
      _schedule();
    } catch (error) {
      if (_disposed || serial != _serial) return;
      _failure = error;
      _failed = true;
      _pending = null;
      _closeWorker();
      _notify();
    }
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    editor.removeListener(_refresh);
    _closeWorker();
    _cache.clear();
    _shifted.clear();
    _rowKeys = {};
    _wanted = [];
    _unavailable.clear();
    _listeners.clear();
  }
}

/// Colors for [after] derived from [colors] of [before], for the frames
/// between an edit and the exact analysis of the edited text. Text outside the
/// edit keeps its colors. A short single-line insertion continues the token
/// it extends; a larger one stays plain. Returns null when the edit kept no
/// text, so a replaced snippet paints plain rather than borrowing colors.
CodeHighlight? shiftCodeHighlight(
  CodeHighlight colors,
  String before,
  String after,
) {
  final limit = before.length < after.length ? before.length : after.length;
  var prefix = 0, suffix = 0;
  while (prefix < limit &&
      before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
    prefix++;
  }
  while (suffix < limit - prefix &&
      before.codeUnitAt(before.length - 1 - suffix) ==
          after.codeUnitAt(after.length - 1 - suffix)) {
    suffix++;
  }
  // Token edges must not split a surrogate pair on either side of the edit.
  if (prefix > 0 && _isHighSurrogate(before.codeUnitAt(prefix - 1))) prefix--;
  if (suffix > 0 && _isLowSurrogate(after.codeUnitAt(after.length - suffix))) {
    suffix--;
  }
  if (prefix + suffix == 0) return null;
  final removedEnd = before.length - suffix,
      insertedEnd = after.length - suffix;
  final shift = after.length - before.length;
  final tokens = <CodeToken>[];
  void add(int start, int end, String? kind) {
    if (end <= start) return;
    final last = tokens.lastOrNull;
    if (last != null && last.kind == kind && last.end == start) {
      tokens[tokens.length - 1] = CodeToken(last.start, end, kind);
    } else {
      tokens.add(CodeToken(start, end, kind));
    }
  }

  String? continued;
  for (final token in colors.tokens) {
    if (token.start < prefix) {
      add(token.start, token.end < prefix ? token.end : prefix, token.kind);
    }
    // The token holding the character before the edit, or the first token.
    if (prefix == 0
        ? token.start == 0
        : token.start < prefix && prefix <= token.end) {
      continued = token.kind;
    }
  }
  final inserted = after.substring(prefix, insertedEnd);
  final continues = inserted.length <= 32 && !inserted.contains('\n');
  add(prefix, insertedEnd, continues ? continued : null);
  for (final token in colors.tokens) {
    if (token.end <= removedEnd) continue;
    add(
      (token.start > removedEnd ? token.start : removedEnd) + shift,
      token.end + shift,
      token.kind,
    );
  }
  return CodeHighlight(colors.language, tokens);
}

bool _isHighSurrogate(int unit) => unit >= 0xd800 && unit <= 0xdbff;
bool _isLowSurrogate(int unit) => unit >= 0xdc00 && unit <= 0xdfff;

final class _Decoration {
  _Decoration(this.analysis)
    : highlight = CodeHighlight(analysis.language.name, [
        for (final span in analysis.spans)
          CodeToken(
            span.start,
            span.end,
            span.scopes.lastOrNull?.split('.').first,
          ),
      ]);
  final CodeAnalysis analysis;
  final CodeHighlight highlight;
}
