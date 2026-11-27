//! A second thread turns a frame of the playing source into the frame an
//! emulator's camera gets, so the page itself is never held up. The worker
//! sends the result itself on the camera's channel, which the page transfers
//! to it (sink.rs), so no picture goes back to the page. Every frame is
//! answered with `{ done }`, which tells the page the next one can come.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Reflect, Uint8Array};
use protocol::{CameraFrame, Framing};
use wasm_bindgen::prelude::*;
use web_sys::{
    CodecState, DedicatedWorkerGlobalScope, MessageEvent, OffscreenCanvas,
    OffscreenCanvasRenderingContext2d, RtcDataChannel, VideoFrame,
};

mod encoding;
mod job;
mod picture;
mod sink;

use encoding::Encoding;
use job::Job;
use picture::{Packing, as_camera_has};
use sink::Sink;

use super::capture::object;

fn told(page: &DedicatedWorkerGlobalScope, bytes: usize, took: Option<f64>) {
    let _ = page.post_message(&object(&[
        ("bytes", (bytes as f64).into()),
        ("took", took.into()),
    ]));
}

fn draw(
    drawn: &OffscreenCanvas,
    pen: &OffscreenCanvasRenderingContext2d,
    frame: &VideoFrame,
    job: Job,
) -> Option<()> {
    let wide = frame.display_width();
    let high = frame.display_height();

    if wide == 0 || high == 0 {
        return None;
    }

    if drawn.width() != job.width || drawn.height() != job.height {
        drawn.set_width(job.width);
        drawn.set_height(job.height);
    }

    let width = f64::from(job.width);
    let height = f64::from(job.height);
    let [a, b, c, d, e, f] = job.transform(wide.into(), high.into());

    pen.reset_transform().ok()?;
    pen.set_fill_style_str("black");
    pen.fill_rect(0.0, 0.0, width, height);
    pen.set_transform(a, b, c, d, e, f).ok()?;
    pen.draw_image_with_video_frame_and_dw_and_dh(frame, 0.0, 0.0, wide.into(), high.into())
        .ok()?;
    pen.reset_transform().ok()?;

    // Fills black beside the strip an upright device sees.
    let strip = f64::from(job.strip());

    pen.fill_rect(strip, 0.0, width - strip, height);

    Some(())
}

fn read(pen: &OffscreenCanvasRenderingContext2d, job: Job) -> Option<Vec<u8>> {
    let read = pen
        .get_image_data(0, 0, job.width as i32, job.height as i32)
        .ok()?;

    Some(as_camera_has(
        &read.data(),
        job.width as usize,
        job.height as usize,
    ))
}

/// The page's message: `{ frame, width, height, fps, upright, lossless, zoom,
/// x, y, quarters, mirrored }`.
fn asked(message: &JsValue) -> Option<(VideoFrame, Job)> {
    let frame: VideoFrame = Reflect::get(message, &"frame".into())
        .ok()?
        .dyn_into()
        .ok()?;

    // A frame without a valid job is still closed.
    let Some(job) = job(message) else {
        frame.close();

        return None;
    };

    Some((frame, job))
}

/// None unless both sides are even and between 2 and 4096.
fn job(message: &JsValue) -> Option<Job> {
    let part = |key: &str| Reflect::get(message, &key.into()).ok();
    let number = |key: &str| part(key)?.as_f64();
    let even = |side: f64| (2.0..=4096.0).contains(&side) && side % 2.0 == 0.0;
    let width = number("width")?;
    let height = number("height")?;

    if !even(width) || !even(height) {
        return None;
    }

    Some(Job {
        width: width as u32,
        height: height as u32,
        fps: number("fps")?.clamp(1.0, 120.0) as u8,
        upright: part("upright")?.is_truthy(),
        lossless: part("lossless")?.is_truthy(),
        framing: Framing {
            zoom: number("zoom")? as f32,
            x: number("x")? as f32,
            y: number("y")? as f32,
            quarters: number("quarters")? as u8 % 4,
            mirrored: part("mirrored")?.is_truthy(),
        },
    })
}

/// Runs in the worker once the page's WebAssembly has started there.
// wasm-bindgen exports only what is `pub`.
#[allow(unreachable_pub)]
#[wasm_bindgen(js_name = captureWorker)]
pub fn capture_worker() {
    console_error_panic_hook::set_once();

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let drawn = OffscreenCanvas::new(2, 2).expect("a picture to draw on");
    // The canvas is read back for every frame, and `willReadFrequently` lets
    // the browser keep it in memory for that.
    let options = object(&[("willReadFrequently", true.into())]);
    let pen: OffscreenCanvasRenderingContext2d = drawn
        .get_context_with_context_options("2d", &options)
        .ok()
        .flatten()
        .expect("a way to draw")
        .unchecked_into();
    let page = scope.clone();
    let packing = RefCell::new(Packing::default());
    let encoding = RefCell::new(None::<Encoding>);
    let sink = Rc::new(RefCell::new(None::<Sink>));
    let framed = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        let message = event.data();
        let part = |key: &str| Reflect::get(&message, &key.into()).ok();

        // A new channel, `{ channel, first }`, replaces the one before. It
        // knows nothing of earlier frames, so its first picture is packed on
        // its own or encoded as a key frame.
        if let Some(channel) = part("channel").and_then(|channel| channel.dyn_into().ok()) {
            let first = part("first")
                .map(|first| Uint8Array::new(&first).to_vec())
                .unwrap_or_default();
            let channel: RtcDataChannel = channel;

            packing.replace(Packing::default());
            encoding.replace(None);
            sink.replace(Some(Sink::new(channel, first, &page)));

            return;
        }

        // `{ close }` releases the camera before the worker is terminated.
        if part("close").is_some_and(|close| close.is_truthy()) {
            encoding.replace(None);
            sink.replace(None);

            return;
        }

        let started = js_sys::Date::now();
        let ready = sink.borrow().as_ref().is_some_and(Sink::ready);

        match asked(&message) {
            // Skipped while the channel is not open or too much is queued on it.
            Some((frame, _)) if !ready => frame.close(),
            Some((frame, job)) => {
                // A screen is far larger than the camera, so it is scaled
                // down smoothly.
                let _ = Reflect::set(&pen, &"imageSmoothingQuality".into(), &"high".into());
                let drew = draw(&drawn, &pen, &frame, job).is_some();

                // The browser holds the frame's source until the frame is
                // closed.
                frame.close();

                // As video, the encoder sends its output itself. Packing
                // starts afresh once lossless frames are sent again.
                if drew && !job.lossless {
                    packing.replace(Packing::default());

                    let mut encoding = encoding.borrow_mut();
                    let current = encoding.as_ref().is_some_and(|encoding| {
                        encoding.size == (job.width, job.height)
                            && encoding.encoder.state() == CodecState::Configured
                    });

                    if !current {
                        *encoding = Encoding::start(job, sink.clone(), &page);
                    }

                    if let Some(encoding) = encoding.as_mut() {
                        encoding.encode(&drawn, job);
                    }
                } else if drew {
                    encoding.replace(None);

                    let frame = read(&pen, job).map(|picture| CameraFrame {
                        width: job.width as u16,
                        height: job.height as u16,
                        data: packing.borrow_mut().pack(picture, job.width, job.height),
                    });
                    let sent = frame
                        .as_ref()
                        .and_then(|frame| sink.borrow().as_ref()?.send(frame));

                    if let Some(bytes) = sent {
                        told(&page, bytes, Some(js_sys::Date::now() - started));
                    }
                }
            }
            None => {}
        }

        let _ = page.post_message(&object(&[("done", true.into())]));
    });

    scope.set_onmessage(Some(framed.as_ref().unchecked_ref()));
    // Lives as long as the worker.
    framed.forget();

    // Messages the page sent before this listener existed are lost, so the
    // page waits for this ready message.
    let _ = scope.post_message(&JsValue::NULL);
}
