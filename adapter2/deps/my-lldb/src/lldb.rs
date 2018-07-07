#![feature(proc_macro)]

#[macro_use]
extern crate cpp;

use std::ffi::{CStr, CString};
use std::mem;
use std::ptr;

cpp!{{
    #include <lldb/API/LLDB.h>
    using namespace lldb;
}}

pub type ThreadID = u64;
pub type BreakpointID = u32;

/////////////////////////////////////////////////////////////////////////////////////////////////////

fn with_cstr<R, F: FnOnce(*const i8) -> R>(s: &str, f: F) -> R {
    let allocated;
    let mut buffer: [u8; 256] = unsafe { mem::uninitialized() };
    let ptr: *const i8 = if s.len() < buffer.len() {
        buffer[0..s.len()].clone_from_slice(s.as_bytes());
        buffer[s.len()] = 0;
        buffer.as_ptr() as *const i8
    } else {
        allocated = Some(CString::new(s).unwrap());
        allocated.as_ref().unwrap().as_ptr()
    };
    f(ptr)
}

fn with_opt_cstr<R, F: FnOnce(*const i8) -> R>(s: Option<&str>, f: F) -> R {
    match s {
        Some(s) => with_cstr(s, f),
        None => f(ptr::null()),
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

struct SBIterator<Item, GetNext>
where
    GetNext: FnMut() -> Option<Item>,
{
    size_hint: Option<usize>,
    get_next: GetNext,
}

impl<Item, GetNext> SBIterator<Item, GetNext>
where
    GetNext: FnMut() -> Option<Item>,
{
    fn new(size_hint: Option<usize>, get_next: GetNext) -> Self {
        Self { size_hint, get_next }
    }
}

impl<Item, GetNext> Iterator for SBIterator<Item, GetNext>
where
    GetNext: FnMut() -> Option<Item>,
{
    type Item = Item;
    fn next(&mut self) -> Option<Self::Item> {
        (self.get_next)()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (0, self.size_hint);
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBDebugger as "SBDebugger");

impl SBDebugger {
    pub fn initialize() {
        cpp!(unsafe [] {
            SBDebugger::Initialize();
        })
    }
    pub fn terminate() {
        cpp!(unsafe [] {
            SBDebugger::Terminate();
        })
    }
    pub fn create(source_init_files: bool) -> SBDebugger {
        cpp!(unsafe [source_init_files as "bool"] -> SBDebugger as "SBDebugger" {
            return SBDebugger::Create(source_init_files);
        })
    }
    pub fn async(&self) -> bool {
        cpp!(unsafe [self as "SBDebugger*"]-> bool as "bool" {
            return self->GetAsync();
        })
    }
    pub fn set_async(&self, async: bool) {
        cpp!(unsafe [self as "SBDebugger*", async as "bool"] {
            self->SetAsync(async);
        })
    }
    pub fn create_target(
        &self, executable: &str, target_triple: Option<&str>, platform_name: Option<&str>, add_dependent_modules: bool,
    ) -> Result<SBTarget, SBError> {
        with_cstr(executable, |executable| {
            with_opt_cstr(target_triple, |target_triple| {
                with_opt_cstr(platform_name, |platform_name| {
                    let mut error = SBError::new();
                    let target = cpp!(unsafe [self as "SBDebugger*", executable as "const char*", target_triple as "const char*",
                                              platform_name as "const char*", add_dependent_modules as "bool", mut error as "SBError"
                                             ] -> SBTarget as "SBTarget" {
                            return self->CreateTarget(executable, target_triple, platform_name, add_dependent_modules, error);
                        });
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

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBError as "SBError");

impl SBError {
    pub fn new() -> SBError {
        cpp!(unsafe [] -> SBError as "SBError" { return SBError(); })
    }
    pub fn success(&self) -> bool {
        cpp!(unsafe [self as "SBError*"] -> bool as "bool" { return self->Success(); })
    }
    pub fn error_string(&self) -> &str {
        let cs_ptr = cpp!(unsafe [self as "SBError*"] -> *const i8 as "const char*" {
                return self->GetCString();
            });
        match unsafe { CStr::from_ptr(cs_ptr) }.to_str() {
            Ok(s) => s,
            _ => panic!("Invalid string?"),
        }
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBTarget as "SBTarget");

impl SBTarget {
    pub fn launch(&self, mut launch_info: SBLaunchInfo) -> Result<SBProcess, SBError> {
        let mut error = SBError::new();

        let process = cpp!(unsafe [self as "SBTarget*", mut launch_info as "SBLaunchInfo", mut error as "SBError"] -> SBProcess as "SBProcess" {
            return self->Launch(launch_info, error);
        });
        if error.success() {
            Ok(process)
        } else {
            Err(error)
        }
    }
    pub fn find_breakpoint_by_id(&self, id: BreakpointID) -> SBBreakpoint {
        cpp!(unsafe [self as "SBTarget*", id as "break_id_t"] -> SBBreakpoint as "SBBreakpoint" {
            return self->FindBreakpointByID(id);
        })
    }
    pub fn breakpoint_create_by_location(&self, file: &str, line: u32) -> SBBreakpoint {
        with_cstr(file, |file| {
            cpp!(unsafe [self as "SBTarget*", file as "const char*", line as "uint32_t"] -> SBBreakpoint as "SBBreakpoint" {
                return self->BreakpointCreateByLocation(file, line);
            })
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBLaunchInfo as "SBLaunchInfo");

impl SBLaunchInfo {
    pub fn new() -> SBLaunchInfo {
        cpp!(unsafe [] -> SBLaunchInfo as "SBLaunchInfo" {
            return SBLaunchInfo(nullptr);
        })
    }
    pub fn set_listener(&self, listener: &SBListener) {
        cpp!(unsafe [self as "SBLaunchInfo*", listener as "SBListener*"] {
            self->SetListener(*listener);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBEvent as "SBEvent");

impl SBEvent {
    pub fn new() -> SBEvent {
        cpp!(unsafe [] -> SBEvent as "SBEvent" {
            return SBEvent();
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBListener as "SBListener");

impl SBListener {
    pub fn new() -> SBListener {
        cpp!(unsafe [] -> SBListener as "SBListener" {
            return SBListener();
        })
    }
    pub fn new_with_name(name: &str) -> SBListener {
        with_cstr(name, |name| {
            cpp!(unsafe [name as "const char*"] -> SBListener as "SBListener" {
                return SBListener(name);
            })
        })
    }
    pub fn wait_for_event(&self, num_seconds: u32, event: &mut SBEvent) -> bool {
        cpp!(unsafe [self as "SBListener*", num_seconds as "uint32_t", event as "SBEvent*"] -> bool as "bool" {
            return self->WaitForEvent(num_seconds, *event);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBProcess as "SBProcess");

impl SBProcess {
    pub fn threads<'a>(&'a self) -> impl Iterator<Item = SBThread> + 'a {
        let num_threads = cpp!(unsafe [self as "SBProcess*"] -> u32 as "uint32_t" {
                return self->GetNumThreads();
            });

        let mut index = 0;
        SBIterator::new(Some(num_threads as usize), move || {
            if index < num_threads {
                index += 1;
                Some(
                    cpp!(unsafe [self as "SBProcess*", index as "uint32_t"] -> SBThread as "SBThread" {
                        return self->GetThreadAtIndex(index);
                    }),
                )
            } else {
                None
            }
        })
    }
    pub fn event_is_process_event(event: &SBEvent) -> bool {
        cpp!(unsafe [event as "SBEvent*"] -> bool as "bool" {
            return SBProcess::EventIsProcessEvent(*event);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBThread as "SBThread");

impl SBThread {
    pub fn index_id(&self) -> u32 {
        cpp!(unsafe [self as "SBThread*"] -> u32 as "uint32_t" {
            return self->GetIndexID();
        })
    }
    pub fn thread_id(&self) -> ThreadID {
        cpp!(unsafe [self as "SBThread*"] -> ThreadID as "tid_t" {
            return self->GetThreadID();
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBBreakpoint as "SBBreakpoint");

impl SBBreakpoint {
    pub fn id(&self) -> u32 {
        cpp!(unsafe [self as "SBBreakpoint*"] -> BreakpointID as "uint32_t" {
            return self->GetID();
        })
    }
    pub fn event_is_breakpoint_event(event: &SBEvent) -> bool {
        cpp!(unsafe [event as "SBEvent*"] -> bool as "bool" {
            return SBBreakpoint::EventIsBreakpointEvent(*event);
        })
    }
}
