#[macro_use]
extern crate cpp;

use std::ffi::{CStr, CString};
use std::ptr;

cpp!{{
    #include <lldb/API/SBError.h>
    #include <lldb/API/SBDebugger.h>
    #include <lldb/API/SBTarget.h>
    #include <lldb/API/SBLaunchInfo.h>
    #include <lldb/API/SBProcess.h>
    using namespace lldb;
}}

fn with_cstr<R, F: FnOnce(*const i8) -> R>(s: &str, f: F) -> R {
    let cs = CString::new(s).unwrap();
    f(cs.as_ptr())
}

fn with_opt_cstr<R, F: FnOnce(*const i8) -> R>(s: Option<&str>, f: F) -> R {
    let cs = s.map(|s| CString::new(s).unwrap());
    f(cs.map_or(ptr::null(), |cs| cs.as_ptr()))
}

cpp_class!(pub unsafe struct SBDebugger as "SBDebugger");

impl SBDebugger {
    pub fn initialize() {
        unsafe {
            cpp!([] {
            SBDebugger::Initialize();
        })
        }
    }
    pub fn terminate() {
        unsafe {
            cpp!([] {
            SBDebugger::Terminate();
        })
        }
    }
    pub fn create(source_init_files: bool) -> SBDebugger {
        unsafe {
            cpp!([source_init_files as "bool"] -> SBDebugger as "SBDebugger" {
                return SBDebugger::Create(source_init_files);
        })
        }
    }
    pub fn create_target(
        &self, executable: &str, target_triple: Option<&str>, platform_name: Option<&str>, add_dependent_modules: bool,
    ) -> Result<SBTarget, SBError> {
        with_cstr(executable, |executable| {
            with_opt_cstr(target_triple, |target_triple| {
                with_opt_cstr(platform_name, |platform_name| {
                    let mut error = SBError::new();
                    let target = unsafe {
                        cpp!([self as "SBDebugger*", executable as "const char*", target_triple as "const char*",
                                    platform_name as "const char*", add_dependent_modules as "bool", mut error as "SBError"] -> SBTarget as "SBTarget" {
                                    return self->CreateTarget(executable, target_triple, platform_name, add_dependent_modules, error);
                                    })
                    };
                    if error.success() {
                        Ok(target)
                    } else {
                        Err(error)
                    }
                })
            })
        })
    }
}

cpp_class!(pub unsafe struct SBError as "SBError");

impl SBError {
    pub fn new() -> SBError {
        unsafe {
            cpp!([] -> SBError as "SBError" {
            return SBError();
        })
        }
    }
    pub fn success(&self) -> bool {
        unsafe { cpp!([self as "SBError*"] -> bool as "bool" { return self->Success(); }) }
    }
    pub fn error_string(&self) -> &str {
        unsafe {
            let cs_ptr = cpp!([self as "SBError*"] -> *const i8 as "const char*" {
                return self->GetCString();
            });
            match CStr::from_ptr(cs_ptr).to_str() {
                Ok(s) => s,
                _ => panic!("Invalid string?"),
            }
        }
    }
}

cpp_class!(pub unsafe struct SBTarget as "SBTarget");

impl SBTarget {
    pub fn launch(&self, mut launch_info: SBLaunchInfo) -> Result<SBProcess, SBError> {
        let mut error = SBError::new();
        let process = unsafe {
            cpp!([self as "SBTarget*", mut launch_info as "SBLaunchInfo", mut error as "SBError"] -> SBProcess as "SBProcess" {
            return self->Launch(launch_info, error);
        })
        };
        if error.success() {
            Ok(process)
        } else {
            Err(error)
        }
    }
}

cpp_class!(pub unsafe struct SBLaunchInfo as "SBLaunchInfo");

impl SBLaunchInfo {
    pub fn new() -> SBLaunchInfo {
        unsafe {
            cpp!([] -> SBLaunchInfo as "SBLaunchInfo" {
            return SBLaunchInfo(nullptr);
        })
        }
    }
}

cpp_class!(pub unsafe struct SBProcess as "SBProcess");
