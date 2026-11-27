use std::time::Duration;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MediaStream, RtcPeerConnection, RtcPeerConnectionState, RtcRtpReceiver, RtcRtpTransceiver,
    RtcRtpTransceiverDirection, RtcRtpTransceiverInit, RtcSdpType, RtcSessionDescriptionInit,
};

use super::Link;
use crate::util::sleep;

/// Dropping it removes the tracks from the connection.
pub(crate) struct Tracks {
    link: Link,
    pc: RtcPeerConnection,
    video: RtcRtpTransceiver,
    audio: RtcRtpTransceiver,
    pub(crate) stream: MediaStream,
}

impl Tracks {
    pub(crate) fn mids(&self) -> Option<(String, String)> {
        Some((self.video.mid()?, self.audio.mid()?))
    }

    pub(crate) fn video(&self) -> RtcRtpReceiver {
        self.video.receiver()
    }
}

impl Drop for Tracks {
    fn drop(&mut self) {
        // A closed connection, as when the link to tetherd dropped, has
        // nothing left to remove, and stopping a transceiver on it throws.
        if self.pc.connection_state() == RtcPeerConnectionState::Closed {
            return;
        }

        self.video.stop();
        self.audio.stop();

        let link = self.link;
        let pc = self.pc.clone();

        leptos::task::spawn_local(async move {
            if let Err(error) = link.renegotiate(&pc).await {
                link.say(error);
            }
        });
    }
}

impl Link {
    pub(crate) async fn tracks(self) -> Result<Tracks, String> {
        let pc = self
            .peer()
            .ok_or_else(|| "screen failed: not connected to the server".to_owned())?;
        let receive = || {
            let init = RtcRtpTransceiverInit::new();

            init.set_direction(RtcRtpTransceiverDirection::Recvonly);
            init
        };
        let video = pc.add_transceiver_with_str_and_init("video", &receive());
        let audio = pc.add_transceiver_with_str_and_init("audio", &receive());

        // Frames show as soon as they decode. There is little jitter between
        // tetherd and this browser to smooth over with a buffer.
        for receiver in [video.receiver(), audio.receiver()] {
            let _ = js_sys::Reflect::set(&receiver, &"jitterBufferTarget".into(), &0.into());
        }

        let stream = MediaStream::new().map_err(|_| "screen failed: no stream".to_owned())?;

        stream.add_track(&video.receiver().track());
        stream.add_track(&audio.receiver().track());

        let tracks = Tracks {
            link: self,
            pc: pc.clone(),
            video,
            audio,
            stream,
        };

        self.renegotiate(&pc).await?;

        Ok(tracks)
    }

    async fn renegotiate(self, pc: &RtcPeerConnection) -> Result<(), String> {
        while self.connection(|connection| connection.offering.replace(true)) {
            sleep(Duration::from_millis(20)).await;
        }

        let done = self.offer_now(pc).await;

        self.connection(|connection| connection.offering.set(false));

        done
    }

    async fn offer_now(self, pc: &RtcPeerConnection) -> Result<(), String> {
        let failed = |error: JsValue| format!("webrtc failed: {error:?}");
        let offer = JsFuture::from(pc.create_offer()).await.map_err(failed)?;

        JsFuture::from(pc.set_local_description(offer.unchecked_ref()))
            .await
            .map_err(failed)?;

        let sdp = pc
            .local_description()
            .map(|description| description.sdp())
            .ok_or_else(|| "webrtc failed: no offer".to_owned())?;
        let answer = self.offer(sdp).await?;
        let description = RtcSessionDescriptionInit::new(RtcSdpType::Answer);

        description.set_sdp(&answer);
        JsFuture::from(pc.set_remote_description(&description))
            .await
            .map_err(failed)?;

        Ok(())
    }
}
