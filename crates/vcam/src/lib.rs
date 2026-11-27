//! The shim: a userspace camera loaded into a program with `LD_PRELOAD`. It
//! makes paths look like V4L2 capture devices, which is all a webcam is to a
//! program on Linux, so no kernel module is needed.
//!
//! A device always delivers frames. Opening it never fails, and the program
//! is never told tetherd is missing. Until tetherd is there the frames are
//! black, paced at the device's rate by a timerfd behind the program's fd.
//! Once tetherd accepts the stream's connection, that connection is put
//! behind the same fd number with `dup3`, and the timerfd returns when
//! tetherd goes. Either way `poll` on the fd wakes once per frame, as with a
//! real camera.
//!
//! The shim starts no threads. At the start of each call on a device it
//! connects if a retry is due and handles tetherd's messages. The shim's own
//! system calls go through rustix, which calls the kernel directly, because
//! calls through the C library would re-enter the shim's hooks.

// The hooks replace C library functions, so their callers already follow
// the C library's safety contract.
#![allow(clippy::missing_safety_doc)]

// V4L2 and LD_PRELOAD exist only on Linux, and Cargo.toml declares the
// dependencies for Linux only.
#[cfg(not(target_os = "linux"))]
compile_error!("vcam builds for Linux only");

use workspace_hack as _;

mod device;
mod real;
mod say;
mod tetherd;

use std::ffi::{CStr, c_char, c_int, c_uint, c_ulong, c_void};
use std::io;
use std::mem::zeroed;

use device::{Device, device};
use tetherd::devices;

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unreachable_pub,
    clippy::all
)]
mod v4l2 {
    include!(concat!(env!("OUT_DIR"), "/v4l2.rs"));
}

/// Only a `/dev/video*` path makes the shim join.
fn ours(path: *const c_char) -> Option<usize> {
    if path.is_null() {
        return None;
    }

    // The C library's contract makes `path` a valid C string.
    let path = unsafe { CStr::from_ptr(path) }.to_bytes();

    if !path.starts_with(b"/dev/video") {
        return None;
    }

    devices()
        .iter()
        .position(|device| device.path.as_bytes() == path)
}

fn fail(error: c_int) -> c_int {
    unsafe { *libc::__errno_location() = error };

    -1
}

fn errno_of(error: &io::Error) -> c_int {
    error.raw_os_error().unwrap_or(libc::EIO)
}

fn opened(path: *const c_char, flags: c_int, pass: impl FnOnce() -> c_int) -> c_int {
    match ours(path) {
        None => pass(),
        Some(index) => Device::open(index, flags).unwrap_or_else(fail),
    }
}

/// `stat` of one of the shim's paths reports a character device, as a
/// camera is.
fn stat_of(path: *const c_char, out: *mut libc::stat64, pass: impl FnOnce() -> c_int) -> c_int {
    let Some(index) = ours(path) else {
        return pass();
    };

    // The caller passes a valid `stat64` to fill.
    unsafe {
        *out = zeroed();
        (*out).st_mode = libc::S_IFCHR | 0o660;
        (*out).st_rdev = libc::makedev(81, index as u32);
        (*out).st_nlink = 1;
    }

    0
}

fn fstat_of(fd: c_int, out: *mut libc::stat64, pass: impl FnOnce() -> c_int) -> c_int {
    let done = pass();

    if let (0, Some(device)) = (done, device(fd)) {
        // The C library has just filled `out`.
        unsafe {
            (*out).st_mode = libc::S_IFCHR | 0o660;
            (*out).st_rdev = libc::makedev(81, device.index);
        }
    }

    done
}

macro_rules! mode {
    ($flags:expr, $rest:expr) => {
        // Matches glibc's __OPEN_NEEDS_MODE. O_TMPFILE includes O_DIRECTORY,
        // so it is compared in full.
        if $flags & libc::O_CREAT != 0 || $flags & libc::O_TMPFILE == libc::O_TMPFILE {
            unsafe { $rest.next_arg::<c_uint>() }
        } else {
            0
        }
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mut rest: ...) -> c_int {
    let mode = mode!(flags, rest);

    opened(path, flags, || unsafe {
        real::open().call((path, flags, mode))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn open64(path: *const c_char, flags: c_int, mut rest: ...) -> c_int {
    let mode = mode!(flags, rest);

    opened(path, flags, || unsafe {
        real::open64().call((path, flags, mode))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn openat(
    dir: c_int,
    path: *const c_char,
    flags: c_int,
    mut rest: ...
) -> c_int {
    let mode = mode!(flags, rest);

    opened(path, flags, || unsafe {
        real::openat().call((dir, path, flags, mode))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn openat64(
    dir: c_int,
    path: *const c_char,
    flags: c_int,
    mut rest: ...
) -> c_int {
    let mode = mode!(flags, rest);

    opened(path, flags, || unsafe {
        real::openat64().call((dir, path, flags, mode))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __open_2(path: *const c_char, flags: c_int) -> c_int {
    opened(path, flags, || unsafe {
        real::__open_2().call((path, flags))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __open64_2(path: *const c_char, flags: c_int) -> c_int {
    opened(path, flags, || unsafe {
        real::__open64_2().call((path, flags))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const c_char, out: *mut libc::stat64) -> c_int {
    stat_of(path, out, || unsafe { real::stat().call((path, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat64(path: *const c_char, out: *mut libc::stat64) -> c_int {
    stat_of(path, out, || unsafe { real::stat64().call((path, out)) })
}

/// The `stat` entry point that programs built against glibc before 2.33
/// call, the emulator among them. Newer glibc keeps it as an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __xstat64(
    _version: c_int,
    path: *const c_char,
    out: *mut libc::stat64,
) -> c_int {
    stat_of(path, out, || unsafe { real::stat64().call((path, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __xstat(
    _version: c_int,
    path: *const c_char,
    out: *mut libc::stat64,
) -> c_int {
    stat_of(path, out, || unsafe { real::stat64().call((path, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat(fd: c_int, out: *mut libc::stat64) -> c_int {
    fstat_of(fd, out, || unsafe { real::fstat().call((fd, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat64(fd: c_int, out: *mut libc::stat64) -> c_int {
    fstat_of(fd, out, || unsafe { real::fstat64().call((fd, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __fxstat64(_version: c_int, fd: c_int, out: *mut libc::stat64) -> c_int {
    fstat_of(fd, out, || unsafe { real::fstat64().call((fd, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __fxstat(_version: c_int, fd: c_int, out: *mut libc::stat64) -> c_int {
    fstat_of(fd, out, || unsafe { real::fstat64().call((fd, out)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const c_char, mode: c_int) -> c_int {
    match ours(path) {
        None => unsafe { real::access().call((path, mode)) },
        Some(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ioctl(fd: c_int, request: c_ulong, arg: *mut c_void) -> c_int {
    let Some(device) = device(fd) else {
        return unsafe { real::ioctl().call((fd, request, arg)) };
    };

    // The kernel only reads the low 32 bits. The emulator passes the
    // request as an `int`, which arrives sign-extended.
    match unsafe { device.ioctl(request as u32, arg) } {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, into: *mut c_void, count: usize) -> isize {
    let Some(device) = device(fd) else {
        return unsafe { real::read().call((fd, into, count)) };
    };

    if into.is_null() {
        return fail(libc::EFAULT) as isize;
    }

    // The caller passes a buffer of `count` bytes.
    let into = unsafe { std::slice::from_raw_parts_mut(into.cast::<u8>(), count) };

    device
        .read(into)
        .map_or_else(|error| fail(error) as isize, |given| given as isize)
}

fn mapped(
    fd: c_int,
    size: usize,
    offset: i64,
    pass: impl FnOnce(c_int) -> *mut c_void,
) -> *mut c_void {
    let Some(device) = device(fd) else {
        return pass(fd);
    };

    device.map_slot(offset, size, pass).unwrap_or_else(|| {
        fail(libc::EINVAL);

        libc::MAP_FAILED
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap(
    at: *mut c_void,
    size: usize,
    protect: c_int,
    flags: c_int,
    fd: c_int,
    offset: i64,
) -> *mut c_void {
    mapped(fd, size, offset, |fd| unsafe {
        real::mmap().call((at, size, protect, flags, fd, offset))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap64(
    at: *mut c_void,
    size: usize,
    protect: c_int,
    flags: c_int,
    fd: c_int,
    offset: i64,
) -> *mut c_void {
    mapped(fd, size, offset, |fd| unsafe {
        real::mmap64().call((at, size, protect, flags, fd, offset))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    device::forget(fd);

    unsafe { real::close().call((fd,)) }
}
