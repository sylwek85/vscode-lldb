extern crate env_logger;

use std::ffi::CStr;
use std::mem;
use std::os::raw::{c_char, c_int, c_void};


#[cfg(unix)]
fn main() {
    env_logger::Builder::from_default_env().init();

    #[link(name = "dl")]
    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }

    const RTLD_LAZY: c_int = 0x00001;
    const RTLD_GLOBAL: c_int = 0x00100;

    unsafe {
        let liblldb = dlopen(
            b"/usr/lib/llvm-6.0/lib/liblldb-6.0.so\0".as_ptr() as *const c_char,
            //b"/Library/Developer/CommandLineTools/Library/PrivateFrameworks/LLDB.framework/LLDB\0".as_ptr() as *const c_char,
            RTLD_LAZY | RTLD_GLOBAL,
        );
        if liblldb.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }
        let libcodelldb = dlopen(
            b"/home/chega/NW/vscode-lldb/target/debug/libcodelldb2.so\0".as_ptr() as *const c_char,
            //b"/Users/chega/NW/vscode-lldb/target/debug/libcodelldb2.dylib\0".as_ptr() as *const c_char,
            RTLD_LAZY,
        );
        if libcodelldb.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }
        let entry = dlsym(libcodelldb, b"entry\0".as_ptr() as *const c_char);
        if entry.is_null() {
            panic!("{:?}", CStr::from_ptr(dlerror()));
        }
        let entry: unsafe extern "C" fn() = mem::transmute(entry);
        entry();
    }
}

#[cfg(windows)]
fn main()
{
    env_logger::Builder::from_default_env().init();

    #[link(name="kernel32")]
    extern "system" {
        fn LoadLibraryA(filename: *const c_char) -> *mut c_void;
        fn GetProcAddress(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    unsafe {
        let liblldb = LoadLibraryA("C:\\NW\\ll\\build\\bin\\liblldb.dll\0".as_ptr() as *const c_char);
        if liblldb.is_null() {
            panic!("load lldb");
        }
        let libcodelldb = LoadLibraryA("C:\\NW\\vscode-lldb\\target\\debug\\codelldb2.dll\0".as_ptr() as *const c_char);
        if libcodelldb.is_null() {
            panic!("load codelldb2");
        }
        let entry = GetProcAddress(libcodelldb, b"entry\0".as_ptr() as *const c_char);
        if entry.is_null() {
            panic!("get entry");
        }
        let entry: unsafe extern "C" fn() = mem::transmute(entry);
        entry();
    }
}
