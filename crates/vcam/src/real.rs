//! The C library's functions are looked up in libc.so.6 directly, because a
//! default lookup would find the shim's symbols of the same name.

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use std::sync::{Arc, OnceLock};

use dlsym::{ArcSymbol, Library};

fn libc() -> &'static Arc<Library> {
    static LIBC: OnceLock<Arc<Library>> = OnceLock::new();

    LIBC.get_or_init(|| {
        // libc is already loaded, so opening it again only raises its
        // reference count.
        match unsafe { Library::open("libc.so.6") } {
            Ok(libc) => Arc::new(libc),
            // The shim runs in the emulator's process and cannot continue without libc.
            Err(_) => std::process::abort(),
        }
    })
}

macro_rules! real {
    ($($name:ident: $kind:ty;)*) => {$(
        pub(crate) fn $name() -> &'static ArcSymbol<$kind> {
            static SYMBOL: OnceLock<ArcSymbol<$kind>> = OnceLock::new();

            SYMBOL.get_or_init(|| {
                // The signature given to real! matches the C library's declaration.
                match unsafe { libc().get_arc(stringify!($name)) } {
                    Ok(symbol) => symbol,
                    Err(_) => std::process::abort(),
                }
            })
        }
    )*};
}

real! {
    open: unsafe extern "C" fn(*const c_char, c_int, c_uint) -> c_int;
    open64: unsafe extern "C" fn(*const c_char, c_int, c_uint) -> c_int;
    openat: unsafe extern "C" fn(c_int, *const c_char, c_int, c_uint) -> c_int;
    openat64: unsafe extern "C" fn(c_int, *const c_char, c_int, c_uint) -> c_int;
    __open_2: unsafe extern "C" fn(*const c_char, c_int) -> c_int;
    __open64_2: unsafe extern "C" fn(*const c_char, c_int) -> c_int;
    stat: unsafe extern "C" fn(*const c_char, *mut libc::stat64) -> c_int;
    stat64: unsafe extern "C" fn(*const c_char, *mut libc::stat64) -> c_int;
    fstat: unsafe extern "C" fn(c_int, *mut libc::stat64) -> c_int;
    fstat64: unsafe extern "C" fn(c_int, *mut libc::stat64) -> c_int;
    access: unsafe extern "C" fn(*const c_char, c_int) -> c_int;
    ioctl: unsafe extern "C" fn(c_int, c_ulong, *mut c_void) -> c_int;
    mmap: unsafe extern "C" fn(*mut c_void, usize, c_int, c_int, c_int, i64) -> *mut c_void;
    mmap64: unsafe extern "C" fn(*mut c_void, usize, c_int, c_int, c_int, i64) -> *mut c_void;
    read: unsafe extern "C" fn(c_int, *mut c_void, usize) -> isize;
    close: unsafe extern "C" fn(c_int) -> c_int;
}
