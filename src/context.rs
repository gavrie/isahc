use crate::error::Error;
use crossbeam_utils::atomic::AtomicCell;
use once_cell::sync::OnceCell;
use std::sync::Arc;

/// Shared state for a single transfer that enables communication between a
/// request handler, a response body stream, and user-facing response methods.
#[derive(Clone, Default)]
pub(crate) struct RequestContext(Arc<Inner>);

#[derive(Default)]
struct Inner {
    /// Set to the final result of the transfer. This is used to communicate an
    /// error while reading the response body if the handler suddenly aborts.
    result: OnceCell<Result<(), Error>>,

    /// Set to true if the user requests the transfer to be aborted prematurely.
    /// This is used in the opposite manner as the above flag; if set, then this
    /// communicates to the handler to stop running since the user has lost
    /// interest in this request.
    aborted: AtomicCell<bool>,
}

impl RequestContext {
    #[inline]
    pub(crate) fn result(&self) -> Option<Result<(), &Error>> {
        self.0.result.get().map(|result| match result {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        })
    }

    #[inline]
    pub(crate) fn set_result(&self, result: Result<(), Error>) -> Result<(), Result<(), Error>> {
        self.0.result.set(result)
    }

    #[inline]
    pub(crate) fn is_aborted(&self) -> bool {
        self.0.aborted.load()
    }

    #[inline]
    pub(crate) fn abort(&self) {
        self.0.aborted.store(true);
    }
}
