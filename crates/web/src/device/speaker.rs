use js_sys::Uint8Array;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AudioData, AudioDecoder, AudioDecoderConfig, AudioDecoderInit, EncodedAudioChunk,
    EncodedAudioChunkInit, EncodedAudioChunkType, MediaStreamTrackGenerator,
    MediaStreamTrackGeneratorInit, WritableStreamDefaultWriter,
};

pub(super) struct Speaker {
    decoder: AudioDecoder,
    writer: WritableStreamDefaultWriter,
    pub(super) track: MediaStreamTrackGenerator,
    _heard: Closure<dyn Fn(AudioData)>,
    _failed: Closure<dyn Fn(JsValue)>,
}

impl Speaker {
    pub(super) fn start() -> Option<Speaker> {
        let track =
            MediaStreamTrackGenerator::new(&MediaStreamTrackGeneratorInit::new("audio")).ok()?;
        let writer = track.writable().get_writer().ok()?;
        // The track takes ownership of each `AudioData` and closes it.
        let heard = {
            let writer = writer.clone();

            Closure::new(move |sound: AudioData| {
                let _ = writer.write_with_chunk(&sound);
            })
        };

        // A packet that fails to decode is left as a gap in the sound.
        let failed = Closure::new(|_: JsValue| {});
        let init = AudioDecoderInit::new(
            failed.as_ref().unchecked_ref(),
            heard.as_ref().unchecked_ref(),
        );
        let decoder = AudioDecoder::new(&init).ok()?;

        // scrcpy sends 48 kHz stereo Opus.
        decoder
            .configure(&AudioDecoderConfig::new("opus", 2, 48_000))
            .ok()?;

        Some(Speaker {
            decoder,
            writer,
            track,
            _heard: heard,
            _failed: failed,
        })
    }

    pub(super) fn decode(&self, time: u64, data: &[u8]) {
        // SAFETY: as for the video, the chunk copies the view before anything
        // here can allocate.
        let data = unsafe { Uint8Array::view(data) };
        // Every Opus packet decodes on its own.
        let init = EncodedAudioChunkInit::new(&data, 0, EncodedAudioChunkType::Key);

        init.set_timestamp_f64(time as f64);

        if let Ok(chunk) = EncodedAudioChunk::new(&init) {
            let _ = self.decoder.decode(&chunk);
        }
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        let _ = self.decoder.close();
        let _ = self.writer.close();
    }
}
