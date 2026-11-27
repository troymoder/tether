//! Frames go into the memory the shim handed over, one frame in each of its
//! slots, at the camera's rate while the stream is on.

use std::collections::VecDeque;
use std::fs::File;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail, ensure};
use memmap2::MmapMut;
use rustix::fs::SealFlags;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_seqpacket::UnixSeqpacket;
use vcam_wire::{Filler, Frame, ToShim, ToTetherd};

use super::{Lens, receive, send};

const SILENT: Duration = Duration::from_secs(2);

struct Slots {
    map: MmapMut,
    stride: usize,
    count: u32,
    /// The slots tetherd may fill, in the order the shim gave them back, with
    /// the generation each was given in.
    free: VecDeque<(u32, u32)>,
}

impl Slots {
    /// The memfd is mapped only once it is sealed, so the shim cannot shrink it
    /// under the map.
    fn map(memfd: OwnedFd, count: u32, stride: u64, frame: usize) -> Result<Self> {
        let seals = rustix::fs::fcntl_get_seals(&memfd)?;

        ensure!(
            seals.contains(SealFlags::SHRINK | SealFlags::GROW),
            "buffers not sealed"
        );

        let memfd = File::from(memfd);
        let stride = usize::try_from(stride)?;

        ensure!(
            stride >= frame,
            "slots of {stride} bytes for frames of {frame}"
        );

        ensure!(
            (stride as u64).checked_mul(u64::from(count)) == Some(memfd.metadata()?.len()),
            "buffers not {count} slots of {stride} bytes"
        );

        // Safe because the seals keep the memfd at the size checked above.
        let map = unsafe { MmapMut::map_mut(&memfd) }?;

        Ok(Slots {
            map,
            stride,
            count,
            free: VecDeque::new(),
        })
    }
}

/// A due frame is the camera's newest, or black once the camera has been silent
/// for `SILENT`. It goes into a slot the shim gave back, and is skipped when
/// there is none so a slow emulator never holds up anything else.
pub(super) async fn stream_frames(lens: &Lens, connection: &UnixSeqpacket) -> Result<()> {
    let black = Arc::new(lens.format.black());
    let size = lens.format.frame_bytes() as usize;
    let mut frames = lens.frames.clone();
    let mut due = interval(Duration::from_secs(1) / lens.format.fps);

    due.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut slots: Option<Slots> = None;
    let mut streaming = false;
    let mut sequence = 0u32;
    let mut fresh = true;
    let mut heard = Instant::now();

    loop {
        tokio::select! {
            message = receive(connection) => match message? {
                None | Some((ToTetherd::Bye, _)) => return Ok(()),
                Some((ToTetherd::Buffers { slots: count, slot_size }, memfd)) => {
                    slots = match (count, memfd) {
                        (0, _) => None,
                        (_, Some(memfd)) => Some(Slots::map(memfd, count, slot_size, size)?),
                        (_, None) => bail!("buffers without their memory"),
                    };
                }
                Some((ToTetherd::StreamOn, _)) => streaming = true,
                Some((ToTetherd::StreamOff, _)) => {
                    streaming = false;

                    if let Some(slots) = &mut slots {
                        slots.free.clear();
                    }
                }
                Some((ToTetherd::Release { slot, generation }, _)) => {
                    if let Some(slots) = &mut slots
                        && slot < slots.count
                    {
                        // A slot released twice may already be back in the
                        // shim's hands.
                        ensure!(
                            !slots.free.iter().any(|(free, _)| *free == slot),
                            "slot {slot} released twice"
                        );
                        slots.free.push_back((slot, generation));
                    }
                }
                Some(_) => {}
            },
            changed = frames.changed() => {
                if changed.is_err() {
                    send(connection, &ToShim::DeviceGone).await?;

                    return Ok(());
                }

                fresh = true;
                heard = Instant::now();
            }
            _ = due.tick(), if streaming => {
                let Some(slots) = &mut slots else {
                    continue;
                };

                let Some((slot, generation)) = slots.free.pop_front() else {
                    continue;
                };

                let (frame, filler) = if heard.elapsed() > SILENT {
                    (black.clone(), Some(Filler::Black))
                } else if fresh {
                    (frames.borrow_and_update().clone(), None)
                } else {
                    (frames.borrow().clone(), Some(Filler::Repeat))
                };
                // A frame of another size does not fit the slot.
                let frame = if frame.len() == size { frame } else { black.clone() };
                let at = slots.stride * slot as usize;

                slots.map[at..at + size].copy_from_slice(&frame);

                fresh = false;
                sequence = sequence.wrapping_add(1);

                let frame = Frame {
                    slot,
                    generation,
                    sequence,
                    micros: monotonic_micros(),
                    size: size as u64,
                    filler,
                };

                send(connection, &ToShim::Frame(frame)).await?;
            }
        }
    }
}

fn monotonic_micros() -> u64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);

    now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1000
}
