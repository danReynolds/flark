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

/// A single editor's optional decoration lane. Only exact current text and
/// language may adopt colors. Selection, document revision and history never
/// change here. Cache and pending work are bounded independently of source size.
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

  CodeHighlight highlight(String text, String info) =>
      _decoration(text, info)?.highlight ??
      CodeHighlight(_key(text, info)?.$2.name, [
        CodeToken(0, text.length, null),
      ]);

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
        if (key == null || wanted.contains(key)) continue;
        if (wanted.length == _maxSnippets) break;
        if (units + row.text.length > _maxUnits) continue;
        wanted.add(key);
        units += row.text.length;
      }
    }
    _wanted = wanted;
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
    _wanted = [];
    _unavailable.clear();
    _listeners.clear();
  }
}

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
