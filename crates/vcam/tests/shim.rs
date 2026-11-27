//! The shim is loaded into a child process the way the emulator loads it,
//! with a fake tetherd. Each request the device answers must return 0,
//! because the emulator treats anything else as a failure and drops the
//! camera.

#![cfg(target_os = "linux")]

use std::ffi::c_int;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use vcam_wire::{Device, Format, FourCc, Range, ToShim, ToTetherd};

const CHILD: &str = "VCAM_TEST_CHILD";

const FORMAT: Format = Format {
    width: 640,
    height: 480,
    fps: 30,
    fourcc: FourCc::Yu12,
    range: Range::Limited,
};

// V4L2 ioctl numbers and argument sizes from linux/videodev2.h on a 64-bit
// system. The shim's own bindings are private to it.
const VIDIOC_QUERYCAP: u32 = 0x8068_5600;
const VIDIOC_ENUM_FMT: u32 = 0xc040_5602;
const VIDIOC_G_FMT: u32 = 0xc0d0_5604;
const VIDIOC_S_FMT: u32 = 0xc0d0_5605;
const VIDIOC_REQBUFS: u32 = 0xc014_5608;
const VIDIOC_G_PARM: u32 = 0xc0cc_5615;
const VIDIOC_ENUMINPUT: u32 = 0xc050_561a;
const VIDIOC_G_INPUT: u32 = 0x8004_5626;
const VIDIOC_TRY_FMT: u32 = 0xc0d0_5640;
const VIDIOC_ENUM_FRAMESIZES: u32 = 0xc02c_564a;

const V4L2_CAP_VIDEO_CAPTURE: u32 = 1;
const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;
const V4L2_FRMSIZE_TYPE_DISCRETE: u32 = 1;

#[test]
fn the_shim_answers_as_a_webcam() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }

    let shim = shim();
    let socket = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vcam-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener = listen(&socket);

    thread::spawn(move || play_tetherd(listener));

    let ran = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "in_the_shim", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .env("LD_PRELOAD", &shim)
        .env("VSHIM_SOCKET", &socket)
        .env("VSHIM_EMULATOR", "test")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&socket);

    assert!(
        ran.status.success(),
        "the program with the shim failed:\n{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr),
    );
}

/// Runs in the child process the shim is loaded into, and passes without
/// doing anything elsewhere.
#[test]
fn in_the_shim() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }

    let fd = unsafe { libc::open(c"/dev/video0".as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };

    assert!(fd >= 0, "open failed: {}", io::Error::last_os_error());

    let mut capability = Arg([0; 104]);

    asked(fd, VIDIOC_QUERYCAP, &mut capability);
    assert_eq!(
        word(&capability, 84) & V4L2_CAP_VIDEO_CAPTURE,
        V4L2_CAP_VIDEO_CAPTURE
    );

    let mut description = Arg([0; 64]);

    put(&mut description, 4, V4L2_BUF_TYPE_VIDEO_CAPTURE);
    asked(fd, VIDIOC_ENUM_FMT, &mut description);
    assert_eq!(word(&description, 44), FourCc::Yu12.code());

    put(&mut description, 0, 1);
    assert_eq!(refused(fd, VIDIOC_ENUM_FMT, &mut description), libc::EINVAL);

    let mut format = Arg([0; 208]);

    put(&mut format, 0, V4L2_BUF_TYPE_VIDEO_CAPTURE);
    asked(fd, VIDIOC_G_FMT, &mut format);
    assert_eq!(
        [word(&format, 8), word(&format, 12), word(&format, 16)],
        [FORMAT.width, FORMAT.height, FourCc::Yu12.code()],
    );
    asked(fd, VIDIOC_TRY_FMT, &mut format);
    asked(fd, VIDIOC_S_FMT, &mut format);

    let mut size = Arg([0; 44]);

    put(&mut size, 4, FourCc::Yu12.code());
    asked(fd, VIDIOC_ENUM_FRAMESIZES, &mut size);
    assert_eq!(
        [word(&size, 8), word(&size, 12), word(&size, 16)],
        [V4L2_FRMSIZE_TYPE_DISCRETE, FORMAT.width, FORMAT.height],
    );

    let mut parameters = Arg([0; 204]);

    put(&mut parameters, 0, V4L2_BUF_TYPE_VIDEO_CAPTURE);
    asked(fd, VIDIOC_G_PARM, &mut parameters);

    let mut input = Arg([0; 80]);
    let mut chosen = Arg([0; 4]);

    asked(fd, VIDIOC_ENUMINPUT, &mut input);
    asked(fd, VIDIOC_G_INPUT, &mut chosen);

    let mut buffers = Arg([0; 20]);

    put(&mut buffers, 0, 4);
    put(&mut buffers, 4, V4L2_BUF_TYPE_VIDEO_CAPTURE);
    put(&mut buffers, 8, V4L2_MEMORY_MMAP);
    asked(fd, VIDIOC_REQBUFS, &mut buffers);
    assert!(word(&buffers, 0) >= 1, "no buffers were made");

    assert_eq!(unsafe { libc::close(fd) }, 0);
}

/// An ioctl argument as raw bytes. Aligned to 8 bytes like V4L2's structs
/// with 64-bit fields.
#[repr(C, align(8))]
struct Arg<const N: usize>([u8; N]);

fn asked<const N: usize>(fd: c_int, request: u32, argument: &mut Arg<N>) {
    let answer = unsafe { libc::ioctl(fd, request as libc::Ioctl, argument.0.as_mut_ptr()) };

    assert_eq!(
        answer,
        0,
        "ioctl {request:#x} gave {answer}: {}",
        io::Error::last_os_error()
    );
}

fn refused<const N: usize>(fd: c_int, request: u32, argument: &mut Arg<N>) -> c_int {
    let answer = unsafe { libc::ioctl(fd, request as libc::Ioctl, argument.0.as_mut_ptr()) };

    assert_eq!(answer, -1, "ioctl {request:#x} was not refused");

    io::Error::last_os_error().raw_os_error().unwrap()
}

fn word<const N: usize>(bytes: &Arg<N>, at: usize) -> u32 {
    u32::from_ne_bytes(bytes.0[at..at + 4].try_into().unwrap())
}

fn put<const N: usize>(bytes: &mut Arg<N>, at: usize, value: u32) {
    bytes.0[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}

/// The shim cargo built for this test, the newest if there are several.
/// Cargo's build-dir layout keeps each artifact, the test included, in
/// `build/vcam/<hash>/out`. The classic layout keeps both in `deps`.
fn shim() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let folder = exe.parent().unwrap();
    // `build/vcam`, above the test's own `<hash>/out`.
    let crate_folder = folder.parent().and_then(Path::parent).unwrap();
    let mut found: Vec<PathBuf> = std::fs::read_dir(crate_folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join("out/libvcam.so"))
        .chain([folder.join("libvcam.so")])
        .filter(|path| path.exists())
        .collect();

    found.sort_by_key(|path| path.metadata().and_then(|meta| meta.modified()).ok());

    found
        .pop()
        .expect("libvcam.so is not built beside the test")
}

fn listen(path: &Path) -> OwnedFd {
    let listener = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .unwrap();

    rustix::net::bind(&listener, &SocketAddrUnix::new(path).unwrap()).unwrap();
    rustix::net::listen(&listener, 8).unwrap();

    listener
}

fn play_tetherd(listener: OwnedFd) {
    loop {
        let Ok(connection) = rustix::net::accept(&listener) else {
            return;
        };

        thread::spawn(move || {
            let mut packet = vec![0; vcam_wire::MOST];

            loop {
                let Ok(got) = rustix::net::recv(&connection, &mut packet, RecvFlags::empty())
                else {
                    return;
                };

                if got.0 == 0 {
                    return;
                }

                let answer = match ToTetherd::decode(&packet[..got.0]) {
                    Some(ToTetherd::Join { .. }) => ToShim::Config {
                        devices: vec![Device {
                            path: "/dev/video0".to_owned(),
                            format: FORMAT,
                        }],
                    },
                    Some(ToTetherd::Hello { .. }) => ToShim::HelloOk { session: 1 },
                    _ => continue,
                };

                let _ =
                    rustix::net::send(connection.as_fd(), &answer.encode(), SendFlags::NOSIGNAL);
            }
        });
    }
}
