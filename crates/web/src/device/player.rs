use js_sys::Uint8Array;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    EncodedVideoChunk, EncodedVideoChunkInit, EncodedVideoChunkType, VideoDecoder,
    VideoDecoderConfig, VideoDecoderInit, VideoFrame,
};

pub(crate) struct Player {
    decoder: Option<VideoDecoder>,
    /// A decoder can only start at a key frame.
    waiting_for_key: bool,
    drawn: Closure<dyn Fn(VideoFrame)>,
    failed: Closure<dyn Fn(JsValue)>,
}

impl Player {
    /// `drawn` must close each frame it is handed.
    pub(crate) fn new(
        drawn: impl Fn(VideoFrame) + 'static,
        failed: impl Fn(JsValue) + 'static,
    ) -> Self {
        Player {
            decoder: None,
            waiting_for_key: true,
            drawn: Closure::new(drawn),
            failed: Closure::new(failed),
        }
    }

    pub(crate) fn start(&mut self, codec: &str) {
        self.close();

        let init = VideoDecoderInit::new(
            self.failed.as_ref().unchecked_ref(),
            self.drawn.as_ref().unchecked_ref(),
        );

        let config = VideoDecoderConfig::new(codec);

        config.set_optimize_for_latency(true);

        if let Ok(decoder) = VideoDecoder::new(&init)
            && decoder.configure(&config).is_ok()
        {
            self.decoder = Some(decoder);
            self.waiting_for_key = true;
        }
    }

    pub(crate) fn decode(&mut self, key: bool, time: u64, data: &[u8]) {
        if self.waiting_for_key && !key {
            return;
        }

        let Some(decoder) = &self.decoder else {
            return;
        };

        // SAFETY: the view into WebAssembly memory is only valid until that
        // memory grows. The chunk copies it before anything here allocates.
        let data = unsafe { Uint8Array::view(data) };
        let kind = if key {
            EncodedVideoChunkType::Key
        } else {
            EncodedVideoChunkType::Delta
        };

        let init = EncodedVideoChunkInit::new(&data, 0, kind);

        // `new` takes the timestamp as an i32, which is too small for
        // microseconds.
        init.set_timestamp_f64(time as f64);

        let decoded = EncodedVideoChunk::new(&init).and_then(|chunk| decoder.decode(&chunk));

        self.waiting_for_key = decoded.is_err();
    }

    pub(crate) fn close(&mut self) {
        if let Some(decoder) = self.decoder.take() {
            let _ = decoder.close();
        }
    }
}
