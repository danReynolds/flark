/* Flark v3 persistent parser Worker. External classic Worker; no eval/import. */
'use strict';

(() => {
  const COMMAND_INITIALIZE = 0;
  const COMMAND_DISPATCH = 1;
  const COMMAND_STRICT_CLOSE = 2;
  const COMMAND_RECOVER = 3;
  const COMMAND_DISPOSE = 4;
  const COMMAND_DISPATCH_HOST_POLL = 5;
  const COMMAND_CHECKPOINT_B_PROBE = 6;
  const COMMAND_DISPATCH_INLINE_SIDECAR_HOST_POLL = 7;
  const COMMAND_DISPATCH_VIEWPORT_PRESENTATION_HOST_POLL = 8;

  const EVENT_READY = 0;
  const EVENT_FRAME = 1;
  const EVENT_FAILURE = 2;
  const EVENT_DISPOSED = 3;
  const EVENT_RECLAMATION_UNPROVEN = 4;
  const EVENT_CHECKPOINT_B_PROBE = 5;

  const STATUS_OK = 0x0000;
  const STATUS_INVALID = 0x0100;
  const STATUS_INVALID_STATE = 0x0101;
  const STATUS_BACKPRESSURE = 0x0102;
  const STATUS_FOREGROUND_BOUND_EXCEEDED = 0x010a;
  const STATUS_INTERNAL_FAULT = 0x0111;

  const ACTION_EVENT_RECEIPT_ACCEPTED = 5;
  const ACTION_CLOSE_LATCHED = 6;
  const EVENT_KIND_FAILED = 5;
  const EVENT_KIND_CLOSED = 7;

  const ENDPOINT_ABI_VERSION = 0x00020002;
  const MAXIMUM_FRAME_BYTES = 262168;
  const CHECKPOINT_B_MAXIMUM_JSON_BYTES = 64 * 1024;
  // One outer Worker task performs exactly one source microgrant first. This
  // is also its hard aggregate source-byte quota; source and candidate work
  // therefore remain fair even when both lanes have resumable work.
  const POLL_TASK_SOURCE_BYTE_QUOTA = 32 * 1024;
  const POLL_CHECKPOINTS = 64;
  const POLL_RETIREMENT_TRANSITIONS = 32;
  // Candidate work is amortized across bounded ABI microgrants inside the
  // Worker. The clock normally ends a turn first; the aggregate quota remains
  // authoritative when the monotonic clock is unexpectedly coarse.
  const POLL_CANDIDATE_TRANSITIONS_PER_SUBGRANT = 64;
  const POLL_TASK_CANDIDATE_TRANSITION_QUOTA = 4096;
  const POLL_TASK_BUDGET_MILLISECONDS = 4;

  const CONFIG_BYTES = 64;
  const HANDLE_BYTES = 8;
  const BINDING_BYTES = 24;
  const DISPATCH_RECEIPT_BYTES = 16;
  const POLL_RECEIPT_BYTES = 60;
  const CANDIDATE_RECEIPT_BYTES = 16;
  const WRITTEN_BYTES = 4;

  const OFFSET_CONFIG = 0;
  const OFFSET_HANDLE = OFFSET_CONFIG + CONFIG_BYTES;
  const OFFSET_BINDING = OFFSET_HANDLE + HANDLE_BYTES;
  const OFFSET_DISPATCH_RECEIPT = OFFSET_BINDING + BINDING_BYTES;
  const OFFSET_POLL_RECEIPT =
    OFFSET_DISPATCH_RECEIPT + DISPATCH_RECEIPT_BYTES;
  const OFFSET_CANDIDATE_RECEIPT = OFFSET_POLL_RECEIPT + POLL_RECEIPT_BYTES;
  const OFFSET_WRITTEN =
    OFFSET_CANDIDATE_RECEIPT + CANDIDATE_RECEIPT_BYTES;
  const OFFSET_INPUT = OFFSET_WRITTEN + WRITTEN_BYTES;
  const OFFSET_OUTPUT = OFFSET_INPUT + MAXIMUM_FRAME_BYTES;
  const SCRATCH_BYTES = OFFSET_OUTPUT + MAXIMUM_FRAME_BYTES;

  const REQUIRED_EXPORTS = Object.freeze([
    'memory',
    'flark_v3_wasm_alloc',
    'flark_v3_wasm_free',
    'flark_v3_wasm_endpoint_native_abi_version',
    'flark_v3_wasm_endpoint_config_standard',
    'flark_v3_wasm_endpoint_create',
    'flark_v3_wasm_endpoint_recover',
    'flark_v3_wasm_endpoint_dispatch',
    'flark_v3_wasm_endpoint_dispatch_host_poll',
    'flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll',
    'flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll',
    'flark_v3_wasm_endpoint_poll',
    'flark_v3_wasm_endpoint_poll_candidate',
    'flark_v3_wasm_endpoint_encode',
    'flark_v3_wasm_endpoint_close',
    'flark_v3_wasm_endpoint_remove',
    'flark_v3_wasm_endpoint_emergency_destroy',
  ]);

  class WorkerFailure extends Error {
    constructor(operation, status, detail) {
      super(detail == null ? operation : `${operation}: ${detail}`);
      this.name = 'FlarkV3WorkerFailure';
      this.operation = operation;
      this.status = status >>> 0;
      this.detail = detail == null ? null : String(detail);
    }
  }

  let endpoint = null;
  let initialized = false;
  let terminal = false;
  let commandChain = Promise.resolve();
  // A MessagePort continuation is a real Worker task, so inbound commands
  // and cancellation can interleave between bounded poll turns. Unlike a
  // nested zero-delay timer, it does not accumulate browser timer clamping.
  const pollContinuationChannel = new MessageChannel();
  pollContinuationChannel.port1.onmessage = (event) =>
    enqueue(() => {
      const current = endpoint;
      if (current !== null) current.poll(event.data);
    });

  function enqueue(operation) {
    commandChain = commandChain
      .then(() => {
        if (!terminal) return operation();
        return undefined;
      })
      .catch(failTerminally);
  }

  self.onmessage = (event) => enqueue(() => receive(event.data));
  self.onmessageerror = () =>
    enqueue(() => {
      throw new WorkerFailure(
        'messageDecode',
        STATUS_INVALID,
        'Worker command could not be deserialized',
      );
    });

  async function receive(message) {
    requireArray(message, 'Worker command');
    if (message.length === 0) {
      throw new WorkerFailure(
        'commandDecode',
        STATUS_INVALID,
        'empty Worker command',
      );
    }
    const command = requireU32(message[0], 'command');
    if (!initialized && command !== COMMAND_INITIALIZE && command !== COMMAND_DISPOSE) {
      throw new WorkerFailure(
        'commandBeforeInitialize',
        STATUS_INVALID_STATE,
        `command ${command}`,
      );
    }
    switch (command) {
      case COMMAND_INITIALIZE:
        if (message.length !== 2 || initialized || endpoint !== null) {
          throw new WorkerFailure(
            'initialize',
            STATUS_INVALID_STATE,
            'duplicate or malformed initialize command',
          );
        }
        await initialize(requireString(message[1], 'wasmUri'));
        return;
      case COMMAND_DISPATCH:
        requireMessageLength(message, 2, 'dispatch');
        endpoint.dispatch(requireFrame(message[1]), false, false, false, false);
        return;
      case COMMAND_STRICT_CLOSE:
        requireMessageLength(message, 2, 'strictClose');
        endpoint.dispatch(requireFrame(message[1]), true, false, false, false);
        return;
      case COMMAND_RECOVER:
        requireMessageLength(message, 7, 'recover');
        endpoint.recover(message.slice(1).map((value, index) =>
          requireU32(value, `recovery[${index}]`),
        ));
        return;
      case COMMAND_DISPOSE:
        requireMessageLength(message, 1, 'dispose');
        disposeAndClose(true);
        return;
      case COMMAND_DISPATCH_HOST_POLL:
        requireMessageLength(message, 2, 'dispatchHostPoll');
        endpoint.dispatch(requireFrame(message[1]), false, true, false, false);
        return;
      case COMMAND_DISPATCH_INLINE_SIDECAR_HOST_POLL:
        requireMessageLength(message, 2, 'dispatchInlineSidecarHostPoll');
        endpoint.dispatch(requireFrame(message[1]), false, false, true, false);
        return;
      case COMMAND_DISPATCH_VIEWPORT_PRESENTATION_HOST_POLL:
        requireMessageLength(
          message,
          2,
          'dispatchViewportPresentationHostPoll',
        );
        endpoint.dispatch(requireFrame(message[1]), false, false, false, true);
        return;
      case COMMAND_CHECKPOINT_B_PROBE:
        requireMessageLength(message, 1, 'checkpointBProbe');
        endpoint.runCheckpointBProbe();
        return;
      default:
        throw new WorkerFailure(
          'commandDecode',
          STATUS_INVALID,
          `unknown command ${command}`,
        );
    }
  }

  async function initialize(wasmUri) {
    let response;
    try {
      response = await fetch(wasmUri, { credentials: 'same-origin' });
    } catch (error) {
      throw new WorkerFailure('wasmFetch', STATUS_INTERNAL_FAULT, error);
    }
    if (!response.ok) {
      throw new WorkerFailure(
        'wasmFetch',
        STATUS_INTERNAL_FAULT,
        `${response.status} ${response.statusText}`,
      );
    }
    let instantiated;
    try {
      const bytes = await response.arrayBuffer();
      instantiated = await WebAssembly.instantiate(bytes, {});
    } catch (error) {
      throw new WorkerFailure('wasmInstantiate', STATUS_INTERNAL_FAULT, error);
    }
    const exports = instantiated.instance.exports;
    validateExports(exports);
    endpoint = new WasmEndpoint(exports);
    endpoint.initialize();
    initialized = true;
    self.postMessage([EVENT_READY]);
  }

  function validateExports(exports) {
    for (const name of REQUIRED_EXPORTS) {
      const value = exports[name];
      if (name === 'memory') {
        if (!(value instanceof WebAssembly.Memory)) {
          throw new WorkerFailure(
            'wasmExports',
            STATUS_INTERNAL_FAULT,
            'memory export is missing or invalid',
          );
        }
      } else if (typeof value !== 'function') {
        throw new WorkerFailure(
          'wasmExports',
          STATUS_INTERNAL_FAULT,
          `missing export ${name}`,
        );
      }
    }
  }

  function disposeAndClose(gracefulFirst) {
    if (terminal) return;
    terminal = true;
    const current = endpoint;
    endpoint = null;
    if (current === null) {
      self.postMessage([EVENT_DISPOSED]);
      self.close();
      return;
    }
    const result = current.dispose(gracefulFirst);
    if (result.status === STATUS_OK) {
      self.postMessage([EVENT_DISPOSED]);
    } else {
      self.postMessage([
        EVENT_RECLAMATION_UNPROVEN,
        result.operation,
        result.status,
        result.detail,
      ]);
    }
    self.close();
  }

  function failTerminally(error) {
    if (terminal) return;
    terminal = true;
    const failure = normalizeFailure(error);
    self.postMessage([
      EVENT_FAILURE,
      failure.operation,
      failure.status,
      failure.detail,
    ]);
    const current = endpoint;
    endpoint = null;
    if (current === null) {
      self.postMessage([EVENT_DISPOSED]);
      self.close();
      return;
    }
    const result = current.dispose(false);
    if (result.status === STATUS_OK) {
      // This receipt proves abnormal registry revocation. The Dart owner keeps
      // the original failure as the terminal result of its `done` future.
      self.postMessage([EVENT_DISPOSED]);
    } else {
      self.postMessage([
        EVENT_RECLAMATION_UNPROVEN,
        result.operation,
        result.status,
        result.detail,
      ]);
    }
    self.close();
  }

  function normalizeFailure(error) {
    if (error instanceof WorkerFailure) return error;
    const detail = error instanceof Error
      ? `${error.name}: ${error.message}`
      : String(error);
    return new WorkerFailure('workerCommand', STATUS_INTERNAL_FAULT, detail);
  }

  class WasmEndpoint {
    constructor(exports) {
      this.exports = exports;
      this.scratch = 0;
      this.slot = 0;
      this.generation = 0;
      this.orphanHandles = [];
      this.deliveredEventId = 0;
      this.deliveredEventKind = 0;
      this.deferred = null;
      this.pollScheduled = false;
      this.pollEpoch = 0;
      this.checkpointBProbeRan = false;
      this.disposed = false;
    }

    initialize() {
      const abi = this.call('flark_v3_wasm_endpoint_native_abi_version');
      if (abi !== ENDPOINT_ABI_VERSION) {
        throw new WorkerFailure(
          'abiVersion',
          abi,
          `expected 0x${ENDPOINT_ABI_VERSION.toString(16)}`,
        );
      }
      this.scratch = this.call('flark_v3_wasm_alloc', SCRATCH_BYTES);
      if (this.scratch === 0) {
        throw new WorkerFailure(
          'scratchAllocate',
          STATUS_INTERNAL_FAULT,
          `${SCRATCH_BYTES} bytes`,
        );
      }
      this.zero(this.scratch, SCRATCH_BYTES);
      const config = this.pointer(OFFSET_CONFIG);
      requireOk(
        'configStandard',
        this.call('flark_v3_wasm_endpoint_config_standard', config),
      );
      const data = this.data;
      if (
        data.getUint32(config, true) !== ENDPOINT_ABI_VERSION ||
        data.getUint32(config + 4, true) !== CONFIG_BYTES
      ) {
        throw new WorkerFailure(
          'configLayout',
          STATUS_INTERNAL_FAULT,
          `abi=${data.getUint32(config, true)} size=${data.getUint32(config + 4, true)}`,
        );
      }
      const handle = this.pointer(OFFSET_HANDLE);
      this.zero(handle, HANDLE_BYTES);
      requireOk(
        'create',
        this.call('flark_v3_wasm_endpoint_create', config, handle),
      );
      this.slot = data.getUint32(handle, true);
      this.generation = data.getUint32(handle + 4, true);
      this.requireHandle(this.slot, this.generation, 'create');
    }

    dispatch(
      frame,
      strictClose,
      hostPoll,
      inlineSidecarHostPoll,
      viewportPresentationHostPoll,
    ) {
      this.requireLive();
      if (
        Number(strictClose) +
          Number(hostPoll) +
          Number(inlineSidecarHostPoll) +
          Number(viewportPresentationHostPoll) >
        1
      ) {
        throw new WorkerFailure(
          'dispatch',
          STATUS_INVALID,
          'dispatch protocol routes are mutually exclusive',
        );
      }
      const input = this.pointer(OFFSET_INPUT);
      const receipt = this.pointer(OFFSET_DISPATCH_RECEIPT);
      this.copyFrameToMemory(frame, input);
      this.zero(receipt, DISPATCH_RECEIPT_BYTES);
      let operation;
      let status;
      if (viewportPresentationHostPoll) {
        operation = 'dispatchViewportPresentationHostPoll';
        status = this.call(
          'flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll',
          this.slot,
          this.generation,
          input,
          frame.byteLength,
          receipt,
        );
      } else if (inlineSidecarHostPoll) {
        operation = 'dispatchInlineSidecarHostPoll';
        status = this.call(
          'flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll',
          this.slot,
          this.generation,
          input,
          frame.byteLength,
          receipt,
        );
      } else if (hostPoll) {
        operation = 'dispatchHostPoll';
        status = this.call(
          'flark_v3_wasm_endpoint_dispatch_host_poll',
          this.slot,
          this.generation,
          input,
          frame.byteLength,
          receipt,
        );
      } else if (strictClose) {
        operation = 'strictClose';
        status = this.call(
          'flark_v3_wasm_endpoint_close',
          this.slot,
          this.generation,
          input,
          frame.byteLength,
          receipt,
        );
      } else {
        operation = 'dispatch';
        status = this.call(
          'flark_v3_wasm_endpoint_dispatch',
          this.slot,
          this.generation,
          input,
          frame.byteLength,
          receipt,
        );
      }
      const result = this.dispatchReceipt(receipt);
      if (result.outstandingEventId !== 0) {
        const newlyDelivered = this.isNewOutstanding(
          result.outstandingEventId,
          result.outstandingEventKind,
        );
        if (newlyDelivered) {
          this.emitOutstanding(
            result.outstandingEventId,
            result.outstandingEventKind,
          );
        }
        if (status === STATUS_BACKPRESSURE) {
          this.defer(
            frame,
            strictClose,
            hostPoll,
            inlineSidecarHostPoll,
            viewportPresentationHostPoll,
          );
        } else if (!newlyDelivered) {
          requireOk(`${operation}WithOutstandingCredit`, status);
        }
        if (
          !hostPoll &&
          !inlineSidecarHostPoll &&
          !viewportPresentationHostPoll &&
          status === STATUS_OK &&
          result.action === ACTION_CLOSE_LATCHED
        ) {
          this.deferred = null;
        }
        return;
      }

      const completedEventKind = this.deliveredEventKind;
      this.clearDelivered();
      requireOk(operation, status);
      if (
        !hostPoll &&
        !inlineSidecarHostPoll &&
        !viewportPresentationHostPoll &&
        result.action === ACTION_CLOSE_LATCHED
      ) {
        this.deferred = null;
      } else if (
        !hostPoll &&
        !inlineSidecarHostPoll &&
        !viewportPresentationHostPoll &&
        result.action === ACTION_EVENT_RECEIPT_ACCEPTED
      ) {
        const deferred = this.takeDeferred(completedEventKind);
        if (deferred !== null) {
          this.dispatch(
            deferred.frame,
            deferred.strictClose,
            deferred.hostPoll,
            deferred.inlineSidecarHostPoll,
            deferred.viewportPresentationHostPoll,
          );
          return;
        }
      }
      this.schedulePoll();
    }

    recover(words) {
      this.requireLive();
      if (words.length !== 6) {
        throw new WorkerFailure(
          'recover',
          STATUS_INVALID,
          'expected six recovery words',
        );
      }
      const binding = this.pointer(OFFSET_BINDING);
      const handle = this.pointer(OFFSET_HANDLE);
      this.zero(binding, BINDING_BYTES);
      const data = this.data;
      for (let index = 0; index < 6; index += 1) {
        data.setUint32(binding + index * 4, words[index], true);
      }
      this.zero(handle, HANDLE_BYTES);
      requireOk(
        'recoverCreateReplacement',
        this.call(
          'flark_v3_wasm_endpoint_recover',
          this.pointer(OFFSET_CONFIG),
          binding,
          handle,
        ),
      );
      const replacementSlot = data.getUint32(handle, true);
      const replacementGeneration = data.getUint32(handle + 4, true);
      this.requireHandle(
        replacementSlot,
        replacementGeneration,
        'recoverCreateReplacement',
      );

      const prior = { slot: this.slot, generation: this.generation };
      this.slot = replacementSlot;
      this.generation = replacementGeneration;
      const retirement = this.retire(prior, false, 'recoverRevokePrior');
      if (retirement.status !== STATUS_OK) {
        this.orphanHandles.push(prior);
        throw new WorkerFailure(
          retirement.operation,
          retirement.status,
          retirement.detail,
        );
      }
      this.clearDelivered();
      this.deferred = null;
      this.pollScheduled = false;
      this.pollEpoch += 1;
    }

    runCheckpointBProbe() {
      this.requireLive();
      if (this.checkpointBProbeRan) {
        throw new WorkerFailure(
          'checkpointBProbe',
          STATUS_INVALID_STATE,
          'probe is one-shot',
        );
      }
      this.checkpointBProbeRan = true;

      const exportName = 'flark_v3_wasm_checkpoint_b_probe';
      if (typeof this.exports[exportName] !== 'function') {
        throw new WorkerFailure(
          'checkpointBProbeExport',
          STATUS_INTERNAL_FAULT,
          `missing export ${exportName}`,
        );
      }

      const written = this.pointer(OFFSET_WRITTEN);
      const output = this.pointer(OFFSET_OUTPUT);
      this.zero(written, WRITTEN_BYTES);
      requireOk(
        'checkpointBProbe',
        this.call(
          exportName,
          output,
          CHECKPOINT_B_MAXIMUM_JSON_BYTES,
          written,
        ),
      );
      const actual = this.data.getUint32(written, true);
      if (actual === 0 || actual > CHECKPOINT_B_MAXIMUM_JSON_BYTES) {
        throw new WorkerFailure(
          'checkpointBProbeLength',
          STATUS_INTERNAL_FAULT,
          `actual=${actual}`,
        );
      }

      const owned = new Uint8Array(actual);
      owned.set(new Uint8Array(this.memory.buffer, output, actual));
      self.postMessage(
        [EVENT_CHECKPOINT_B_PROBE, owned.buffer],
        [owned.buffer],
      );
    }

    poll(epoch) {
      if (this.disposed || epoch !== this.pollEpoch) return;
      this.pollScheduled = false;
      const taskStarted = performance.now();
      const receipt = this.pointer(OFFSET_POLL_RECEIPT);
      this.zero(receipt, POLL_RECEIPT_BYTES);
      const status = this.call(
        'flark_v3_wasm_endpoint_poll',
        this.slot,
        this.generation,
        POLL_TASK_SOURCE_BYTE_QUOTA,
        POLL_CHECKPOINTS,
        POLL_RETIREMENT_TRANSITIONS,
        receipt,
      );
      const source = this.pollReceipt(receipt);
      if (source.outstandingEventId !== 0) {
        if (
          this.isNewOutstanding(
            source.outstandingEventId,
            source.outstandingEventKind,
          )
        ) {
          this.emitOutstanding(
            source.outstandingEventId,
            source.outstandingEventKind,
          );
        } else {
          requireOk('pollWithOutstandingCredit', status);
        }
        return;
      }
      this.clearDelivered();
      requireOk('poll', status);
      const sourceNeedsAnotherTurn =
        source.madeProgress && !source.scanComplete && !source.cleanupComplete;

      const candidateReceipt = this.pointer(OFFSET_CANDIDATE_RECEIPT);
      let candidateMadeProgress = false;
      let candidateNeedsAnotherTurn = false;
      let candidateTransitionGrantSpent = 0;
      do {
        const candidateGrant = Math.min(
          POLL_CANDIDATE_TRANSITIONS_PER_SUBGRANT,
          POLL_TASK_CANDIDATE_TRANSITION_QUOTA -
            candidateTransitionGrantSpent,
        );
        this.zero(candidateReceipt, CANDIDATE_RECEIPT_BYTES);
        const candidateStatus = this.call(
          'flark_v3_wasm_endpoint_poll_candidate',
          this.slot,
          this.generation,
          candidateGrant,
          candidateReceipt,
        );
        const candidate = this.candidateReceipt(candidateReceipt);
        candidateTransitionGrantSpent += candidateGrant;
        if (candidate.transitions > candidateGrant) {
          throw new WorkerFailure(
            'pollCandidateReceipt',
            STATUS_INTERNAL_FAULT,
            `transitions=${candidate.transitions} grant=${candidateGrant}`,
          );
        }
        if (candidate.outstandingEventId !== 0) {
          if (
            this.isNewOutstanding(
              candidate.outstandingEventId,
              candidate.outstandingEventKind,
            )
          ) {
            this.emitOutstanding(
              candidate.outstandingEventId,
              candidate.outstandingEventKind,
            );
          } else {
            requireOk('pollCandidateWithOutstandingCredit', candidateStatus);
          }
          return;
        }
        requireOk('pollCandidate', candidateStatus);
        candidateMadeProgress = candidate.transitions !== 0;
        candidateNeedsAnotherTurn = !candidate.cleanupComplete;
      } while (
        candidateMadeProgress &&
        candidateTransitionGrantSpent <
          POLL_TASK_CANDIDATE_TRANSITION_QUOTA &&
        performance.now() - taskStarted < POLL_TASK_BUDGET_MILLISECONDS
      );
      if (
        sourceNeedsAnotherTurn ||
        candidateMadeProgress ||
        candidateNeedsAnotherTurn
      ) {
        this.schedulePoll();
      }
    }

    emitOutstanding(eventId, eventKind) {
      const written = this.pointer(OFFSET_WRITTEN);
      this.zero(written, WRITTEN_BYTES);
      const queryStatus = this.call(
        'flark_v3_wasm_endpoint_encode',
        this.slot,
        this.generation,
        0,
        0,
        written,
      );
      const required = this.data.getUint32(written, true);
      if (
        queryStatus !== STATUS_FOREGROUND_BOUND_EXCEEDED ||
        required === 0 ||
        required > MAXIMUM_FRAME_BYTES
      ) {
        throw new WorkerFailure(
          'encodeSize',
          queryStatus,
          `required=${required}`,
        );
      }
      this.zero(written, WRITTEN_BYTES);
      const output = this.pointer(OFFSET_OUTPUT);
      requireOk(
        'encode',
        this.call(
          'flark_v3_wasm_endpoint_encode',
          this.slot,
          this.generation,
          output,
          required,
          written,
        ),
      );
      const actual = this.data.getUint32(written, true);
      if (actual !== required) {
        throw new WorkerFailure(
          'encodeLength',
          STATUS_INTERNAL_FAULT,
          `required=${required} actual=${actual}`,
        );
      }
      const owned = new Uint8Array(required);
      owned.set(new Uint8Array(this.memory.buffer, output, required));
      this.deliveredEventId = eventId;
      this.deliveredEventKind = eventKind;
      self.postMessage([EVENT_FRAME, owned.buffer], [owned.buffer]);
    }

    schedulePoll() {
      if (this.disposed || this.pollScheduled || this.deferred !== null) return;
      this.pollScheduled = true;
      const epoch = this.pollEpoch;
      pollContinuationChannel.port2.postMessage(epoch);
    }

    defer(
      frame,
      strictClose,
      hostPoll,
      inlineSidecarHostPoll,
      viewportPresentationHostPoll,
    ) {
      const blocked = this.deferred;
      if (blocked !== null) {
        if (
          blocked.strictClose === strictClose &&
          blocked.hostPoll === hostPoll &&
          blocked.inlineSidecarHostPoll === inlineSidecarHostPoll &&
          blocked.viewportPresentationHostPoll ===
            viewportPresentationHostPoll &&
          sameBytes(blocked.frame, frame)
        ) {
          return;
        }
        throw new WorkerFailure(
          'deferredDispatch',
          STATUS_INVALID_STATE,
          'endpoint exceeded one deferred command',
        );
      }
      this.deferred = {
        frame,
        strictClose,
        hostPoll,
        inlineSidecarHostPoll,
        viewportPresentationHostPoll,
      };
    }

    takeDeferred(completedEventKind) {
      const blocked = this.deferred;
      this.deferred = null;
      if (
        completedEventKind === EVENT_KIND_FAILED ||
        completedEventKind === EVENT_KIND_CLOSED
      ) {
        return null;
      }
      return blocked;
    }

    dispose(gracefulFirst) {
      if (this.disposed) {
        return { status: STATUS_OK, operation: 'dispose', detail: null };
      }
      this.disposed = true;
      this.deferred = null;
      this.pollScheduled = false;
      this.pollEpoch += 1;

      let failure = null;
      if (this.slot !== 0 && this.generation !== 0) {
        const result = this.retire(
          { slot: this.slot, generation: this.generation },
          gracefulFirst,
          gracefulFirst ? 'dispose' : 'abandon',
        );
        if (result.status !== STATUS_OK) failure = result;
        else {
          this.slot = 0;
          this.generation = 0;
        }
      }
      for (const orphan of this.orphanHandles) {
        const result = this.retire(orphan, false, 'disposeOrphan');
        if (failure === null && result.status !== STATUS_OK) failure = result;
      }
      this.orphanHandles = [];

      if (failure !== null) return failure;
      if (this.scratch !== 0) {
        const status = this.call(
          'flark_v3_wasm_free',
          this.scratch,
          SCRATCH_BYTES,
        );
        if (status !== STATUS_OK) {
          return {
            status,
            operation: 'scratchFree',
            detail: 'Wasm scratch ownership was not released',
          };
        }
        this.scratch = 0;
      }
      return { status: STATUS_OK, operation: 'dispose', detail: null };
    }

    retire(handle, gracefulFirst, operation) {
      let status = gracefulFirst
        ? this.call(
          'flark_v3_wasm_endpoint_remove',
          handle.slot,
          handle.generation,
        )
        : this.call(
          'flark_v3_wasm_endpoint_emergency_destroy',
          handle.slot,
          handle.generation,
        );
      if (gracefulFirst && status !== STATUS_OK) {
        status = this.call(
          'flark_v3_wasm_endpoint_emergency_destroy',
          handle.slot,
          handle.generation,
        );
      }
      return {
        status,
        operation,
        detail: status === STATUS_OK
          ? null
          : 'Wasm endpoint registry removal was not proven',
      };
    }

    dispatchReceipt(pointer) {
      const data = this.data;
      return {
        correlationId: data.getUint32(pointer, true),
        action: data.getUint32(pointer + 4, true),
        outstandingEventId: data.getUint32(pointer + 8, true),
        outstandingEventKind: data.getUint32(pointer + 12, true),
      };
    }

    pollReceipt(pointer) {
      const data = this.data;
      let madeProgress = false;
      for (let offset = 0; offset <= 48; offset += 4) {
        madeProgress = madeProgress || data.getUint32(pointer + offset, true) !== 0;
      }
      return {
        madeProgress,
        sourceFactTransitions: data.getUint32(pointer + 20, true),
        scanComplete: data.getUint32(pointer + 40, true) !== 0,
        cleanupComplete: data.getUint32(pointer + 48, true) !== 0,
        outstandingEventId: data.getUint32(pointer + 52, true),
        outstandingEventKind: data.getUint32(pointer + 56, true),
      };
    }

    candidateReceipt(pointer) {
      const data = this.data;
      return {
        transitions: data.getUint32(pointer, true),
        cleanupComplete: data.getUint32(pointer + 4, true) !== 0,
        outstandingEventId: data.getUint32(pointer + 8, true),
        outstandingEventKind: data.getUint32(pointer + 12, true),
      };
    }

    copyFrameToMemory(frame, pointer) {
      this.checkRange(pointer, frame.byteLength);
      new Uint8Array(this.memory.buffer, pointer, frame.byteLength).set(
        new Uint8Array(frame),
      );
    }

    isNewOutstanding(eventId, eventKind) {
      return eventId !== this.deliveredEventId ||
        eventKind !== this.deliveredEventKind;
    }

    clearDelivered() {
      this.deliveredEventId = 0;
      this.deliveredEventKind = 0;
    }

    requireHandle(slot, generation, operation) {
      if (slot === 0 || generation === 0) {
        throw new WorkerFailure(
          operation,
          STATUS_INTERNAL_FAULT,
          `invalid handle ${slot}/${generation}`,
        );
      }
    }

    requireLive() {
      if (this.disposed || this.slot === 0 || this.generation === 0) {
        throw new WorkerFailure(
          'endpointState',
          STATUS_INVALID_STATE,
          'Wasm endpoint is unavailable',
        );
      }
    }

    call(name, ...arguments_) {
      for (const argument of arguments_) {
        requireU32(argument, `${name} argument`);
      }
      const result = this.exports[name](...arguments_);
      return requireU32(result, `${name} result`);
    }

    pointer(offset) {
      const pointer = this.scratch + offset;
      this.checkRange(pointer, 0);
      return pointer;
    }

    zero(pointer, length) {
      this.checkRange(pointer, length);
      new Uint8Array(this.memory.buffer, pointer, length).fill(0);
    }

    checkRange(pointer, length) {
      requireU32(pointer, 'memory pointer');
      requireU32(length, 'memory length');
      const end = pointer + length;
      if (
        !Number.isSafeInteger(end) ||
        end < pointer ||
        end > this.memory.buffer.byteLength
      ) {
        throw new WorkerFailure(
          'memoryRange',
          STATUS_INTERNAL_FAULT,
          `[${pointer}, ${end}) of ${this.memory.buffer.byteLength}`,
        );
      }
    }

    get memory() {
      return this.exports.memory;
    }

    get data() {
      return new DataView(this.memory.buffer);
    }
  }

  function requireOk(operation, status) {
    if (status !== STATUS_OK) throw new WorkerFailure(operation, status, null);
  }

  function requireArray(value, name) {
    if (!Array.isArray(value)) {
      throw new WorkerFailure(
        'messageDecode',
        STATUS_INVALID,
        `${name} is not an Array`,
      );
    }
  }

  function requireMessageLength(message, expected, operation) {
    if (message.length !== expected) {
      throw new WorkerFailure(
        operation,
        STATUS_INVALID,
        `expected ${expected} fields, got ${message.length}`,
      );
    }
  }

  function requireU32(value, name) {
    if (
      typeof value !== 'number' ||
      !Number.isInteger(value) ||
      value < 0 ||
      value > 0xffffffff
    ) {
      throw new WorkerFailure(
        'numericContract',
        STATUS_INVALID,
        `${name} is not a u32`,
      );
    }
    return value >>> 0;
  }

  function requireString(value, name) {
    if (typeof value !== 'string' || value.length === 0) {
      throw new WorkerFailure(
        'messageDecode',
        STATUS_INVALID,
        `${name} is not a non-empty string`,
      );
    }
    return value;
  }

  function requireFrame(value) {
    if (
      !(value instanceof ArrayBuffer) ||
      value.byteLength === 0 ||
      value.byteLength > MAXIMUM_FRAME_BYTES
    ) {
      throw new WorkerFailure(
        'frameBounds',
        STATUS_INVALID,
        'command frame is not one bounded ArrayBuffer',
      );
    }
    return value;
  }

  function sameBytes(left, right) {
    if (left.byteLength !== right.byteLength) return false;
    const leftBytes = new Uint8Array(left);
    const rightBytes = new Uint8Array(right);
    for (let index = 0; index < leftBytes.length; index += 1) {
      if (leftBytes[index] !== rightBytes[index]) return false;
    }
    return true;
  }
})();
