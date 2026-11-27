use std::ffi::{c_int, c_void};
use std::mem::zeroed;
use std::time::Duration;

use vcam_wire::{Format, Range};

use super::{Buffers, Device, Slot};
use crate::v4l2;

const MOST_BUFFERS: u32 = 8;

/// Truncated to leave a NUL at the end.
fn name<const N: usize>(text: &str) -> [u8; N] {
    let mut out = [0; N];
    let length = text.len().min(N - 1);

    out[..length].copy_from_slice(&text.as_bytes()[..length]);

    out
}

impl Device {
    fn buffer(&self, buffers: &Buffers, buffer: &mut v4l2::v4l2_buffer, index: u32, flags: u32) {
        *buffer = unsafe { zeroed() };
        buffer.index = index;
        buffer.type_ = v4l2::v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buffer.memory = v4l2::v4l2_memory_V4L2_MEMORY_MMAP;
        buffer.m.offset = (buffers.stride * index as usize) as u32;
        buffer.length = self.format.frame_bytes();
        buffer.field = v4l2::v4l2_field_V4L2_FIELD_NONE;
        buffer.flags = v4l2::V4L2_BUF_FLAG_MAPPED | v4l2::V4L2_BUF_FLAG_TIMESTAMP_MONOTONIC | flags;
    }

    fn pix(&self, pix: &mut v4l2::v4l2_pix_format) {
        *pix = unsafe { zeroed() };
        pix.width = self.format.width;
        pix.height = self.format.height;
        pix.pixelformat = self.format.fourcc.code();
        pix.field = v4l2::v4l2_field_V4L2_FIELD_NONE;
        pix.bytesperline = self.format.bytes_per_line();
        pix.sizeimage = self.format.frame_bytes();
        pix.colorspace = v4l2::v4l2_colorspace_V4L2_COLORSPACE_REC709;
        pix.quantization = match self.format.range {
            Range::Limited => v4l2::v4l2_quantization_V4L2_QUANTIZATION_LIM_RANGE,
            Range::Full => v4l2::v4l2_quantization_V4L2_QUANTIZATION_FULL_RANGE,
        };
    }

    pub(crate) unsafe fn ioctl(&self, request: u32, arg: *mut c_void) -> Result<(), c_int> {
        let capture = v4l2::v4l2_buf_type_V4L2_BUF_TYPE_VIDEO_CAPTURE;
        let mut state = self.state.lock().unwrap();

        self.keep_up(&mut state);

        if arg.is_null() {
            return Err(libc::EFAULT);
        }

        let Format {
            width,
            height,
            fps,
            fourcc,
            ..
        } = self.format;

        // The caller passes a pointer to the struct each request defines.
        unsafe {
            match request {
                v4l2::VIDIOC_QUERYCAP => {
                    let caps = &mut *arg.cast::<v4l2::v4l2_capability>();

                    *caps = zeroed();
                    caps.driver = name("vcam");
                    caps.card = name(&format!("tether camera {}", self.index));
                    caps.bus_info = name(&format!("platform:vcam-{}", self.index));
                    caps.version = (6 << 16) | (1 << 8);
                    caps.device_caps = v4l2::V4L2_CAP_VIDEO_CAPTURE
                        | v4l2::V4L2_CAP_STREAMING
                        | v4l2::V4L2_CAP_READWRITE;
                    caps.capabilities = caps.device_caps | v4l2::V4L2_CAP_DEVICE_CAPS;
                }
                v4l2::VIDIOC_ENUM_FMT => {
                    let format = &mut *arg.cast::<v4l2::v4l2_fmtdesc>();

                    if format.index != 0 || format.type_ != capture {
                        return Err(libc::EINVAL);
                    }

                    format.flags = 0;
                    format.description = name(&String::from_utf8_lossy(fourcc.name()));
                    format.pixelformat = fourcc.code();
                }
                // Every format request gets the one format tetherd configured.
                v4l2::VIDIOC_G_FMT | v4l2::VIDIOC_S_FMT | v4l2::VIDIOC_TRY_FMT => {
                    let format = &mut *arg.cast::<v4l2::v4l2_format>();

                    if format.type_ != capture {
                        return Err(libc::EINVAL);
                    }

                    if request == v4l2::VIDIOC_S_FMT && state.streaming {
                        return Err(libc::EBUSY);
                    }

                    let asked_width = format.fmt.pix.width;
                    let asked_height = format.fmt.pix.height;

                    self.pix(&mut format.fmt.pix);
                    self.log(|| {
                        format!("asked for {asked_width}x{asked_height}, given {width}x{height}")
                    });
                }
                v4l2::VIDIOC_ENUM_FRAMESIZES => {
                    let size = &mut *arg.cast::<v4l2::v4l2_frmsizeenum>();

                    if size.index != 0 || size.pixel_format != fourcc.code() {
                        return Err(libc::EINVAL);
                    }

                    size.type_ = v4l2::v4l2_frmsizetypes_V4L2_FRMSIZE_TYPE_DISCRETE;
                    size.__bindgen_anon_1.discrete.width = width;
                    size.__bindgen_anon_1.discrete.height = height;
                }
                v4l2::VIDIOC_ENUM_FRAMEINTERVALS => {
                    let interval = &mut *arg.cast::<v4l2::v4l2_frmivalenum>();

                    if interval.index != 0 || interval.pixel_format != fourcc.code() {
                        return Err(libc::EINVAL);
                    }

                    interval.type_ = v4l2::v4l2_frmivaltypes_V4L2_FRMIVAL_TYPE_DISCRETE;
                    interval.__bindgen_anon_1.discrete.numerator = 1;
                    interval.__bindgen_anon_1.discrete.denominator = fps;
                }
                v4l2::VIDIOC_G_PARM | v4l2::VIDIOC_S_PARM => {
                    let parm = &mut *arg.cast::<v4l2::v4l2_streamparm>();

                    if parm.type_ != capture {
                        return Err(libc::EINVAL);
                    }

                    if request == v4l2::VIDIOC_S_PARM && state.streaming {
                        return Err(libc::EBUSY);
                    }

                    parm.parm.capture = zeroed();
                    parm.parm.capture.capability = v4l2::V4L2_CAP_TIMEPERFRAME;
                    parm.parm.capture.timeperframe.numerator = 1;
                    parm.parm.capture.timeperframe.denominator = fps;
                }
                v4l2::VIDIOC_ENUMINPUT => {
                    let input = &mut *arg.cast::<v4l2::v4l2_input>();

                    if input.index != 0 {
                        return Err(libc::EINVAL);
                    }

                    *input = zeroed();
                    input.name = name("Camera");
                    input.type_ = v4l2::V4L2_INPUT_TYPE_CAMERA;
                }
                v4l2::VIDIOC_G_INPUT => *arg.cast::<c_int>() = 0,
                v4l2::VIDIOC_S_INPUT => {
                    if *arg.cast::<c_int>() != 0 {
                        return Err(libc::EINVAL);
                    }
                }
                // Reporting no controls keeps programs from adjusting brightness and the like.
                v4l2::VIDIOC_QUERYCTRL
                | v4l2::VIDIOC_QUERY_EXT_CTRL
                | v4l2::VIDIOC_QUERYMENU
                | v4l2::VIDIOC_G_CTRL
                | v4l2::VIDIOC_S_CTRL
                | v4l2::VIDIOC_G_EXT_CTRLS
                | v4l2::VIDIOC_S_EXT_CTRLS
                | v4l2::VIDIOC_TRY_EXT_CTRLS => return Err(libc::EINVAL),
                v4l2::VIDIOC_REQBUFS => {
                    let asked = &mut *arg.cast::<v4l2::v4l2_requestbuffers>();

                    if asked.type_ != capture || asked.memory != v4l2::v4l2_memory_V4L2_MEMORY_MMAP
                    {
                        return Err(libc::EINVAL);
                    }

                    if state.streaming {
                        return Err(libc::EBUSY);
                    }

                    asked.count = asked.count.min(MOST_BUFFERS);
                    asked.capabilities = v4l2::V4L2_BUF_CAP_SUPPORTS_MMAP;
                    // Requesting buffers ends read() I/O. Frames flow again at VIDIOC_STREAMON.
                    self.want(&mut state, false, false);
                    self.set_buffers(&mut state, asked.count)?;

                    let count = asked.count;

                    self.log(|| format!("{count} buffers"));
                }
                v4l2::VIDIOC_QUERYBUF | v4l2::VIDIOC_QBUF => {
                    let buffer = &mut *arg.cast::<v4l2::v4l2_buffer>();
                    let index = buffer.index;

                    let Some(buffers) = &mut state.buffers else {
                        return Err(libc::EINVAL);
                    };

                    if buffer.type_ != capture || index >= buffers.count() {
                        return Err(libc::EINVAL);
                    }

                    let mine = buffers.slots[index as usize] == Slot::Program;

                    if request == v4l2::VIDIOC_QBUF {
                        if !mine || buffer.memory != v4l2::v4l2_memory_V4L2_MEMORY_MMAP {
                            return Err(libc::EINVAL);
                        }

                        buffers.slots[index as usize] = Slot::Queued;
                        buffers.queued.push_back(index);
                    }

                    let flags = match buffers.slots[index as usize] {
                        Slot::Program => 0,
                        Slot::Queued | Slot::Released => v4l2::V4L2_BUF_FLAG_QUEUED,
                        Slot::Filled { .. } => v4l2::V4L2_BUF_FLAG_DONE,
                    };

                    let buffers = state.buffers.as_ref().unwrap();

                    self.buffer(buffers, buffer, index, flags);

                    if request == v4l2::VIDIOC_QBUF {
                        self.release(&mut state);
                    }
                }
                v4l2::VIDIOC_DQBUF => {
                    let buffer = &mut *arg.cast::<v4l2::v4l2_buffer>();

                    if buffer.type_ != capture {
                        return Err(libc::EINVAL);
                    }

                    // A blocking dequeue with nothing queued waits for another
                    // thread to queue a buffer, as vb2 does.
                    while !self.nonblock
                        && state.streaming
                        && state
                            .buffers
                            .as_ref()
                            .is_some_and(|buffers| buffers.queued.is_empty())
                    {
                        drop(state);
                        std::thread::sleep(Duration::from_millis(10));
                        state = self.state.lock().unwrap();
                    }

                    if !state.streaming {
                        return Err(libc::EINVAL);
                    }

                    let slot = self.next_filled(&mut state)?;
                    let (size, micros, sequence) = self.take(&mut state, slot);
                    let buffers = state.buffers.as_ref().unwrap();

                    self.buffer(buffers, buffer, slot, v4l2::V4L2_BUF_FLAG_DONE);
                    buffer.bytesused = size as u32;
                    buffer.sequence = sequence;
                    buffer.timestamp.tv_sec = (micros / 1_000_000) as _;
                    buffer.timestamp.tv_usec = (micros % 1_000_000) as _;
                }
                v4l2::VIDIOC_STREAMON | v4l2::VIDIOC_STREAMOFF => {
                    if *arg.cast::<c_int>() != capture as c_int {
                        return Err(libc::EINVAL);
                    }

                    let on = request == v4l2::VIDIOC_STREAMON;

                    self.log(|| if on { "on" } else { "off" }.to_owned());
                    self.want(&mut state, on, false);

                    // Stopping hands every buffer back to the program, and the
                    // new generation discards frames tetherd fills meanwhile.
                    if !on {
                        state.sequence = 0;
                        state.generation = state.generation.wrapping_add(1);

                        if let Some(buffers) = &mut state.buffers {
                            buffers.slots.fill(Slot::Program);
                            buffers.queued.clear();
                            buffers.filled.clear();
                        }
                    }
                }
                _ => {
                    self.log(|| format!("unanswered request {request:#x}"));

                    return Err(libc::ENOTTY);
                }
            }
        }

        Ok(())
    }
}
