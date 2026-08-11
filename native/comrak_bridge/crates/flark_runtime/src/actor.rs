use std::fmt;
use std::ops::Range;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use crate::{
    DocumentCloseReceipt, DocumentEditReceipt, DocumentLiveViewport, DocumentPumpReceipt,
    DocumentSession, DocumentSessionError, DocumentSessionPhase, DocumentViewport,
};

const DOCUMENT_ACTOR_STACK_BYTES: usize = 16 * 1024 * 1024;

/// A job runs against the session when one is live, and is told when the
/// actor has already contained a panic so it can report that instead. It
/// returns whether it contained a panic, which poisons the actor.
type ActorJob = Box<dyn FnOnce(Option<&mut DocumentSession>) -> bool + Send + 'static>;

enum ActorCommand {
    Run(ActorJob),
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentActorInspection {
    pub revision: u64,
    pub source_byte_len: usize,
    pub source_utf16_len: usize,
    pub phase: DocumentSessionPhase,
}

#[derive(Debug)]
pub enum DocumentActorError {
    Spawn(std::io::Error),
    Session(DocumentSessionError),
    Closed,
    /// A job unwound inside the document actor. The panic was contained, the
    /// session was discarded, and every later call reports this rather than
    /// touching state whose invariants a partial mutation may have broken.
    Panicked,
}

impl fmt::Display for DocumentActorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "could not start document actor: {error}"),
            Self::Session(error) => error.fmt(formatter),
            Self::Closed => formatter.write_str("document actor is closed"),
            Self::Panicked => {
                formatter.write_str("document actor contained a panic and is faulted")
            }
        }
    }
}

impl std::error::Error for DocumentActorError {}

impl From<DocumentSessionError> for DocumentActorError {
    fn from(value: DocumentSessionError) -> Self {
        Self::Session(value)
    }
}

/// A native-stack owner for one mutable document session.
///
/// Dart and Flutter native callbacks can have substantially smaller stacks
/// than Rust's parser plans require. Keeping the move-heavy parser state on a
/// dedicated Rust thread also gives every foreign-language host the same
/// serialized document-actor boundary.
pub struct DocumentActor {
    commands: SyncSender<ActorCommand>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for DocumentActor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentActor")
            .finish_non_exhaustive()
    }
}

impl DocumentActor {
    pub fn begin(source: String) -> Result<Self, DocumentActorError> {
        let (commands, receiver) = mpsc::sync_channel(0);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(0);
        let thread = thread::Builder::new()
            .name("flark-document".to_owned())
            .stack_size(DOCUMENT_ACTOR_STACK_BYTES)
            .spawn(move || run_document_actor(source, receiver, startup_sender))
            .map_err(DocumentActorError::Spawn)?;
        match startup_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(DocumentActorError::Session(error))
            }
            Err(_) => {
                let _ = thread.join();
                Err(DocumentActorError::Closed)
            }
        }
    }

    pub fn inspect(&self) -> Result<DocumentActorInspection, DocumentActorError> {
        self.call(|document| {
            Ok(DocumentActorInspection {
                revision: document.revision(),
                source_byte_len: document.source_byte_len(),
                source_utf16_len: document.source_utf16_len(),
                phase: document.phase(),
            })
        })
    }

    pub fn pump(&self, max_work_units: usize) -> Result<DocumentPumpReceipt, DocumentActorError> {
        self.call(move |document| document.pump(max_work_units))
    }

    pub fn apply_edit(
        &self,
        expected_revision: u64,
        range: Range<usize>,
        replacement: String,
    ) -> Result<DocumentEditReceipt, DocumentActorError> {
        self.call(move |document| document.apply_edit(expected_revision, range, &replacement))
    }

    pub fn source_bytes(&self, range: Range<usize>) -> Result<Vec<u8>, DocumentActorError> {
        self.call(move |document| document.source_bytes(range))
    }

    /// Moves a byte offset back to the nearest UTF-8 scalar boundary. Hosts
    /// and the ABI cap byte ranges against buffer and work budgets; only the
    /// runtime knows where a cut is legal.
    pub fn snapped_to_scalar_boundary(&self, offset: usize) -> Result<usize, DocumentActorError> {
        self.call(move |document| document.snapped_to_scalar_boundary(offset))
    }

    pub fn byte_offset_for_utf16(&self, offset: usize) -> Result<usize, DocumentActorError> {
        self.call(move |document| document.byte_offset_for_utf16(offset))
    }

    pub fn utf16_offset_for_byte(&self, offset: usize) -> Result<usize, DocumentActorError> {
        self.call(move |document| document.utf16_offset_for_byte(offset))
    }

    pub fn query_viewport(
        &self,
        revision: u64,
        requested_range: Range<usize>,
        maximum_rows: u32,
    ) -> Result<DocumentViewport, DocumentActorError> {
        self.call(move |document| document.query_viewport(revision, requested_range, maximum_rows))
    }

    pub fn query_live_viewport(
        &self,
        revision: u64,
        requested_range: Range<usize>,
        maximum_spans: u32,
    ) -> Result<DocumentLiveViewport, DocumentActorError> {
        self.call(move |document| {
            document.query_live_viewport(revision, requested_range, maximum_spans)
        })
    }

    pub fn begin_close(&self) -> Result<(), DocumentActorError> {
        self.call(DocumentSession::begin_close)
    }

    pub fn pump_close(
        &self,
        max_work_units: usize,
    ) -> Result<DocumentCloseReceipt, DocumentActorError> {
        self.call(move |document| document.pump_close(max_work_units))
    }

    /// Test-only entry point for driving a job whose behaviour, including an
    /// unwind, is the subject under test.
    #[doc(hidden)]
    pub fn call_for_test<T, F>(&self, operation: F) -> Result<T, DocumentActorError>
    where
        T: Send + 'static,
        F: FnOnce(&mut DocumentSession) -> Result<T, DocumentSessionError> + Send + 'static,
    {
        self.call(operation)
    }

    fn call<T, F>(&self, operation: F) -> Result<T, DocumentActorError>
    where
        T: Send + 'static,
        F: FnOnce(&mut DocumentSession) -> Result<T, DocumentSessionError> + Send + 'static,
    {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(0);
        self.commands
            .send(ActorCommand::Run(Box::new(move |document| {
                // Engine work runs behind a panic barrier here as well as at
                // the ABI entrypoint: a panic on this thread would otherwise
                // kill the actor silently and degrade every later call to an
                // anonymous internal fault.
                let (reply, panicked) = match document {
                    None => (Err(DocumentActorError::Panicked), false),
                    Some(document) => {
                        match catch_unwind(AssertUnwindSafe(|| operation(document))) {
                            Ok(result) => (result.map_err(DocumentActorError::Session), false),
                            Err(_) => (Err(DocumentActorError::Panicked), true),
                        }
                    }
                };
                let _ = reply_sender.send(reply);
                panicked
            })))
            .map_err(|_| DocumentActorError::Closed)?;
        reply_receiver
            .recv()
            .map_err(|_| DocumentActorError::Closed)?
    }
}

impl Drop for DocumentActor {
    fn drop(&mut self) {
        let _ = self.commands.send(ActorCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_document_actor(
    source: String,
    commands: Receiver<ActorCommand>,
    startup: SyncSender<Result<(), DocumentSessionError>>,
) {
    let mut document = match DocumentSession::begin(&source) {
        Ok(document) => {
            let _ = startup.send(Ok(()));
            Some(document)
        }
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    while let Ok(command) = commands.recv() {
        match command {
            ActorCommand::Run(operation) => {
                if operation(document.as_mut()) {
                    // A contained panic may have left the session mid
                    // transition, so it is discarded rather than reused, and
                    // its destructor runs behind the same barrier.
                    discard(document.take());
                }
            }
            ActorCommand::Shutdown => break,
        }
    }
    discard(document.take());
}

fn discard(document: Option<DocumentSession>) {
    if let Some(document) = document {
        let _ = catch_unwind(AssertUnwindSafe(move || drop(document)));
    }
}
