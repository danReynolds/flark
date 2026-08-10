import 'dart:ffi';

import 'package:flark_core/src/native/bindings.dart';
import 'package:test/test.dart';

void main() {
  test('Dart FFI records match the frozen v4 C layouts', () {
    expect(sizeOf<FlarkV4Outcome>(), 112);
    expect(sizeOf<FlarkV4ResultPageHeader>(), 96);
    expect(sizeOf<FlarkV4ViewportRowRecord>(), 128);
    expect(sizeOf<FlarkV4InlineFactRecord>(), 80);
    expect(sizeOf<FlarkV4CertificationRangeRecord>(), 40);
    expect(sizeOf<FlarkV4SourceRange>(), 16);
    expect(sizeOf<FlarkV4EditDescriptor>(), 32);
    expect(sizeOf<FlarkV4WorkBudget>(), 24);
    expect(sizeOf<FlarkV4SessionRef>(), 16);
    expect(sizeOf<FlarkV4SessionConfig>(), 64);
    expect(sizeOf<FlarkV4CreateRequest>(), 88);
    expect(sizeOf<FlarkV4StageRequest>(), 64);
    expect(sizeOf<FlarkV4SourceReadRequest>(), 64);
    expect(sizeOf<FlarkV4SmallEditRequest>(), 88);
    expect(sizeOf<FlarkV4BulkBeginRequest>(), 72);
    expect(sizeOf<FlarkV4TransactionRequest>(), 80);
    expect(sizeOf<FlarkV4PumpRequest>(), 64);
    expect(sizeOf<FlarkV4QueryRequest>(), 96);
    expect(sizeOf<FlarkV4ContinuationRequest>(), 80);
    expect(sizeOf<FlarkV4CloseRequest>(), 64);
    expect(sizeOf<FlarkV4CoordinateRequest>(), 96);
    expect(sizeOf<FlarkV4HistoryRequest>(), 80);
    expect(sizeOf<FlarkV4AnchorRequest>(), 96);
    expect(sizeOf<FlarkV4CancelRequest>(), 64);
    expect(sizeOf<FlarkV4OwnerTransferRequest>(), 64);
    expect(sizeOf<FlarkV4InspectRequest>(), 64);
    expect(sizeOf<FlarkV4SessionInspection>(), 64);
  });
}
