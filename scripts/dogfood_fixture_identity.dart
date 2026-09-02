// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';

import 'package:crypto/crypto.dart';

import '../packages/flark_flutter/example/lib/dogfood_documents.dart';

Map<String, Object> dogfoodFixtureIdentity(String cellId) {
  final preset = switch (cellId) {
    String id
        when id.startsWith('product-tour') || id.startsWith('lifecycle-') =>
      DogfoodDocumentPreset.productTour,
    String id when id.startsWith('ordinary-1m') =>
      DogfoodDocumentPreset.prose1MiB,
    'dense-blocks-1m-journey' => DogfoodDocumentPreset.denseBlocks1MiB,
    'ordinary-5m-journey' => DogfoodDocumentPreset.prose5MiB,
    'giant-line-5m-journey' => DogfoodDocumentPreset.giantLine5MiB,
    'ordinary-10m-journey' => DogfoodDocumentPreset.prose10MiB,
    'streamed-10m-journey' => DogfoodDocumentPreset.streamed10MiB,
    _ => throw UnsupportedError('$cellId has no frozen dogfood fixture'),
  };
  final sourceBytes = utf8.encode(buildDogfoodDocument(preset));
  return {
    'presetId': preset.name,
    'sourceBytes': sourceBytes.length,
    'sourceSha256': sha256.convert(sourceBytes).toString(),
  };
}
