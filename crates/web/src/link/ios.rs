//! tetherd performs each operation with the phone's pair record. An opened
//! service's stream comes back on the operation's channel, which the `ios`
//! crate reads as a pipe.

use futures_util::StreamExt;
use ios::pipe::{Pipe, pipe};
use leptos::task::spawn_local;
use protocol::{DeviceId, IosOperation, Media, access, own};

use super::Link;

type Answer = Result<Vec<u8>, String>;

impl Link {
    /// tetherd has already done any TLS on the stream.
    pub(crate) async fn ios_open(self, device: DeviceId, service: &str) -> Result<Pipe, String> {
        let operation = IosOperation::Open {
            service: service.to_owned(),
        };
        let Some(lane) = self.lane(&Media::Ios { device, operation }) else {
            return Err("ios failed: not connected to the server".to_owned());
        };
        let Some(first) = lane.next().await else {
            return Err("ios failed: connection to the server lost".to_owned());
        };

        access::<Answer>(&first)
            .map(own::<Answer>)
            .map_err(|error| format!("ios failed: {error}"))??;

        let (pipe, far) = pipe();
        let mut heard = lane.heard();
        let incoming = far.incoming;
        let mut outgoing = far.outgoing;

        spawn_local(async move {
            while let Some(message) = heard.next().await {
                if incoming.unbounded_send(message).is_err() {
                    break;
                }
            }
        });

        // Dropping the stream ends this task, and the lane with it, which
        // closes the channel.
        spawn_local(async move {
            while let Some(data) = outgoing.next().await {
                if !lane.send(&data) {
                    break;
                }
            }
        });

        Ok(pipe)
    }
}
