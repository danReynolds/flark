import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

class DocumentState {
  const DocumentState({
    required this.value,
    required this.revision,
    required this.lineIndex,
    required this.geometry,
  });

  final TextEditingValue value;
  final int revision;
  final LineIndex lineIndex;
  final GeometryModel geometry;

  DocumentState copyWith({
    TextEditingValue? value,
    int? revision,
    LineIndex? lineIndex,
    GeometryModel? geometry,
  }) {
    return DocumentState(
      value: value ?? this.value,
      revision: revision ?? this.revision,
      lineIndex: lineIndex ?? this.lineIndex,
      geometry: geometry ?? this.geometry,
    );
  }
}
