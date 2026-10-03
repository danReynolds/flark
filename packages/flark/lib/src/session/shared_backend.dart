import '../parse/backend.dart';
import '../parse/render_model.dart';
import 'backend_loader.dart';

/// One parser lent to every holder of a lease: the first lease loads it,
/// leases asked for during that load wait for it, and disposing the last
/// lease disposes the parser.
///
/// A failed load fails each lease waiting for it and is then forgotten, so
/// the next lease loads again. An idle parser is not kept either. The web
/// shares its Wasm parser this way, and Wasm linear memory never shrinks: a
/// kept parser would go on holding the memory of the largest document it
/// parsed after every document had closed, while a new parser costs only an
/// instantiation of the module the page has already compiled.
///
/// Sharing needs a parser that serves its holders one parse at a time and
/// stays usable after a parse error, as the Wasm parser does by rebuilding
/// its instance after a trap. A parse error is the parsing holder's alone:
/// nothing here discards the parser over it.
final class SharedBackend {
  SharedBackend(this._load);

  final Future<FlarkParseBackend> Function() _load;

  /// The parser, or its load in progress, while any lease holds or awaits it.
  Future<FlarkParseBackend>? _parser;

  /// Leases not yet disposed, and leases still waiting for the load. A lease
  /// counts from the moment it is asked for, so one disposed as soon as it
  /// arrives cannot dispose the parser that another is about to receive.
  int _holders = 0;

  /// Lend the parser, loading it first if no lease holds it.
  Future<FlarkBackendLease> lease() async {
    // A loader that throws rather than failing its future records nothing
    // here, and fails only this lease.
    final parser = _parser ??= _load();
    _holders++;
    final FlarkParseBackend backend;
    try {
      backend = await parser;
    } catch (_) {
      _holders--;
      // A lease that retried while this one waited may already have started
      // the next load; that one stays.
      if (identical(_parser, parser)) _parser = null;
      rethrow;
    }
    final lent = _LentBackend(backend, _release);
    return FlarkBackendLease(lent, lent.dispose);
  }

  void _release(FlarkParseBackend backend) {
    if (--_holders > 0) return;
    // Forget the parser before disposing it, so that a dispose that throws
    // still leaves the next lease to load a new one.
    _parser = null;
    backend.dispose();
  }
}

/// One holder's hold on the shared parser. It refuses to parse once disposed,
/// as a parser of the holder's own would, and disposing it releases this
/// holder's claim exactly once: a second release would take another holder's
/// claim and could dispose the parser under it.
final class _LentBackend implements FlarkParseBackend {
  _LentBackend(this._backend, this._release);

  FlarkParseBackend? _backend;
  final void Function(FlarkParseBackend) _release;

  FlarkParseBackend get _shared =>
      _backend ?? (throw StateError('parser lease used after dispose'));

  @override
  int get schemaVersion => _shared.schemaVersion;

  @override
  RenderModel parse(String source) => _shared.parse(source);

  @override
  void dispose() {
    final backend = _backend;
    if (backend == null) return;
    _backend = null;
    _release(backend);
  }
}
