import 'package:flutter/services.dart';

import '../../engine/syntax_snapshot.dart';

class ProjectionState {
  const ProjectionState({
    required this.projectedHiddenRanges,
    required this.projectedExclusionRanges,
    required this.authoritativeHiddenRanges,
    required this.authoritativeExclusionRanges,
    required this.projectedCursorMask,
    required this.authoritativeCursorMask,
    required this.activeCursorMask,
    required this.latestAuthoritativeSnapshot,
  });

  final List<TextRange> projectedHiddenRanges;
  final List<TextRange> projectedExclusionRanges;
  final List<TextRange> authoritativeHiddenRanges;
  final List<TextRange> authoritativeExclusionRanges;
  final CursorValidationMask projectedCursorMask;
  final CursorValidationMask authoritativeCursorMask;
  final CursorValidationMask activeCursorMask;
  final SyntaxSnapshot? latestAuthoritativeSnapshot;

  ProjectionState copyWith({
    List<TextRange>? projectedHiddenRanges,
    List<TextRange>? projectedExclusionRanges,
    List<TextRange>? authoritativeHiddenRanges,
    List<TextRange>? authoritativeExclusionRanges,
    CursorValidationMask? projectedCursorMask,
    CursorValidationMask? authoritativeCursorMask,
    CursorValidationMask? activeCursorMask,
    SyntaxSnapshot? latestAuthoritativeSnapshot,
  }) {
    return ProjectionState(
      projectedHiddenRanges:
          projectedHiddenRanges ?? this.projectedHiddenRanges,
      projectedExclusionRanges:
          projectedExclusionRanges ?? this.projectedExclusionRanges,
      authoritativeHiddenRanges:
          authoritativeHiddenRanges ?? this.authoritativeHiddenRanges,
      authoritativeExclusionRanges:
          authoritativeExclusionRanges ?? this.authoritativeExclusionRanges,
      projectedCursorMask: projectedCursorMask ?? this.projectedCursorMask,
      authoritativeCursorMask:
          authoritativeCursorMask ?? this.authoritativeCursorMask,
      activeCursorMask: activeCursorMask ?? this.activeCursorMask,
      latestAuthoritativeSnapshot:
          latestAuthoritativeSnapshot ?? this.latestAuthoritativeSnapshot,
    );
  }
}
