import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:flutter/foundation.dart';

typedef _Key = (String, CodeLanguage);
typedef CodeColorRequest =
    Future<CodeAnalysis?> Function(
      String source, {
      required CodeLanguage language,
    });

/// A single editor's optional decoration lane. Only exact current text and
/// language may adopt colors. Selection, document revision and history never
/// change here. Cache and pending work are bounded independently of source size.
final class FlarkCodeColors extends ChangeNotifier {
  FlarkCodeColors(this.editor, {Uri? workerUri, Uri? wasmUri}) {
    editor.addListener(_refresh);
    _refresh();
    unawaited(_start(workerUri, wasmUri));
  }

  @visibleForTesting
  FlarkCodeColors.withWorker(
    this.editor,
    CodeColorRequest request,
    void Function() dispose,
  ) : _request = request,
      _disposeWorker = dispose {
    editor.addListener(_refresh);
    _refresh();
  }

  final FlarkEditor editor;
  CodeColorRequest? _request;
  void Function()? _disposeWorker;
  final _cache = <_Key, CodeHighlight>{};
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
    if (_disposed || listEquals(next, _visibleRows)) return;
    _visibleRows = next;
    _refresh();
  }

  CodeHighlight highlight(String text, String info) {
    final key = _key(text, info);
    if (key == null) {
      return CodeHighlight(null, [CodeToken(0, text.length, null)]);
    }
    final cached = _cache.remove(key);
    if (cached != null) {
      _cache[key] = cached;
      return cached;
    }
    return CodeHighlight(key.$2.name, [CodeToken(0, text.length, null)]);
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
        if (wanted.length == 32 || units + row.text.length > 65536) break;
        wanted.add(key);
        units += row.text.length;
      }
    }
    _wanted = wanted;
    _schedule();
  }

  void _schedule() {
    if (_disposed || _failed || _request == null) return;
    final missing = _wanted.where((key) => !_cache.containsKey(key));
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
      if (analysis == null ||
          analysis.source != key.$1 ||
          analysis.language != key.$2) {
        throw StateError('Color worker returned no matching analysis');
      }
      // A completed response has no authority over a replaced/deleted fence.
      if (!_wanted.contains(key)) return;
      _cache[key] = CodeHighlight(key.$2.name, [
        for (final span in analysis.spans)
          CodeToken(
            span.start,
            span.end,
            span.scopes.lastOrNull?.split('.').first,
          ),
      ]);
      _cachedUnits += key.$1.length;
      while (_cache.length > 32 || _cachedUnits > 65536) {
        final oldest = _cache.keys.first;
        _cachedUnits -= oldest.$1.length;
        _cache.remove(oldest);
      }
      _pending = null;
      _revision++;
      notifyListeners();
      _schedule();
    } catch (error) {
      if (_disposed || serial != _serial) return;
      _failure = error;
      _failed = true;
      _pending = null;
      _disposeWorker?.call();
    }
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    editor.removeListener(_refresh);
    _disposeWorker?.call();
    _cache.clear();
    _wanted = [];
    super.dispose();
  }
}
