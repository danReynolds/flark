class ListMarkerContext {
  const ListMarkerContext({
    required this.markerStart,
    required this.markerEnd,
    required this.contentStart,
    required this.continueMarker,
    required this.isOrdered,
  });

  final int markerStart;
  final int markerEnd;
  final int contentStart;
  final String continueMarker;
  final bool isOrdered;
}
