#![allow(non_upper_case_globals)]

#[macro_use]
extern crate cpp;

use std::ffi::{CStr, CString};
use std::fmt;
use std::mem;
use std::os::raw::c_char;
//use std::path::{Path, PathBuf};
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

fn with_cstr<R, F>(s: &str, f: F) -> R
where
    F: FnOnce(*const i8) -> R,
{
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

fn with_opt_cstr<R, F>(s: Option<&str>, f: F) -> R
where
    F: FnOnce(*const i8) -> R,
{
    match s {
        Some(s) => with_cstr(s, f),
        None => f(ptr::null()),
    }
}

fn get_string<F>(initial_capacity: usize, f: F) -> String
where
    F: Fn(*mut c_char, usize) -> usize,
{
    let mut buffer = Vec::with_capacity(initial_capacity);
    let mut size = f(buffer.as_ptr() as *mut c_char, buffer.capacity());
    if (size as isize) < 0 {
        panic!();
    }
    if size >= buffer.capacity() {
        let additional = size - buffer.capacity() + 1;
        buffer.reserve(additional);
        size = f(buffer.as_ptr() as *mut c_char, buffer.capacity());
        if (size as isize) < 0 {
            panic!();
        }
    }
    unsafe { buffer.set_len(size) };
    String::from_utf8(buffer).unwrap()
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

struct SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    size: u32,
    get_item: GetItem,
    index: u32,
}

impl<Item, GetItem> SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    fn new(size: u32, get_item: GetItem) -> Self {
        Self {
            size: size,
            get_item: get_item,
            index: 0,
        }
    }
}

impl<Item, GetItem> Iterator for SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    type Item = Item;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.size {
            self.index += 1;
            Some((self.get_item)(self.index - 1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (0, Some(self.size as usize));
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBDebugger*"] -> bool as "bool" {
            return self->IsValid();
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBError*"] -> bool as "bool" {
            return self->IsValid();
        })
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBTarget*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBEvent*"] -> bool as "bool" {
            return self->IsValid();
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
    pub fn flags(&self) -> u32 {
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
    pub fn as_breakpoint_event(&self) -> Option<SBBreakpointEvent> {
        if cpp!(unsafe [self as "SBEvent*"] -> bool as "bool" {
            return SBBreakpoint::EventIsBreakpointEvent(*self);
        }) {
            Some(SBBreakpointEvent(self))
        } else {
            None
        }
    }
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

pub struct SBBreakpointEvent<'a>(&'a SBEvent);

pub struct SBTargetEvent<'a>(&'a SBEvent);

pub struct SBThreadEvent<'a>(&'a SBEvent);

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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBStream*"] -> bool as "bool" {
            return self->IsValid();
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBListener*"] -> bool as "bool" {
            return self->IsValid();
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBProcess*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn num_threads(&self) -> u32 {
        cpp!(unsafe [self as "SBProcess*"] -> u32 as "uint32_t" {
                return self->GetNumThreads();
        })
    }
    pub fn thread_at_index(&self, index: u32) -> SBThread {
        cpp!(unsafe [self as "SBProcess*", index as "uint32_t"] -> SBThread as "SBThread" {
            return self->GetThreadAtIndex(index);
        })
    }
    pub fn threads<'a>(&'a self) -> impl Iterator<Item = SBThread> + 'a {
        SBIterator::new(self.num_threads(), move |index| self.thread_at_index(index))
    }
    pub fn state(&self) -> ProcessState {
        cpp!(unsafe [self as "SBProcess*"] -> ProcessState as "uint32_t" {
            return self->GetState();
        })
    }
    pub fn exit_status(&self) -> i32 {
        cpp!(unsafe [self as "SBProcess*"] -> i32 as "int32_t" {
            return self->GetExitStatus();
        })
    }
    pub fn selected_thread(&self) -> SBThread {
        cpp!(unsafe [self as "SBProcess*"] -> SBThread as "SBThread" {
            return self->GetSelectedThread();
        })
    }
    pub fn set_selected_thread(&self, thread: &SBThread) -> bool {
        cpp!(unsafe [self as "SBProcess*", thread as "SBThread*"] -> bool as "bool" {
            return self->SetSelectedThread(*thread);
        })
    }
    pub fn thread_by_id(&self, tid: ThreadID) -> Option<SBThread> {
        let thread = cpp!(unsafe [self as "SBProcess*", tid as "tid_t"] -> SBThread as "SBThread" {
            return self->GetThreadByID(tid);
        });
        if thread.is_valid() {
            Some(thread)
        } else {
            None
        }
    }
    pub fn thread_by_index_id(&self, index_id: u32) -> Option<SBThread> {
        let thread = cpp!(unsafe [self as "SBProcess*", index_id as "uint32_t"] -> SBThread as "SBThread" {
            return self->GetThreadByIndexID(index_id);
        });
        if thread.is_valid() {
            Some(thread)
        } else {
            None
        }
    }
    pub fn kill(&self) -> SBError {
        cpp!(unsafe [self as "SBProcess*"] -> SBError as "SBError" {
            return self->Kill();
        })
    }
    pub fn detach(&self) -> SBError {
        cpp!(unsafe [self as "SBProcess*"] -> SBError as "SBError" {
            return self->Detach();
        })
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
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBThread*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn thread_id(&self) -> ThreadID {
        cpp!(unsafe [self as "SBThread*"] -> ThreadID as "tid_t" {
            return self->GetThreadID();
        })
    }
    pub fn index_id(&self) -> u32 {
        cpp!(unsafe [self as "SBThread*"] -> u32 as "uint32_t" {
            return self->GetIndexID();
        })
    }
    pub fn stop_reason(&self) -> StopReason {
        cpp!(unsafe [self as "SBThread*"] -> StopReason as "uint32_t" {
            return self->GetStopReason();
        })
    }
    pub fn stop_description(&self) -> String {
        get_string(64, |ptr, size| {
            cpp!(unsafe [self as "SBThread*", ptr as "char*", size as "size_t"] -> usize as "size_t" {
                return self->GetStopDescription(ptr, size);
            })
        })
    }
    pub fn num_frames(&self) -> u32 {
        cpp!(unsafe [self as "SBThread*"] -> u32 as "uint32_t" {
            return self->GetNumFrames();
        })
    }
    pub fn frame_at_index(&self, index: u32) -> SBFrame {
        cpp!(unsafe [self as "SBThread*", index as "uint32_t"] -> SBFrame as "SBFrame" {
            return self->GetFrameAtIndex(index);
        })
    }
    pub fn frames<'a>(&'a self) -> impl Iterator<Item = SBFrame> + 'a {
        SBIterator::new(self.num_frames(), move |index| self.frame_at_index(index))
    }
}

#[repr(u32)]
pub enum StopReason {
    Invalid = 0,
    None = 1,
    Trace = 2,
    Breakpoint = 3,
    Watchpoint = 4,
    Signal = 5,
    Exception = 6,
    Exec = 7, // Program was re-exec'ed
    PlanComplete = 8,
    ThreadExiting = 9,
    Instrumentation = 10,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBFrame as "SBFrame");

unsafe impl Send for SBFrame {}

impl SBFrame {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBFrame*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn function_name(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBFrame*"] -> *const c_char as "const char*" {
            return self->GetFunctionName();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn display_function_name(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBFrame*"] -> *const c_char as "const char*" {
            return self->GetDisplayFunctionName();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn line_entry(&self) -> Option<SBLineEntry> {
        let line_entry = cpp!(unsafe [self as "SBFrame*"] -> SBLineEntry as "SBLineEntry" {
            return self->GetLineEntry();
        });
        if line_entry.is_valid() {
            Some(line_entry)
        } else {
            None
        }
    }
    pub fn pc_address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBFrame*"] -> SBAddress as "SBAddress" {
            return self->GetPCAddress();
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBBreakpoint as "SBBreakpoint");

unsafe impl Send for SBBreakpoint {}

impl SBBreakpoint {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBBreakpoint*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn id(&self) -> u32 {
        cpp!(unsafe [self as "SBBreakpoint*"] -> BreakpointID as "uint32_t" {
            return self->GetID();
        })
    }
    pub fn num_locations(&self) -> u32 {
        cpp!(unsafe [self as "SBBreakpoint*"] -> usize as "size_t" {
            return self->GetNumLocations();
        }) as u32
    }
    pub fn num_resolved_locations(&self) -> u32 {
        cpp!(unsafe [self as "SBBreakpoint*"] -> usize as "size_t" {
            return self->GetNumResolvedLocations();
        }) as u32
    }
    pub fn location_at_index(&self, index: u32) -> SBBreakpointLocation {
        cpp!(unsafe [self as "SBBreakpoint*", index as "uint32_t"] -> SBBreakpointLocation as "SBBreakpointLocation" {
            return self->GetLocationAtIndex(index);
        })
    }
    pub fn locations<'a>(&'a self) -> impl Iterator<Item = SBBreakpointLocation> + 'a {
        SBIterator::new(self.num_locations(), move |index| self.location_at_index(index))
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBBreakpointLocation as "SBBreakpointLocation");

unsafe impl Send for SBBreakpointLocation {}

impl SBBreakpointLocation {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBBreakpointLocation*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn id(&self) -> u32 {
        cpp!(unsafe [self as "SBBreakpointLocation*"] -> BreakpointID as "uint32_t" {
            return self->GetID();
        })
    }
    pub fn address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBBreakpointLocation*"] -> SBAddress as "SBAddress" {
            return self->GetAddress();
        })
    }
    pub fn breakpoint(&self) -> SBBreakpoint {
        cpp!(unsafe [self as "SBBreakpointLocation*"] -> SBBreakpoint as "SBBreakpoint" {
            return self->GetBreakpoint();
        })
    }
    pub fn is_enabled(&self) -> bool {
        cpp!(unsafe [self as "SBBreakpointLocation*"] -> bool as "bool" {
            return self->IsEnabled();
        })
    }
    pub fn set_enabled(&self, enabled: bool) {
        cpp!(unsafe [self as "SBBreakpointLocation*", enabled as "bool"] {
            self->SetEnabled(enabled);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBAddress as "SBAddress");

unsafe impl Send for SBAddress {}

impl SBAddress {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBAddress*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn file_address(&self) -> usize {
        cpp!(unsafe [self as "SBAddress*"] -> usize as "size_t" {
            return self->GetFileAddress();
        })
    }
    pub fn load_address(&self, target: &SBTarget) -> usize {
        cpp!(unsafe [self as "SBAddress*", target as "SBTarget*"] -> usize as "size_t" {
            return self->GetLoadAddress(*target);
        })
    }
    pub fn offset(&self) -> usize {
        cpp!(unsafe [self as "SBAddress*"] -> usize as "size_t" {
            return self->GetOffset();
        })
    }
    pub fn line_entry(&self) -> Option<SBLineEntry> {
        let line_entry = cpp!(unsafe [self as "SBAddress*"] -> SBLineEntry as "SBLineEntry" {
            return self->GetLineEntry();
        });
        if line_entry.is_valid() {
            Some(line_entry)
        } else {
            None
        }
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBLineEntry as "SBLineEntry");

unsafe impl Send for SBLineEntry {}

impl SBLineEntry {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBLineEntry*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn line(&self) -> u32 {
        cpp!(unsafe [self as "SBLineEntry*"] -> u32 as "uint32_t" {
            return self->GetLine();
        })
    }
    pub fn column(&self) -> u32 {
        cpp!(unsafe [self as "SBLineEntry*"] -> u32 as "uint32_t" {
            return self->GetColumn();
        })
    }
    pub fn file_spec(&self) -> SBFileSpec {
        cpp!(unsafe [self as "SBLineEntry*"] -> SBFileSpec as "SBFileSpec" {
            return self->GetFileSpec();
        })
    }
    pub fn start_address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBLineEntry*"] -> SBAddress as "SBAddress" {
            return self->GetStartAddress();
        })
    }
    pub fn end_address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBLineEntry*"] -> SBAddress as "SBAddress" {
            return self->GetEndAddress();
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBFileSpec as "SBFileSpec");

unsafe impl Send for SBFileSpec {}

impl SBFileSpec {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBFileSpec*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn filename(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBFileSpec*"] -> *const c_char as "const char*" {
            return self->GetFilename();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn directory(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBFileSpec*"] -> *const c_char as "const char*" {
            return self->GetDirectory();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn path(&self) -> String {
        get_string(64, |ptr, size| {
            cpp!(unsafe [self as "SBFileSpec*", ptr as "char*", size as "size_t"] -> u32 as "uint32_t" {
                return self->GetPath(ptr, size);
            }) as usize
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBValue as "SBValue");

unsafe impl Send for SBValue {}

impl SBValue {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBValue*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
}
