import 'dart:ffi';

final class FlarkV4AbiInfo extends Struct {
  @Uint32()
  external int structSize;

  @Uint16()
  external int abiMajor;

  @Uint16()
  external int abiMinor;

  @Uint64()
  external int capabilityBits;

  @Uint32()
  external int maxSmallEditBytes;

  @Uint32()
  external int maxBulkChunkBytes;

  @Uint32()
  external int maxSourceChunkBytes;

  @Uint32()
  external int maxResultBytes;

  @Uint32()
  external int maxQueryItems;

  @Uint32()
  external int maxTransactionEdits;

  @Array(3)
  external Array<Uint64> reserved;
}

final class FlarkV4NegotiateRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint16()
  external int requestedMajor;

  @Uint16()
  external int requestedMinor;

  @Uint64()
  external int requiredCapabilityBits;
}

final class FlarkV4SessionConfig extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int parserProfile;

  @Uint64()
  external int historyBudgetBytes;

  @Uint64()
  external int maxDocumentBytes;

  @Uint64()
  external int flags;

  @Array(4)
  external Array<Uint64> reserved;
}

final class FlarkV4SessionRef extends Struct {
  @Uint64()
  external int session;

  @Uint64()
  external int ownerToken;
}

final class FlarkV4SourceRange extends Struct {
  @Uint64()
  external int startByte;

  @Uint64()
  external int endByte;
}

final class FlarkV4WorkBudget extends Struct {
  @Uint64()
  external int maxWorkUnits;

  @Uint64()
  external int advisoryMaxMicros;

  @Uint32()
  external int maxResultItems;

  @Uint32()
  external int maxResultBytes;
}

final class FlarkV4Outcome extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int operation;

  @Uint32()
  external int status;

  @Uint32()
  external int progressState;

  @Uint64()
  external int primaryHandle;

  @Uint64()
  external int secondaryHandle;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  @Uint64()
  external int progressToken;

  @Uint64()
  external int requiredBytes;

  @Uint64()
  external int writtenBytes;

  @Uint64()
  external int detailCode;

  @Array(4)
  external Array<Uint64> reserved;
}

final class FlarkV4CreateRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  @Uint64()
  external int ownerToken;

  @Uint64()
  external int expectedTotalBytes;

  external FlarkV4SessionConfig config;
}

final class FlarkV4StageRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int transaction;

  @Uint64()
  external int chunkOffset;

  @Uint64()
  external int chunkLen;

  @Array(2)
  external Array<Uint64> reserved;
}

final class FlarkV4TransactionRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int transaction;

  @Uint64()
  external int expectedRevision;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4PumpRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int expectedRevision;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;
}

final class FlarkV4EditDescriptor extends Struct {
  @Uint64()
  external int startByte;

  @Uint64()
  external int endByte;

  @Uint64()
  external int replacementOffset;

  @Uint64()
  external int replacementLen;
}

final class FlarkV4SmallEditRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int expectedRevision;

  @Uint32()
  external int editCount;

  @Uint32()
  external int reservedU32;

  @Uint64()
  external int replacementBytesLen;

  external FlarkV4WorkBudget budget;

  @Array(2)
  external Array<Uint64> reserved;
}

final class FlarkV4BulkBeginRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int expectedRevision;

  external FlarkV4SourceRange range;

  @Uint64()
  external int expectedTotalBytes;

  @Array(2)
  external Array<Uint64> reserved;
}

final class FlarkV4CoordinateRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int fromKind;

  @Uint32()
  external int toKind;

  @Uint32()
  external int reservedU32;

  external FlarkV4SessionRef session;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  @Uint64()
  external int position;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4HistoryRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int expectedRevision;

  @Uint64()
  external int historyToken;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4SourceReadRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int revision;

  external FlarkV4SourceRange range;

  @Array(2)
  external Array<Uint64> reserved;
}

final class FlarkV4QueryRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int queryKind;

  external FlarkV4SessionRef session;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  external FlarkV4SourceRange range;

  @Uint64()
  external int continuation;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4ContinuationRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  @Uint64()
  external int continuation;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4CloseRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;

  @Array(1)
  external Array<Uint64> reserved;
}

final class FlarkV4ResultPageHeader extends Struct {
  @Uint32()
  external int structSize;

  @Uint16()
  external int abiMajor;

  @Uint16()
  external int abiMinor;

  @Uint32()
  external int recordKind;

  @Uint32()
  external int certificationState;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  external FlarkV4SourceRange requestedRange;
  external FlarkV4SourceRange coveredRange;

  @Uint32()
  external int itemCount;

  @Uint32()
  external int payloadBytes;

  @Uint64()
  external int continuation;

  @Array(2)
  external Array<Uint64> reserved;
}

final class FlarkV4CertificationRangeRecord extends Struct {
  @Uint32()
  external int certificationState;

  @Uint32()
  external int reserved;

  external FlarkV4SourceRange sourceRange;
  external FlarkV4SourceRange sourceUtf16Range;
}

final class FlarkV4ViewportRowRecord extends Struct {
  @Uint64()
  external int ordinal;

  @Uint32()
  external int kind;

  @Uint32()
  external int flags;

  @Uint64()
  external int sourceStartByte;

  @Uint64()
  external int sourceEndByte;

  @Uint64()
  external int sourceStartUtf16;

  @Uint64()
  external int sourceEndUtf16;

  @Uint64()
  external int editableStartByte;

  @Uint64()
  external int editableEndByte;

  @Uint64()
  external int editableStartUtf16;

  @Uint64()
  external int editableEndUtf16;

  @Uint64()
  external int presentationPrefixStartByte;

  @Uint64()
  external int presentationPrefixEndByte;

  @Uint64()
  external int presentationPrefixStartUtf16;

  @Uint64()
  external int presentationPrefixEndUtf16;

  @Uint32()
  external int pathDepth;

  @Uint32()
  external int semanticVariant;

  @Uint32()
  external int semanticValue;

  @Uint32()
  external int inlineFactCount;
}

final class FlarkV4InlineFactRecord extends Struct {
  @Uint32()
  external int kind;

  @Uint32()
  external int flags;

  @Uint64()
  external int sourceStartByte;

  @Uint64()
  external int sourceEndByte;

  @Uint64()
  external int sourceStartUtf16;

  @Uint64()
  external int sourceEndUtf16;

  @Uint64()
  external int contentStartByte;

  @Uint64()
  external int contentEndByte;

  @Uint64()
  external int contentStartUtf16;

  @Uint64()
  external int contentEndUtf16;

  @Uint32()
  external int replacementFirst;

  @Uint32()
  external int replacementSecond;
}

final class FlarkV4AnchorRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int coordinateKind;

  external FlarkV4SessionRef session;

  @Uint64()
  external int revision;

  @Uint64()
  external int snapshot;

  @Uint64()
  external int anchor;

  @Uint64()
  external int position;

  @Uint32()
  external int affinity;

  @Uint32()
  external int reservedU32;

  @Uint64()
  external int progressToken;

  external FlarkV4WorkBudget budget;
}

final class FlarkV4CancelRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int progressToken;

  @Array(4)
  external Array<Uint64> reserved;
}

final class FlarkV4OwnerTransferRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Uint64()
  external int newOwnerToken;

  @Array(4)
  external Array<Uint64> reserved;
}

final class FlarkV4InspectRequest extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int flags;

  external FlarkV4SessionRef session;

  @Array(5)
  external Array<Uint64> reserved;
}

final class FlarkV4SessionInspection extends Struct {
  @Uint32()
  external int structSize;

  @Uint32()
  external int sessionState;

  @Uint64()
  external int session;

  @Uint64()
  external int revision;

  @Uint32()
  external int liveTransactions;

  @Uint32()
  external int liveContinuations;

  @Uint32()
  external int liveAnchors;

  @Uint32()
  external int liveHistoryTokens;

  @Array(3)
  external Array<Uint64> reserved;
}

typedef _CreateBeginNative =
    Uint32 Function(
      Pointer<FlarkV4CreateRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef CreateBeginDart =
    int Function(
      Pointer<FlarkV4CreateRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _CreateAppendNative =
    Uint32 Function(
      Pointer<FlarkV4StageRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef CreateAppendDart =
    int Function(
      Pointer<FlarkV4StageRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _CreateCommitNative =
    Uint32 Function(
      Pointer<FlarkV4TransactionRequest>,
      Pointer<FlarkV4Outcome>,
    );
typedef CreateCommitDart =
    int Function(Pointer<FlarkV4TransactionRequest>, Pointer<FlarkV4Outcome>);

typedef _PumpNative =
    Uint32 Function(Pointer<FlarkV4PumpRequest>, Pointer<FlarkV4Outcome>);
typedef PumpDart =
    int Function(Pointer<FlarkV4PumpRequest>, Pointer<FlarkV4Outcome>);

typedef _SmallEditNative =
    Uint32 Function(
      Pointer<FlarkV4SmallEditRequest>,
      Pointer<FlarkV4EditDescriptor>,
      Uint32,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef SmallEditDart =
    int Function(
      Pointer<FlarkV4SmallEditRequest>,
      Pointer<FlarkV4EditDescriptor>,
      int,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _BulkBeginNative =
    Uint32 Function(Pointer<FlarkV4BulkBeginRequest>, Pointer<FlarkV4Outcome>);
typedef BulkBeginDart =
    int Function(Pointer<FlarkV4BulkBeginRequest>, Pointer<FlarkV4Outcome>);

typedef _BulkAppendNative =
    Uint32 Function(
      Pointer<FlarkV4StageRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef BulkAppendDart =
    int Function(
      Pointer<FlarkV4StageRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _BulkTransactionNative =
    Uint32 Function(
      Pointer<FlarkV4TransactionRequest>,
      Pointer<FlarkV4Outcome>,
    );
typedef BulkTransactionDart =
    int Function(Pointer<FlarkV4TransactionRequest>, Pointer<FlarkV4Outcome>);

typedef _CoordinateConvertNative =
    Uint32 Function(Pointer<FlarkV4CoordinateRequest>, Pointer<FlarkV4Outcome>);
typedef CoordinateConvertDart =
    int Function(Pointer<FlarkV4CoordinateRequest>, Pointer<FlarkV4Outcome>);

typedef _HistoryNative =
    Uint32 Function(Pointer<FlarkV4HistoryRequest>, Pointer<FlarkV4Outcome>);
typedef HistoryDart =
    int Function(Pointer<FlarkV4HistoryRequest>, Pointer<FlarkV4Outcome>);

typedef _SourceReadNative =
    Uint32 Function(
      Pointer<FlarkV4SourceReadRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef SourceReadDart =
    int Function(
      Pointer<FlarkV4SourceReadRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _QueryViewportNative =
    Uint32 Function(
      Pointer<FlarkV4QueryRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef QueryViewportDart =
    int Function(
      Pointer<FlarkV4QueryRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _ContinuationNextNative =
    Uint32 Function(
      Pointer<FlarkV4ContinuationRequest>,
      Pointer<Uint8>,
      Uint64,
      Pointer<FlarkV4Outcome>,
    );
typedef ContinuationNextDart =
    int Function(
      Pointer<FlarkV4ContinuationRequest>,
      Pointer<Uint8>,
      int,
      Pointer<FlarkV4Outcome>,
    );

typedef _ContinuationReleaseNative =
    Uint32 Function(
      Pointer<FlarkV4ContinuationRequest>,
      Pointer<FlarkV4Outcome>,
    );
typedef ContinuationReleaseDart =
    int Function(Pointer<FlarkV4ContinuationRequest>, Pointer<FlarkV4Outcome>);

typedef _CloseNative =
    Uint32 Function(Pointer<FlarkV4CloseRequest>, Pointer<FlarkV4Outcome>);
typedef CloseDart =
    int Function(Pointer<FlarkV4CloseRequest>, Pointer<FlarkV4Outcome>);

typedef _AnchorNative =
    Uint32 Function(Pointer<FlarkV4AnchorRequest>, Pointer<FlarkV4Outcome>);
typedef AnchorDart =
    int Function(Pointer<FlarkV4AnchorRequest>, Pointer<FlarkV4Outcome>);

typedef _CancelNative =
    Uint32 Function(Pointer<FlarkV4CancelRequest>, Pointer<FlarkV4Outcome>);
typedef CancelDart =
    int Function(Pointer<FlarkV4CancelRequest>, Pointer<FlarkV4Outcome>);

typedef _OwnerTransferNative =
    Uint32 Function(
      Pointer<FlarkV4OwnerTransferRequest>,
      Pointer<FlarkV4Outcome>,
    );
typedef OwnerTransferDart =
    int Function(Pointer<FlarkV4OwnerTransferRequest>, Pointer<FlarkV4Outcome>);

typedef _SessionInspectNative =
    Uint32 Function(
      Pointer<FlarkV4InspectRequest>,
      Pointer<FlarkV4SessionInspection>,
      Pointer<FlarkV4Outcome>,
    );
typedef SessionInspectDart =
    int Function(
      Pointer<FlarkV4InspectRequest>,
      Pointer<FlarkV4SessionInspection>,
      Pointer<FlarkV4Outcome>,
    );

typedef _NegotiateNative =
    Uint32 Function(
      Pointer<FlarkV4NegotiateRequest>,
      Pointer<FlarkV4AbiInfo>,
      Pointer<FlarkV4Outcome>,
    );
typedef NegotiateDart =
    int Function(
      Pointer<FlarkV4NegotiateRequest>,
      Pointer<FlarkV4AbiInfo>,
      Pointer<FlarkV4Outcome>,
    );

final class FlarkV4Bindings {
  FlarkV4Bindings(DynamicLibrary library)
    : negotiate = library.lookupFunction<_NegotiateNative, NegotiateDart>(
        'flark_v4_negotiate',
      ),
      createBegin = library.lookupFunction<_CreateBeginNative, CreateBeginDart>(
        'flark_v4_create_begin',
      ),
      createAppend = library
          .lookupFunction<_CreateAppendNative, CreateAppendDart>(
            'flark_v4_create_append',
          ),
      createCommit = library
          .lookupFunction<_CreateCommitNative, CreateCommitDart>(
            'flark_v4_create_commit',
          ),
      pump = library.lookupFunction<_PumpNative, PumpDart>('flark_v4_pump'),
      smallEdit = library.lookupFunction<_SmallEditNative, SmallEditDart>(
        'flark_v4_small_edit',
      ),
      bulkBegin = library.lookupFunction<_BulkBeginNative, BulkBeginDart>(
        'flark_v4_bulk_begin',
      ),
      bulkAppend = library.lookupFunction<_BulkAppendNative, BulkAppendDart>(
        'flark_v4_bulk_append',
      ),
      bulkCommit = library
          .lookupFunction<_BulkTransactionNative, BulkTransactionDart>(
            'flark_v4_bulk_commit',
          ),
      bulkAbort = library
          .lookupFunction<_BulkTransactionNative, BulkTransactionDart>(
            'flark_v4_bulk_abort',
          ),
      coordinateConvert = library
          .lookupFunction<_CoordinateConvertNative, CoordinateConvertDart>(
            'flark_v4_coordinate_convert',
          ),
      historyReplay = library.lookupFunction<_HistoryNative, HistoryDart>(
        'flark_v4_history_replay',
      ),
      historyRelease = library.lookupFunction<_HistoryNative, HistoryDart>(
        'flark_v4_history_release',
      ),
      sourceRead = library.lookupFunction<_SourceReadNative, SourceReadDart>(
        'flark_v4_source_read',
      ),
      queryViewport = library
          .lookupFunction<_QueryViewportNative, QueryViewportDart>(
            'flark_v4_query_viewport',
          ),
      continuationNext = library
          .lookupFunction<_ContinuationNextNative, ContinuationNextDart>(
            'flark_v4_continuation_next',
          ),
      continuationRelease = library
          .lookupFunction<_ContinuationReleaseNative, ContinuationReleaseDart>(
            'flark_v4_continuation_release',
          ),
      closeBegin = library.lookupFunction<_CloseNative, CloseDart>(
        'flark_v4_close_begin',
      ),
      closePump = library.lookupFunction<_CloseNative, CloseDart>(
        'flark_v4_close_pump',
      ),
      closeFinish = library.lookupFunction<_CloseNative, CloseDart>(
        'flark_v4_close_finish',
      ),
      createAbort = library
          .lookupFunction<_CreateCommitNative, CreateCommitDart>(
            'flark_v4_create_abort',
          ),
      anchorCreate = library.lookupFunction<_AnchorNative, AnchorDart>(
        'flark_v4_anchor_create',
      ),
      anchorTransform = library.lookupFunction<_AnchorNative, AnchorDart>(
        'flark_v4_anchor_transform',
      ),
      anchorResolve = library.lookupFunction<_AnchorNative, AnchorDart>(
        'flark_v4_anchor_resolve',
      ),
      anchorRelease = library.lookupFunction<_AnchorNative, AnchorDart>(
        'flark_v4_anchor_release',
      ),
      cancel = library.lookupFunction<_CancelNative, CancelDart>(
        'flark_v4_cancel',
      ),
      sessionTransferOwner = library
          .lookupFunction<_OwnerTransferNative, OwnerTransferDart>(
            'flark_v4_session_transfer_owner',
          ),
      sessionInspect = library
          .lookupFunction<_SessionInspectNative, SessionInspectDart>(
            'flark_v4_session_inspect',
          );

  final NegotiateDart negotiate;
  final CreateBeginDart createBegin;
  final CreateAppendDart createAppend;
  final CreateCommitDart createCommit;
  final PumpDart pump;
  final SmallEditDart smallEdit;
  final BulkBeginDart bulkBegin;
  final BulkAppendDart bulkAppend;
  final BulkTransactionDart bulkCommit;
  final BulkTransactionDart bulkAbort;
  final CoordinateConvertDart coordinateConvert;
  final HistoryDart historyReplay;
  final HistoryDart historyRelease;
  final SourceReadDart sourceRead;
  final QueryViewportDart queryViewport;
  final ContinuationNextDart continuationNext;
  final ContinuationReleaseDart continuationRelease;
  final CloseDart closeBegin;
  final CloseDart closePump;
  final CloseDart closeFinish;
  final CreateCommitDart createAbort;
  final AnchorDart anchorCreate;
  final AnchorDart anchorTransform;
  final AnchorDart anchorResolve;
  final AnchorDart anchorRelease;
  final CancelDart cancel;
  final OwnerTransferDart sessionTransferOwner;
  final SessionInspectDart sessionInspect;
}
