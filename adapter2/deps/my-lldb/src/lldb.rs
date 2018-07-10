#![allow(non_upper_case_globals)]

#[macro_use]
extern crate cpp;

use std::ffi::{CStr, CString};
use std::fmt;
use std::mem;
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::str;

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

unsafe impl Send for SBDebugger {}

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

unsafe impl Send for SBError {}

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

unsafe impl Send for SBTarget {}

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

unsafe impl Send for SBLaunchInfo {}

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

unsafe impl Send for SBEvent {}

impl SBEvent {
    pub fn new() -> SBEvent {
        cpp!(unsafe [] -> SBEvent as "SBEvent" {
            return SBEvent();
        })
    }
    pub fn get_cstring_from_event(event: &SBEvent) -> Option<&CStr> {
        unsafe {
            let ptr = cpp!([event as "SBEvent*"] -> *const c_char as "const char*" {
                return SBEvent::GetCStringFromEvent(*event);
            });
            if ptr.is_null() {
                None
            } else {
                Some(CStr::from_ptr(ptr))
            }
        }
    }
    pub fn get_description(&self, description: &mut SBStream) -> bool {
        cpp!(unsafe [self as "SBEvent*", description as "SBStream*"] -> bool as "bool" {
            return self->GetDescription(*description);
        })
    }
    pub fn event_type(&self) -> u32 {
        cpp!(unsafe [self as "SBEvent*"] -> u32 as "uint32_t" {
            return self->GetType();
        })
    }
    pub fn as_process_event(&self) -> Option<SBProcessEvent> {
        if cpp!(unsafe [self as "SBEvent*"] -> bool as "bool" {
            return SBProcess::EventIsProcessEvent(*self);
        }) {
            Some(SBProcessEvent(self))
        } else {
            None
        }
    }
    // pub fn as_breakpoint_event(&self) -> Option<SBBreakpointEvent> {}
    // pub fn as_target_event(&self) -> Option<SBTargetEvent> {}
    // pub fn as_thread_event(&self) -> Option<SBThreadEvent> {}
}

impl fmt::Display for SBEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut descr = SBStream::new();
        if self.get_description(&mut descr) {
            match str::from_utf8(descr.data()) {
                Ok(s) => f.write_str(s),
                Err(_) => Err(fmt::Error),
            }
        } else {
            Ok(())
        }
    }
}

pub struct SBProcessEvent<'a>(&'a SBEvent);

impl<'a> SBProcessEvent<'a> {
    pub fn as_event(&self) -> &SBEvent {
        self.0
    }
    pub fn process(&self) -> SBProcess {
        let event = self.0;
        cpp!(unsafe [event as "SBEvent*"] -> SBProcess as "SBProcess" {
            return SBProcess::GetProcessFromEvent(*event);
        })
    }
    pub fn process_state(&self) -> ProcessState {
        let event = self.0;
        cpp!(unsafe [event as "SBEvent*"] -> ProcessState as "uint32_t" {
            return SBProcess::GetStateFromEvent(*event);
        })
    }
    pub fn restarted(&self) -> bool {
        let event = self.0;
        cpp!(unsafe [event as "SBEvent*"] -> bool as "bool" {
            return SBProcess::GetRestartedFromEvent(*event);
        })
    }
    pub fn interrupted(&self) -> bool {
        let event = self.0;
        cpp!(unsafe [event as "SBEvent*"] -> bool as "bool" {
            return SBProcess::GetInterruptedFromEvent(*event);
        })
    }
}

#[repr(u32)]
pub enum ProcessState {
    Invalid = 0,
    Unloaded = 1,
    Connected = 2,
    Attaching = 3,
    Launching = 4,
    Stopped = 5,
    Running = 6,
    Stepping = 7,
    Crashed = 8,
    Detached = 9,
    Exited = 10,
    Suspended = 11,
}

// struct SBBreakpointEvent(&SBEvent);

// struct SBTargetEvent(&SBEvent);

// struct SBThreadEvent(&SBEvent);

// Possible values for SBEvent::event_type()

// pub const SBCommandInterpreter_eBroadcastBitAsynchronousErrorData: u32 = 16;
// pub const SBCommandInterpreter_eBroadcastBitAsynchronousOutputData: u32 = 8;
// pub const SBCommandInterpreter_eBroadcastBitQuitCommandReceived: u32 = 4;
// pub const SBCommandInterpreter_eBroadcastBitResetPrompt: u32 = 2;
// pub const SBCommandInterpreter_eBroadcastBitThreadShouldExit: u32 = 1;

// pub const SBCommunication_eAllEventBits: u32 = !0;
// pub const SBCommunication_eBroadcastBitDisconnected: u32 = 1;
// pub const SBCommunication_eBroadcastBitPacketAvailable: u32 = 16;
// pub const SBCommunication_eBroadcastBitReadThreadDidExit: u32 = 4;
// pub const SBCommunication_eBroadcastBitReadThreadGotBytes: u32 = 2;
// pub const SBCommunication_eBroadcastBitReadThreadShouldExit: u32 = 8;

// pub const SBProcess_eBroadcastBitInterrupt: u32 = 2;
// pub const SBProcess_eBroadcastBitProfileData: u32 = 16;
// pub const SBProcess_eBroadcastBitSTDERR: u32 = 8;
// pub const SBProcess_eBroadcastBitSTDOUT: u32 = 4;
// pub const SBProcess_eBroadcastBitStateChanged: u32 = 1;
// pub const SBProcess_eBroadcastBitStructuredData: u32 = 32;

// pub const SBTarget_eBroadcastBitBreakpointChanged: u32 = 1;
// pub const SBTarget_eBroadcastBitModulesLoaded: u32 = 2;
// pub const SBTarget_eBroadcastBitModulesUnloaded: u32 = 4;
// pub const SBTarget_eBroadcastBitSymbolsLoaded: u32 = 16;
// pub const SBTarget_eBroadcastBitWatchpointChanged: u32 = 8;

// pub const SBThread_eBroadcastBitSelectedFrameChanged: u32 = 8;
// pub const SBThread_eBroadcastBitStackChanged: u32 = 1;
// pub const SBThread_eBroadcastBitThreadResumed: u32 = 4;
// pub const SBThread_eBroadcastBitThreadSelected: u32 = 16;
// pub const SBThread_eBroadcastBitThreadSuspended: u32 = 2;

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBStream as "SBStream");

unsafe impl Send for SBStream {}

impl SBStream {
    pub fn new() -> SBStream {
        cpp!(unsafe [] -> SBStream as "SBStream" {
            return SBStream();
        })
    }
    pub fn data(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
    pub fn len(&self) -> usize {
        cpp!(unsafe [self as "SBStream*"] -> usize as "size_t" {
            return self->GetSize();
        })
    }
    pub fn as_ptr(&self) -> *const u8 {
        cpp!(unsafe [self as "SBStream*"] -> *const c_char as "const char*" {
            return self->GetData();
        }) as *const u8
    }
    pub fn clear(&mut self) {
        cpp!(unsafe [self as "SBStream*"]  {
            self->Clear();
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBListener as "SBListener");

unsafe impl Send for SBListener {}

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

unsafe impl Send for SBProcess {}

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
    pub fn exit_status(&self) -> Option<i32> {

    }

    pub const eBroadcastBitStateChanged: u32 = 1;
    pub const eBroadcastBitInterrupt: u32 = 2;
    pub const eBroadcastBitSTDOUT: u32 = 4;
    pub const eBroadcastBitSTDERR: u32 = 8;
    pub const eBroadcastBitProfileData: u32 = 16;
    pub const eBroadcastBitStructuredData: u32 = 32;
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBThread as "SBThread");

unsafe impl Send for SBThread {}

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

unsafe impl Send for SBBreakpoint {}

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
