use tokio::task::AbortHandle;

pub(crate) struct Stopped(pub(crate) AbortHandle);

impl Drop for Stopped {
    fn drop(&mut self) {
        self.0.abort();
    }
}
