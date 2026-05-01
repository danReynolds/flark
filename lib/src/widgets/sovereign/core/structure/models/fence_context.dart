import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';

class FenceContext {
  const FenceContext({
    required this.block,
    required this.openLine,
    required this.closeLineExclusive,
    required this.closeLine,
    required this.hasClosingFence,
  });

  final MeasuredBlock block;
  final int openLine;
  final int closeLineExclusive;
  final int? closeLine;
  final bool hasClosingFence;

  int get startOffset => block.startOffset;
  int get endOffset => block.endOffset;
}
