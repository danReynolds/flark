import 'dart:async';
import 'dart:convert';
import 'dart:io';

Future<void> main(List<String> arguments) async {
  if (arguments.length != 6) {
    stderr.writeln(
      'Usage: dart hosted_server.dart '
      '<flark.tar.gz> <flark-sha256> <flark-pubspec> '
      '<flark_flutter.tar.gz> <flark-flutter-sha256> '
      '<flark-flutter-pubspec>',
    );
    exitCode = 64;
    return;
  }

  final archives = <String, _HostedArchive>{
    'flark': _HostedArchive(
      file: File(arguments[0]),
      sha256: arguments[1],
      pubspec: _readHostedPubspec(File(arguments[2])),
    ),
    'flark_flutter': _HostedArchive(
      file: File(arguments[3]),
      sha256: arguments[4],
      pubspec: _readHostedPubspec(File(arguments[5])),
    ),
  };
  for (final archive in archives.values) {
    if (!archive.file.existsSync()) {
      throw StateError('Missing archive: ${archive.file.path}');
    }
  }

  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  final upstream = HttpClient();
  final baseUri = Uri.parse('http://127.0.0.1:${server.port}/');
  stdout.writeln(baseUri);
  await stdout.flush();

  await for (final request in server) {
    unawaited(
      _serve(request, baseUri, archives, upstream).catchError((Object error) {
        stderr.writeln('Hosted server request failed: $error');
        try {
          request.response.statusCode = HttpStatus.internalServerError;
        } on StateError {
          // The upstream may already have started a response.
        }
        return request.response.close();
      }),
    );
  }
}

Future<void> _serve(
  HttpRequest request,
  Uri baseUri,
  Map<String, _HostedArchive> archives,
  HttpClient upstream,
) async {
  final segments = request.uri.pathSegments;
  if (segments.length >= 3 &&
      segments[0] == 'api' &&
      segments[1] == 'packages') {
    final name = segments[2];
    final archive = archives[name];
    if (archive != null) {
      final version = _versionMetadata(name, archive, baseUri);
      if (segments.length == 3) {
        await _json(request.response, <String, Object?>{
          'name': name,
          'latest': version,
          'versions': <Object?>[version],
        });
        return;
      }
      if (segments.length == 5 &&
          segments[3] == 'versions' &&
          segments[4] == archive.version) {
        await _json(request.response, version);
        return;
      }
    }
  }

  if (segments.length == 4 &&
      segments[0] == 'archives' &&
      segments[2] == 'versions') {
    final archive = archives[segments[1]];
    if (archive != null && segments[3] == '${archive.version}.tar.gz') {
      request.response.headers.contentType = ContentType('application', 'gzip');
      request.response.contentLength = archive.file.lengthSync();
      await request.response.addStream(archive.file.openRead());
      await request.response.close();
      return;
    }
  }

  // Every non-Flark hosted dependency remains a real pub.dev dependency. The
  // loopback URL is the default host only so flark_flutter's ordinary
  // `flark` version constraint resolves to the same local archive source.
  final upstreamUri = Uri(
    scheme: 'https',
    host: 'pub.dev',
    path: request.uri.path,
    query: request.uri.query.isEmpty ? null : request.uri.query,
  );
  final upstreamRequest = await upstream.getUrl(upstreamUri);
  final userAgent = request.headers.value(HttpHeaders.userAgentHeader);
  if (userAgent != null) {
    upstreamRequest.headers.set(HttpHeaders.userAgentHeader, userAgent);
  }
  final upstreamResponse = await upstreamRequest.close();
  request.response.statusCode = upstreamResponse.statusCode;
  for (final header in const <String>[
    HttpHeaders.contentTypeHeader,
    HttpHeaders.cacheControlHeader,
    HttpHeaders.etagHeader,
    HttpHeaders.lastModifiedHeader,
  ]) {
    final values = upstreamResponse.headers[header];
    if (values != null) request.response.headers.set(header, values);
  }
  await request.response.addStream(upstreamResponse);
  await request.response.close();
}

Map<String, Object?> _versionMetadata(
  String name,
  _HostedArchive archive,
  Uri baseUri,
) => <String, Object?>{
  'version': archive.version,
  'pubspec': archive.pubspec,
  'archive_url': baseUri
      .resolve('archives/$name/versions/${archive.version}.tar.gz')
      .toString(),
  'archive_sha256': archive.sha256,
  'published': '2026-07-22T00:00:00.000Z',
};

Future<void> _json(HttpResponse response, Map<String, Object?> value) async {
  response.headers.contentType = ContentType.json;
  response.write(jsonEncode(value));
  await response.close();
}

final class _HostedArchive {
  const _HostedArchive({
    required this.file,
    required this.sha256,
    required this.pubspec,
  });

  final File file;
  final String sha256;
  final Map<String, Object?> pubspec;

  String get version => pubspec['version']! as String;
}

/// Parses the small portion of pubspec YAML used by hosted version solving.
///
/// The authoritative file comes from the extracted archive. Supporting only
/// scalar `environment` values and scalar-or-one-map-level dependencies keeps
/// this harness independent of the repository's resolved package graph while
/// still failing clearly if either published pubspec adopts a new shape.
Map<String, Object?> _readHostedPubspec(File file) {
  final result = <String, Object?>{};
  final environment = <String, Object?>{};
  final dependencies = <String, Object?>{};
  String? section;
  String? nestedDependency;

  for (final rawLine in file.readAsLinesSync()) {
    final line = rawLine.trimRight();
    final trimmed = line.trimLeft();
    if (trimmed.isEmpty || trimmed.startsWith('#')) continue;
    final indent = line.length - trimmed.length;
    final field = _yamlField(trimmed);
    if (field == null) continue;
    final (key, value) = field;

    if (indent == 0) {
      section = key;
      nestedDependency = null;
      if ((key == 'name' || key == 'version') && value.isNotEmpty) {
        result[key] = _yamlScalar(value);
      }
      continue;
    }
    if (section == 'environment' && indent == 2 && value.isNotEmpty) {
      environment[key] = _yamlScalar(value);
      continue;
    }
    if (section != 'dependencies') continue;
    if (indent == 2) {
      if (value.isEmpty) {
        dependencies[key] = <String, Object?>{};
        nestedDependency = key;
      } else {
        dependencies[key] = _yamlScalar(value);
        nestedDependency = null;
      }
      continue;
    }
    if (indent == 4 && nestedDependency != null && value.isNotEmpty) {
      (dependencies[nestedDependency]! as Map<String, Object?>)[key] =
          _yamlScalar(value);
      continue;
    }
    throw FormatException(
      'Unsupported dependency shape in ${file.path}: $rawLine',
    );
  }

  if (result['name'] == null || result['version'] == null) {
    throw FormatException('Missing name/version in ${file.path}.');
  }
  result['environment'] = environment;
  result['dependencies'] = dependencies;
  return result;
}

(String, String)? _yamlField(String line) {
  final separator = line.indexOf(':');
  if (separator <= 0) return null;
  return (
    line.substring(0, separator).trim(),
    line.substring(separator + 1).trim(),
  );
}

String _yamlScalar(String value) {
  if (value.length >= 2 && value.startsWith("'") && value.endsWith("'")) {
    return value.substring(1, value.length - 1).replaceAll("''", "'");
  }
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) {
    return jsonDecode(value) as String;
  }
  return value;
}
