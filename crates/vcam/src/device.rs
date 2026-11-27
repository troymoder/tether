mod ioctl;

use std::collections::VecDeque;
use std::ffi::c_int;
use std::fs::File;
use std::io::{self, IoSlice};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use memmap2::MmapMut;
use rustix::event::{PollFd, PollFlags};
use rustix::fs::{MemfdFlags, SealFlags};
use rustix::io::DupFlags;
use rustix::net::{RecvFlags, SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
use rustix::time::{
    ClockId, Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec,
};
use vcam_wire::{Format, ToShim, ToTetherd};

use crate::tetherd::{RETRY, connect, devices, receive, send, tetherd};
use crate::{errno_of, real};

/// Every fd the shim emulates is below this.
const FDS: usize = 1 << 16;

static DEVICES: [AtomicPtr<Device>; FDS] = [const { AtomicPtr::new(null_mut()) }; FDS];

/// The time tetherd has to answer a stream's `Hello`.
const ANSWER: Duration = Duration::from_millis(200);

pub(crate) fn device(fd: c_int) -> Option<Arc<Device>> {
    let at = usize::try_from(fd).ok().filter(|at| *at < FDS)?;
    let device = DEVICES[at].load(Ordering::Acquire);

    if device.is_null() {
        return None;
    }

    // shortcut: an fd used on one thread while another closes it is not
    // guarded against, which only a broken program does. Add a lock if one
    // turns up.
    unsafe {
        Arc::increment_strong_count(device);

        Some(Arc::from_raw(device))
    }
}

pub(crate) fn forget(fd: c_int) {
    if let Some(slot) = usize::try_from(fd).ok().and_then(|at| DEVICES.get(at)) {
        let device = slot.swap(null_mut(), Ordering::AcqRel);

        if !device.is_null() {
            // `Device::open` stored it with `Arc::into_raw`.
            unsafe { Arc::from_raw(device) }.close();
        }
    }
}

fn monotonic_micros() -> u64 {
    let now = rustix::time::clock_gettime(ClockId::Monotonic);

    now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1000
}

fn readable(fd: BorrowedFd, wait: Option<Duration>) -> bool {
    let timeout = wait.map(|wait| Timespec {
        tv_sec: wait.as_secs() as _,
        tv_nsec: wait.subsec_nanos() as _,
    });

    let mut ready = [PollFd::new(&fd, PollFlags::IN)];

    rustix::event::poll(&mut ready, timeout.as_ref()).is_ok_and(|ready| ready > 0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    /// The program's: not queued yet, or already dequeued.
    Program,
    Queued,
    Released,
    Filled {
        size: u64,
        micros: u64,
    },
}

/// A memfd of slots, one frame each, each starting on a page boundary so the
/// program can map them one at a time.
struct Buffers {
    memfd: File,
    /// The whole memfd, mapped so the shim can write black frames.
    map: MmapMut,
    stride: usize,
    slots: Vec<Slot>,
    /// In the order the program queued them.
    queued: VecDeque<u32>,
    /// In the order they were filled.
    filled: VecDeque<u32>,
}

impl Buffers {
    fn new(count: u32, frame: usize) -> io::Result<Self> {
        let page = rustix::param::page_size();
        let stride = frame.div_ceil(page) * page;
        let memfd =
            rustix::fs::memfd_create("vcam", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING)?;
        let memfd = File::from(memfd);

        memfd.set_len((stride * count as usize) as u64)?;
        // The seals stop either side from resizing the memfd under the other's map.
        rustix::fs::fcntl_add_seals(
            &memfd,
            SealFlags::SHRINK | SealFlags::GROW | SealFlags::SEAL,
        )?;

        // Only the shim maps the memfd like this, and the memfd outlives the map.
        let map = unsafe { MmapMut::map_mut(&memfd) }?;

        Ok(Buffers {
            memfd,
            map,
            stride,
            slots: vec![Slot::Program; count as usize],
            queued: VecDeque::new(),
            filled: VecDeque::new(),
        })
    }

    fn count(&self) -> u32 {
        self.slots.len() as u32
    }

    fn slot(&mut self, index: u32, length: usize) -> &mut [u8] {
        let at = self.stride * index as usize;

        &mut self.map[at..at + length]
    }

    fn send(&self, to: BorrowedFd) -> io::Result<()> {
        let message = ToTetherd::Buffers {
            slots: self.count(),
            slot_size: self.stride as u64,
        }
        .encode();

        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut extra = SendAncillaryBuffer::new(&mut space);
        let fds = [self.memfd.as_fd()];

        extra.push(SendAncillaryMessage::ScmRights(&fds));
        rustix::net::sendmsg(
            to,
            &[IoSlice::new(&message)],
            &mut extra,
            SendFlags::NOSIGNAL,
        )?;

        Ok(())
    }
}

struct State {
    /// True while the stream's connection to tetherd is behind the fd.
    connected: bool,
    tried: Option<Instant>,
    /// The connection's last status, logged once until it changes.
    news: String,
    streaming: bool,
    /// True if the program uses read() instead of streaming, through a slot
    /// of the shim's own.
    reading: bool,
    buffers: Option<Buffers>,
    sequence: u32,
    /// Incremented when the stream stops or the buffers change, so frames
    /// tetherd filled earlier are recognised as stale.
    generation: u32,
}

impl State {
    fn wanting(&self) -> bool {
        self.streaming || self.reading
    }

    /// A slot taken back meanwhile is not tetherd's to fill.
    fn fits(&self, frame: &vcam_wire::Frame) -> bool {
        frame.generation == self.generation
            && self.buffers.as_ref().is_some_and(|buffers| {
                buffers.slots.get(frame.slot as usize) == Some(&Slot::Released)
            })
    }
}

pub(crate) struct Device {
    path: String,
    pub(crate) index: u32,
    format: Format,
    black: Vec<u8>,
    /// The fd number the program holds, whatever file is behind it.
    fd: c_int,
    nonblock: bool,
    cloexec: bool,
    state: Mutex<State>,
}

fn timer(fps: u32, on: bool, nonblock: bool, cloexec: bool) -> io::Result<OwnedFd> {
    let mut flags = TimerfdFlags::empty();

    flags.set(TimerfdFlags::NONBLOCK, nonblock);
    flags.set(TimerfdFlags::CLOEXEC, cloexec);

    let fd = rustix::time::timerfd_create(TimerfdClockId::Monotonic, flags)?;

    tick(fd.as_fd(), fps, on);

    Ok(fd)
}

fn tick(fd: BorrowedFd, fps: u32, on: bool) {
    let period = if on {
        1_000_000_000 / i64::from(fps)
    } else {
        0
    };

    let tick = Timespec {
        tv_sec: period / 1_000_000_000,
        tv_nsec: period % 1_000_000_000,
    };

    let every = Itimerspec {
        it_interval: tick,
        it_value: tick,
    };

    let _ = rustix::time::timerfd_settime(fd, TimerfdTimerFlags::empty(), &every);
}

impl Device {
    pub(crate) fn open(index: usize, flags: c_int) -> Result<c_int, c_int> {
        let vcam_wire::Device { path, format } = &devices()[index];
        let nonblock = flags & libc::O_NONBLOCK != 0;
        let cloexec = flags & libc::O_CLOEXEC != 0;
        let fd = timer(format.fps, false, nonblock, cloexec)
            .map_err(|error| errno_of(&error))?
            .into_raw_fd();

        if fd as usize >= FDS {
            unsafe { real::close().call((fd,)) };

            return Err(libc::EMFILE);
        }

        let device = Arc::new(Device {
            path: path.clone(),
            index: index as u32,
            format: *format,
            black: format.black(),
            fd,
            nonblock,
            cloexec,
            state: Mutex::new(State {
                connected: false,
                tried: None,
                news: String::new(),
                streaming: false,
                reading: false,
                buffers: None,
                sequence: 0,
                generation: 0,
            }),
        });

        device.keep_up(&mut device.state.lock().unwrap());
        DEVICES[fd as usize].store(Arc::into_raw(device).cast_mut(), Ordering::Release);

        Ok(fd)
    }

    fn fd(&self) -> BorrowedFd<'_> {
        // The fd stays open while the device is in the table, because the
        // program closes it through the `close` hook, which removes it first.
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }

    fn log(&self, what: impl FnOnce() -> String) {
        tracing::debug!("{} {}", self.path, what());
    }

    fn news(&self, state: &mut State, what: String) {
        if state.news != what {
            tracing::info!("{} {what}", self.path);
            state.news = what;
        }
    }

    fn replace(&self, new: OwnedFd) {
        // The fd number belongs to the program, so it must not be closed
        // when this handle is dropped.
        let mut target = ManuallyDrop::new(unsafe { OwnedFd::from_raw_fd(self.fd) });
        let mut flags = DupFlags::empty();

        flags.set(DupFlags::CLOEXEC, self.cloexec);

        let _ = rustix::io::dup3(&new, &mut target, flags);
    }

    fn tell(&self, state: &mut State, message: &ToTetherd) {
        if state.connected && send(self.fd(), message).is_err() {
            self.disconnect(state, "lost tetherd".to_owned());
        }
    }

    /// The slots tetherd was filling return to the queue.
    fn disconnect(&self, state: &mut State, why: String) {
        if state.connected {
            state.connected = false;

            if let Ok(timer) = timer(
                self.format.fps,
                state.wanting(),
                self.nonblock,
                self.cloexec,
            ) {
                self.replace(timer);
            }

            if let Some(buffers) = &mut state.buffers {
                for slot in &mut buffers.slots {
                    if *slot == Slot::Released {
                        *slot = Slot::Queued;
                    }
                }
            }
        }

        state.tried = Some(Instant::now());
        self.news(state, why);
    }

    fn release(&self, state: &mut State) {
        if !state.connected || !state.wanting() {
            return;
        }

        let Some(buffers) = &mut state.buffers else {
            return;
        };

        let given: Vec<u32> = buffers
            .queued
            .iter()
            .copied()
            .filter(|slot| buffers.slots[*slot as usize] == Slot::Queued)
            .collect();

        for slot in &given {
            buffers.slots[*slot as usize] = Slot::Released;
        }

        let generation = state.generation;

        for slot in given {
            self.tell(state, &ToTetherd::Release { slot, generation });
        }
    }

    /// The next valid frame is left in the socket so each frame wakes the
    /// program's `poll` once. Called at the start of every call on the
    /// device.
    fn keep_up(&self, state: &mut State) {
        if !state.connected {
            if state.tried.is_none_or(|tried| tried.elapsed() >= RETRY) {
                state.tried = Some(Instant::now());
                self.start_stream(state);
            }

            return;
        }

        loop {
            match receive(self.fd(), RecvFlags::PEEK | RecvFlags::DONTWAIT) {
                Ok(Some(ToShim::Frame(frame))) if state.fits(&frame) => return,
                Ok(Some(_)) => match receive(self.fd(), RecvFlags::DONTWAIT) {
                    Ok(Some(message)) => self.heard(state, message),
                    _ => return self.disconnect(state, "lost tetherd".to_owned()),
                },
                Ok(None) => return self.disconnect(state, "lost tetherd".to_owned()),
                Err(error) if matches!(error.raw_os_error(), Some(libc::EAGAIN | libc::EINTR)) => {
                    return;
                }
                Err(_) => return self.disconnect(state, "lost tetherd".to_owned()),
            }
        }
    }

    fn start_stream(&self, state: &mut State) {
        let hello = ToTetherd::Hello {
            version: vcam_wire::VERSION,
            emulator: tetherd().1.clone(),
            pid: std::process::id(),
            device: self.path.clone(),
            format: self.format,
        };

        match connect(&hello, ANSWER) {
            Ok((connection, ToShim::HelloOk { .. })) => {
                if let Some(buffers) = &state.buffers
                    && buffers.send(connection.as_fd()).is_err()
                {
                    return self.news(state, "lost tetherd".to_owned());
                }

                self.replace(connection);
                state.connected = true;
                self.news(state, "connected".to_owned());

                if state.wanting() {
                    self.tell(state, &ToTetherd::StreamOn);
                    self.release(state);
                }
            }
            // The device keeps its format, which the program already has.
            Ok((_, ToShim::Err(why))) => self.news(state, format!("turned down: {why}")),
            Ok(_) => self.news(state, "tetherd did not answer".to_owned()),
            Err(error) => self.news(state, format!("no tetherd: {error}")),
        }
    }

    fn heard(&self, state: &mut State, message: ToShim) {
        match message {
            ToShim::Frame(frame) => {
                if state.fits(&frame)
                    && let Some(buffers) = &mut state.buffers
                {
                    buffers.slots[frame.slot as usize] = Slot::Filled {
                        size: frame.size.min(self.black.len() as u64),
                        micros: frame.micros,
                    };

                    buffers.queued.retain(|slot| *slot != frame.slot);
                    buffers.filled.push_back(frame.slot);
                }
            }
            ToShim::DeviceGone => self.disconnect(state, "tetherd has no source".to_owned()),
            ToShim::Err(why) => self.news(state, format!("tetherd: {why}")),
            _ => {}
        }
    }

    fn want(&self, state: &mut State, streaming: bool, reading: bool) {
        let before = state.wanting();

        state.streaming = streaming;
        state.reading = reading;

        if before == state.wanting() {
            return;
        }

        if state.connected {
            if state.wanting() {
                self.tell(state, &ToTetherd::StreamOn);
                self.release(state);
            } else {
                self.tell(state, &ToTetherd::StreamOff);
            }
        } else {
            tick(self.fd(), self.format.fps, state.wanting());
        }
    }

    /// Blocks as the fd's flags say. `EAGAIN` when it would block and the fd
    /// is non-blocking.
    fn next_filled(&self, state: &mut State) -> Result<u32, c_int> {
        loop {
            if let Some(buffers) = &mut state.buffers
                && let Some(slot) = buffers.filled.pop_front()
            {
                return Ok(slot);
            }

            if state
                .buffers
                .as_ref()
                .is_none_or(|buffers| buffers.queued.is_empty())
            {
                return Err(libc::EAGAIN);
            }

            if state.connected {
                let wait = self.nonblock.then_some(Duration::ZERO);

                if !readable(self.fd(), wait) {
                    return Err(libc::EAGAIN);
                }

                match receive(self.fd(), RecvFlags::empty()) {
                    Ok(Some(message)) => self.heard(state, message),
                    _ => self.disconnect(state, "lost tetherd".to_owned()),
                }

                continue;
            }

            let mut ticks = [0; 8];

            rustix::io::read(self.fd(), &mut ticks).map_err(|error| error.raw_os_error())?;

            let black = &self.black;
            let buffers = state.buffers.as_mut().unwrap();
            let slot = buffers.queued.pop_front().unwrap();

            buffers.slot(slot, black.len()).copy_from_slice(black);
            buffers.slots[slot as usize] = Slot::Filled {
                size: black.len() as u64,
                micros: monotonic_micros(),
            };

            buffers.filled.push_back(slot);
        }
    }

    /// Returns its size, capture time, and sequence number counting from 0.
    fn take(&self, state: &mut State, slot: u32) -> (u64, u64, u32) {
        let buffers = state.buffers.as_mut().unwrap();
        let taken = std::mem::replace(&mut buffers.slots[slot as usize], Slot::Program);
        let sequence = state.sequence;

        state.sequence = sequence.wrapping_add(1);

        match taken {
            Slot::Filled { size, micros } => (size, micros, sequence),
            _ => (0, monotonic_micros(), sequence),
        }
    }

    /// A count of 0 frees them.
    fn set_buffers(&self, state: &mut State, count: u32) -> Result<(), c_int> {
        state.buffers = None;
        state.generation = state.generation.wrapping_add(1);

        if count == 0 {
            let none = ToTetherd::Buffers {
                slots: 0,
                slot_size: 0,
            };

            self.tell(state, &none);

            return Ok(());
        }

        let buffers = Buffers::new(count, self.black.len()).map_err(|error| errno_of(&error))?;

        if state.connected && buffers.send(self.fd()).is_err() {
            self.disconnect(state, "lost tetherd".to_owned());
        }

        state.buffers = Some(buffers);

        Ok(())
    }

    pub(crate) fn read(&self, into: &mut [u8]) -> Result<usize, c_int> {
        let mut state = self.state.lock().unwrap();

        self.keep_up(&mut state);

        if state.streaming {
            return Err(libc::EBUSY);
        }

        if state.buffers.is_none() {
            self.set_buffers(&mut state, 1)?;
        }

        if let Some(buffers) = &mut state.buffers
            && buffers.slots[0] == Slot::Program
        {
            buffers.slots[0] = Slot::Queued;
            buffers.queued.push_back(0);
        }

        self.want(&mut state, false, true);
        self.release(&mut state);

        let slot = self.next_filled(&mut state)?;
        let (size, ..) = self.take(&mut state, slot);
        let given = (size as usize).min(into.len());
        let buffers = state.buffers.as_mut().unwrap();

        into[..given].copy_from_slice(&buffers.slot(slot, given)[..given]);

        Ok(given)
    }

    /// The state lock is held during the call to `map`, so the memfd stays
    /// open.
    pub(crate) fn map_slot<R>(
        &self,
        offset: i64,
        size: usize,
        map: impl FnOnce(c_int) -> R,
    ) -> Option<R> {
        let state = self.state.lock().unwrap();
        let buffers = state.buffers.as_ref()?;
        let offset = usize::try_from(offset).ok()?;
        let fits = offset.is_multiple_of(buffers.stride)
            && size <= buffers.stride
            && offset / buffers.stride < buffers.slots.len();

        fits.then(|| map(buffers.memfd.as_raw_fd()))
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();

        if state.connected {
            let bye = ToTetherd::Bye.encode();
            let _ = rustix::net::send(self.fd(), &bye, SendFlags::NOSIGNAL | SendFlags::DONTWAIT);
        }

        state.buffers = None;
    }
}
