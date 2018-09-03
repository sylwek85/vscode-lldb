extern crate env_logger;

use std::env;
use std::mem;

#[cfg(unix)]
fn main() -> Result<(), std::io::Error> {
    use std::os::raw::{c_char, c_int, c_void};
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::*;

    #[link(name = "dl")]
    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }
    const RTLD_LAZY: c_int = 0x00001;
    const RTLD_GLOBAL: c_int = 0x00100;

    env_logger::Builder::from_default_env().init();

    unsafe {
        //std::thread::sleep_ms(10000);

        let liblldb_path: &[u8] = if cfg!(target_os = "macos") {
            b"LLDB.framework/LLDB\0"
        } else {
            b"liblldb-6.0.so\0"
        };
        let liblldb = dlopen(liblldb_path.as_ptr() as *const c_char, RTLD_LAZY | RTLD_GLOBAL);
        if liblldb.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }

        let mut codelldb_path = env::current_exe()?;
        if cfg!(target_os = "macos") {
            codelldb_path.set_file_name("libcodelldb.dylib");
        } else {
            codelldb_path.set_file_name("libcodelldb.so");
        }
        let codelldb_path = CString::new(codelldb_path.as_os_str().as_bytes())?;
        let libcodelldb = dlopen(codelldb_path.as_ptr() as *const c_char, RTLD_LAZY);
        if libcodelldb.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }

        let entry = dlsym(libcodelldb, b"entry\0".as_ptr() as *const c_char);
        if entry.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }
        let entry: unsafe extern "C" fn(&[&str]) = mem::transmute(entry);

        let args = env::args().collect::<Vec<_>>();
        let arg_refs = args.iter().map(|a| a.as_ref()).collect::<Vec<_>>();
        entry(&arg_refs);
    }
    Ok(())
}

#[cfg(windows)]
fn main() -> Result<(), std::io::Error> {
    use std::os::raw::{c_char, c_void};
    use std::ffi::{CString};

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryA(filename: *const c_char) -> *mut c_void;
        fn GetProcAddress(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    env_logger::Builder::from_default_env().init();

    unsafe {
        let liblldb = LoadLibraryA(b"liblldb.dll\0".as_ptr() as *const c_char);
        if liblldb.is_null() {
            panic!("Could not load liblldb.dll");
        }

        let mut codelldb_path = env::current_exe()?;
        codelldb_path.set_file_name("codelldb.dll");
        let codelldb_path = CString::new(codelldb_path.as_os_str().to_str().unwrap().as_bytes())?;

        let libcodelldb = LoadLibraryA(codelldb_path.as_ptr() as *const c_char);
        if libcodelldb.is_null() {
            panic!("Could not load codelldb.dll");
        }
        let entry = GetProcAddress(libcodelldb, b"entry\0".as_ptr() as *const c_char);
        if entry.is_null() {
            panic!("Could not get the entry point.");
        }
        let entry: unsafe extern "C" fn(&[&str]) = mem::transmute(entry);

        let args = env::args().collect::<Vec<_>>();
        let arg_refs = args.iter().map(|a| a.as_ref()).collect::<Vec<_>>();
        entry(&arg_refs);
    }
    Ok(())
}
