class QuoteContext {
  const QuoteContext({
    required this.startLine,
    required this.endLineExclusive,
    required this.firstContentLine,
    required this.lastContentLine,
  });

  final int startLine;
  final int endLineExclusive;
  final int firstContentLine;
  final int lastContentLine;
}
