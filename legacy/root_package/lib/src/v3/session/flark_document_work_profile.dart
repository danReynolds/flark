import '../host/host.dart';

/// Bounded work admitted on the Dart session's latency-sensitive caller.
///
/// Platform adapters may impose tighter input or frame budgets, but parser
/// publication and query work is guarded here so non-Flutter clients receive
/// the same safety contract.
final class FlarkDocumentWorkProfile {
  const FlarkDocumentWorkProfile({
    required this.maximumQueryEncodedBytes,
    this.maximumInlineSidecarQueryEncodedBytes = 128 * 1024,
    this.maximumViewportQueryEncodedBytes = 256 * 1024,
    required this.maximumQueryOpenDepth,
    required this.maximumQueryLeafCount,
    required this.maximumQueryTreeNodesVisited,
    required this.maximumHostInspectBytes,
    required this.maximumHostCopyBytes,
    required this.maximumHostTransitions,
    required this.maximumPublicationPacketBytes,
  });

  static const prototype = FlarkDocumentWorkProfile(
    maximumQueryEncodedBytes: 64 * 1024,
    maximumInlineSidecarQueryEncodedBytes: 128 * 1024,
    maximumViewportQueryEncodedBytes: 256 * 1024,
    maximumQueryOpenDepth: 64,
    maximumQueryLeafCount: 256,
    maximumQueryTreeNodesVisited: 1024,
    maximumHostInspectBytes: 64 * 1024,
    maximumHostCopyBytes: 64 * 1024,
    maximumHostTransitions: 256,
    maximumPublicationPacketBytes: FlarkV3HostPublicationPacket.maximumRawBytes,
  );

  final int maximumQueryEncodedBytes;
  final int maximumInlineSidecarQueryEncodedBytes;
  final int maximumViewportQueryEncodedBytes;
  final int maximumQueryOpenDepth;
  final int maximumQueryLeafCount;
  final int maximumQueryTreeNodesVisited;
  final int maximumHostInspectBytes;
  final int maximumHostCopyBytes;
  final int maximumHostTransitions;
  final int maximumPublicationPacketBytes;

  void validateQueryBudget(FlarkV3HostQueryBudget budget) {
    if (budget.maxEncodedBytes > maximumQueryEncodedBytes ||
        budget.maxOpenDepth > maximumQueryOpenDepth ||
        budget.maxLeafCount > maximumQueryLeafCount ||
        budget.maxTreeNodesVisited > maximumQueryTreeNodesVisited) {
      throw ArgumentError.value(
        budget,
        'budget',
        'Structural query exceeds the document-session work profile.',
      );
    }
  }

  bool admitsHostGrant(FlarkV3HostWorkGrant grant) =>
      grant.inspectBytes <= maximumHostInspectBytes &&
      grant.copyBytes <= maximumHostCopyBytes &&
      grant.transitions <= maximumHostTransitions;
}
