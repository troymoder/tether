use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Reflect;
use protocol::CameraFrame;
use wasm_bindgen::prelude::*;
use web_sys::{
    DedicatedWorkerGlobalScope, EncodedVideoChunk, LatencyMode, OffscreenCanvas, VideoEncoder,
    VideoEncoderConfig, VideoEncoderEncodeOptions, VideoEncoderInit, VideoFrame, VideoFrameInit,
};

use super::job::Job;
use super::sink::Sink;
use super::told;
use crate::link::capture::object;

pub(super) struct Encoding {
    pub(super) encoder: VideoEncoder,
    pub(super) size: (u32, u32),
    frames: u32,
    _output: Closure<dyn Fn(EncodedVideoChunk)>,
    _error: Closure<dyn Fn(JsValue)>,
}

impl Encoding {
    /// Baseline H.264 has no frames that depend on later ones. Annex B is the
    /// form ffmpeg reads from a pipe.
    pub(super) fn start(
        job: Job,
        sink: Rc<RefCell<Option<Sink>>>,
        page: &DedicatedWorkerGlobalScope,
    ) -> Option<Self> {
        let width = job.width;
        let height = job.height;
        let output = {
            let page = page.clone();

            Closure::<dyn Fn(EncodedVideoChunk)>::new(move |chunk: EncodedVideoChunk| {
                let mut data = vec![0; chunk.byte_length() as usize];

                if chunk.copy_to_with_u8_slice(&mut data).is_err() {
                    return;
                }

                let frame = CameraFrame {
                    width: width as u16,
                    height: height as u16,
                    data,
                };
                let sent = sink.borrow().as_ref().and_then(|sink| sink.send(&frame));

                if let Some(bytes) = sent {
                    told(&page, bytes, None);
                }
            })
        };

        // After an error the encoder closes itself, and the next frame makes
        // a new one.
        let error = Closure::<dyn Fn(JsValue)>::new(|error: JsValue| {
            web_sys::console::error_2(&"camera encoder failed:".into(), &error);
        });

        let encoder = VideoEncoder::new(&VideoEncoderInit::new(
            error.as_ref().unchecked_ref(),
            output.as_ref().unchecked_ref(),
        ))
        .ok()?;
        let area = width * height;
        // 3.1 up to 1280 × 720, 4.0 up to 2048 × 1080, 5.1 beyond.
        let level = if area <= 921_600 {
            "1f"
        } else if area <= 2_088_960 {
            "28"
        } else {
            "33"
        };

        let config = VideoEncoderConfig::new(&format!("avc1.4200{level}"), height, width);
        let fps = f64::from(job.fps.max(1));

        config.set_framerate(fps);
        config.set_bitrate_f64(f64::from(area) * fps / 8.0);
        config.set_latency_mode(LatencyMode::Realtime);

        let avc = object(&[("format", "annexb".into())]);
        let _ = Reflect::set(&config, &"avc".into(), &avc);

        encoder.configure(&config).ok()?;

        Some(Self {
            encoder,
            size: (width, height),
            frames: 0,
            _output: output,
            _error: error,
        })
    }

    /// A key frame comes first and every two seconds of frames, where ffmpeg
    /// can start decoding again. A frame is skipped while the encoder is
    /// behind. The skip happens before encoding, so no later frame refers to it.
    pub(super) fn encode(&mut self, drawn: &OffscreenCanvas, job: Job) {
        if self.encoder.encode_queue_size() > 2 {
            return;
        }

        let init = VideoFrameInit::new();

        init.set_timestamp_f64(js_sys::Date::now() * 1000.0);

        let Ok(frame) = VideoFrame::new_with_offscreen_canvas_and_video_frame_init(drawn, &init)
        else {
            return;
        };

        let options = VideoEncoderEncodeOptions::new();

        options.set_key_frame(self.frames.is_multiple_of(u32::from(job.fps.max(1)) * 2));

        let _ = self.encoder.encode_with_options(&frame, &options);

        frame.close();
        self.frames = self.frames.wrapping_add(1);
    }
}

impl Drop for Encoding {
    fn drop(&mut self) {
        let _ = self.encoder.close();
    }
}
