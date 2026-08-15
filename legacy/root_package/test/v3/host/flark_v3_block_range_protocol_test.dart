import 'package:flark/src/v3/host/host.dart';
import 'package:test/test.dart';

void main() {
  test('packed-entry receipts bind to authenticated storage pages', () {
    expect(
      flarkV3HostPackedEntryReceiptFitsStoragePages(
        storagePagesVisited: 1,
        packedEntriesInspected: 128,
      ),
      isTrue,
    );
    expect(
      flarkV3HostPackedEntryReceiptFitsStoragePages(
        storagePagesVisited: 1,
        packedEntriesInspected: 129,
      ),
      isFalse,
    );
    expect(
      flarkV3HostPackedEntryReceiptFitsStoragePages(
        storagePagesVisited: 0,
        packedEntriesInspected: 0,
      ),
      isTrue,
    );
  });
}
