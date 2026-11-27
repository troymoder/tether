use leptos::prelude::*;
use protocol::{DeviceId, ToServer};

use super::Link;

pub(crate) fn unimplemented(what: &str) -> String {
    format!("unimplemented: {what}")
}

impl Link {
    /// `about` is the device it was done to, which may not be the picked one
    /// by the time it finishes. tetherd already logs the calls made to it.
    pub(crate) fn did(self, what: &str, about: Option<&DeviceId>, result: Result<String, String>) {
        let message = match &result {
            Ok(said) | Err(said) => said.clone(),
        };

        self.noted(what, about, result);
        self.say(message);
    }

    /// Logs without showing, for callers that show the result themselves.
    pub(crate) fn noted(
        self,
        what: &str,
        about: Option<&DeviceId>,
        result: Result<String, String>,
    ) {
        let (failed, message) = match result {
            Ok(said) => (false, said),
            Err(said) => (true, said),
        };

        self.send(&ToServer::Event {
            what: what.to_owned(),
            failed,
            message,
            about: about.cloned(),
        });
    }

    pub(crate) fn unimplemented(self, what: &str) {
        self.say(unimplemented(what));
    }

    pub(crate) fn say(self, text: impl Into<String>) {
        self.notify.run(untrack(|| self.mentioned(&text.into())));
    }
}
