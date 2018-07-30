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
pub type Address = u64;

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
    pub fn command_interpreter(&self) -> SBCommandInterpreter {
        cpp!(unsafe [self as "SBDebugger*"] ->  SBCommandInterpreter as "SBCommandInterpreter" {
            return self->GetCommandInterpreter();
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
        cpp!(unsafe [self as "SBError*"] -> bool as "bool" {
            return self->Success();
        })
    }
    pub fn fail(&self) -> bool {
        cpp!(unsafe [self as "SBError*"] -> bool as "bool" {
            return self->Fail();
        })
    }
    pub fn message(&self) -> &str {
        let cs_ptr = cpp!(unsafe [self as "SBError*"] -> *const c_char as "const char*" {
                return self->GetCString();
            });
        match unsafe { CStr::from_ptr(cs_ptr) }.to_str() {
            Ok(s) => s,
            _ => panic!("Invalid string?"),
        }
    }
}

impl fmt::Display for SBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl fmt::Debug for SBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !self.is_valid() {
            f.write_str("Invalid")
        } else if self.success() {
            f.write_str("Success")
        } else {
            write!(f, "Failure({})", self.message())
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
    pub fn launch(&self, launch_info: &SBLaunchInfo) -> Result<SBProcess, SBError> {
        let mut error = SBError::new();

        let process = cpp!(unsafe [self as "SBTarget*", launch_info as "SBLaunchInfo*", mut error as "SBError"] -> SBProcess as "SBProcess" {
            return self->Launch(*launch_info, error);
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
    pub fn breakpoint_delete(&self, id: BreakpointID) -> bool {
        cpp!(unsafe [self as "SBTarget*", id as "break_id_t"] -> bool as "bool" {
            return self->BreakpointDelete(id);
        })
    }
    pub fn read_instructions(&self, base_addr: &SBAddress, count: u32) -> SBInstructionList {
        let base_addr = base_addr.clone();
        cpp!(unsafe [self as "SBTarget*", base_addr as "SBAddress*", count as "uint32_t"] -> SBInstructionList as "SBInstructionList" {
            return self->ReadInstructions(*base_addr, count);
        })
    }
    pub fn evaluate_expression(&self, expr: &str) -> SBValue {
        with_cstr(expr, |expr| {
            cpp!(unsafe [self as "SBTarget*", expr as "const char*"] -> SBValue as "SBValue" {
                return self->EvaluateExpression(expr);
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
    pub fn set_arguments<'a>(&self, args: impl IntoIterator<Item = &'a str>, append: bool) {
        let cstrs: Vec<CString> = args.into_iter().map(|a| CString::new(a).unwrap()).collect();
        let mut ptrs: Vec<*const c_char> = cstrs.iter().map(|cs| cs.as_ptr()).collect();
        ptrs.push(ptr::null());
        let argv = ptrs.as_ptr();
        cpp!(unsafe [self as "SBLaunchInfo*", argv as "const char**", append as "bool"] {
            self->SetArguments(argv, append);
        });
    }
    pub fn set_environment_entries<'a>(&self, env: impl IntoIterator<Item = &'a str>, append: bool) {
        let cstrs: Vec<CString> = env.into_iter().map(|a| CString::new(a).unwrap()).collect();
        let mut ptrs: Vec<*const c_char> = cstrs.iter().map(|cs| cs.as_ptr()).collect();
        ptrs.push(ptr::null());
        let envp = ptrs.as_ptr();
        cpp!(unsafe [self as "SBLaunchInfo*", envp as "const char**", append as "bool"] {
            self->SetEnvironmentEntries(envp, append);
        });
    }
    pub fn set_working_directory(&self, cwd: &str) {
        with_cstr(cwd, |cwd| {
            cpp!(unsafe [self as "SBLaunchInfo*", cwd as "const char*"] {
                self->SetWorkingDirectory(cwd);
            });
        })
    }
    pub fn set_launch_flags(&self, flags: u32) {
        cpp!(unsafe [self as "SBLaunchInfo*", flags as "uint32_t"] {
            self->SetLaunchFlags(flags);
        })
    }
    pub fn launch_flags(&self) -> u32 {
        cpp!(unsafe [self as "SBLaunchInfo*"] -> u32 as "uint32_t" {
            return self->GetLaunchFlags();
        })
    }

    pub const eLaunchFlagNone: u32 = 0;
    pub const eLaunchFlagExec: u32 = (1 << 0); // Exec when launching and turn the calling
                                               // process into a new process
    pub const eLaunchFlagDebug: u32 = (1 << 1); // Stop as soon as the process launches to
                                                // allow the process to be debugged
    pub const eLaunchFlagStopAtEntry: u32 = (1 << 2); // Stop at the program entry point
                                                      // instead of auto-continuing when
                                                      // launching or attaching at entry point
    pub const eLaunchFlagDisableASLR: u32 = (1 << 3); // Disable Address Space Layout Randomization
    pub const eLaunchFlagDisableSTDIO: u32 = (1 << 4); // Disable stdio for inferior process (e.g. for a GUI app)
    pub const eLaunchFlagLaunchInTTY: u32 = (1 << 5); // Launch the process in a new TTY if supported by the host
    pub const eLaunchFlagLaunchInShell: u32 = (1 << 6); // Launch the process inside a shell to get shell expansion
    pub const eLaunchFlagLaunchInSeparateProcessGroup: u32 = (1 << 7); // Launch the process in a separate process group
    pub const eLaunchFlagDontSetExitStatus: u32 = (1 << 8); // If you are going to hand the
                                                            // process off (e.g. to
                                                            // debugserver)
                                                            // set this flag so lldb & the handee don't race to set its exit status.
    pub const eLaunchFlagDetachOnError: u32 = (1 << 9); // If set, then the client stub
                                                        // should detach rather than killing
                                                        // the debugee
                                                        // if it loses connection with lldb.
    pub const eLaunchFlagShellExpandArguments: u32 = (1 << 10); // Perform shell-style argument expansion
    pub const eLaunchFlagCloseTTYOnExit: u32 = (1 << 11); // Close the open TTY on exit
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

impl fmt::Debug for SBEvent {
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

#[derive(Clone, Copy, Eq, PartialEq)]
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
    pub fn resume(&self) -> SBError {
        cpp!(unsafe [self as "SBProcess*"] -> SBError as "SBError" {
            return self->Continue();
        })
    }
    pub fn stop(&self) -> SBError {
        cpp!(unsafe [self as "SBProcess*"] -> SBError as "SBError" {
            return self->Stop();
        })
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
    pub fn stop_return_value(&self) -> Option<SBValue> {
        let value = cpp!(unsafe [self as "SBThread*"] -> SBValue as "SBValue" {
                return self->GetStopReturnValue();
            });
        if value.is_valid() {
            Some(value)
        } else {
            None
        }
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
    pub fn step_over(&self) {
        cpp!(unsafe [self as "SBThread*"] {
            return self->StepOver();
        })
    }
    pub fn step_into(&self) {
        cpp!(unsafe [self as "SBThread*"] {
            return self->StepInto();
        })
    }
    pub fn step_out(&self) {
        cpp!(unsafe [self as "SBThread*"] {
            return self->StepOut();
        })
    }
    pub fn step_instruction(&self, step_over: bool) {
        cpp!(unsafe [self as "SBThread*", step_over as "bool"] {
            return self->StepInstruction(step_over);
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
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
    pub fn thread(&self) -> SBThread {
        cpp!(unsafe [self as "SBFrame*"] -> SBThread as "SBThread" {
            return self->GetThread();
        })
    }
    pub fn variables(&self, options: &VariableOptions) -> SBValueList {
        let VariableOptions {
            arguments,
            locals,
            statics,
            in_scope_only,
            use_dynamic,
        } = *options;
        cpp!(unsafe [self as "SBFrame*", arguments as "bool", locals as "bool", statics as "bool",
                     in_scope_only as "bool", use_dynamic as "uint32_t"] -> SBValueList as "SBValueList" {
            return self->GetVariables(arguments, locals, statics, in_scope_only, (lldb::DynamicValueType)use_dynamic);
        })
    }
    pub fn evaluate_expression(&self, expr: &str) -> SBValue {
        with_cstr(expr, |expr| {
            cpp!(unsafe [self as "SBFrame*", expr as "const char*"] -> SBValue as "SBValue" {
                return self->EvaluateExpression(expr);
            })
        })
    }
    pub fn registers(&self) -> SBValueList {
        cpp!(unsafe [self as "SBFrame*"] -> SBValueList as "SBValueList" {
            return self->GetRegisters();
        })
    }
    pub fn pc(&self) -> Address {
        cpp!(unsafe [self as "SBFrame*"] -> Address as "addr_t" {
            return self->GetPC();
        })
    }
    pub fn sp(&self) -> Address {
        cpp!(unsafe [self as "SBFrame*"] -> Address as "addr_t" {
            return self->GetSP();
        })
    }
    pub fn fp(&self) -> Address {
        cpp!(unsafe [self as "SBFrame*"] -> Address as "addr_t" {
            return self->GetFP();
        })
    }
}

#[derive(Clone, Copy)]
pub struct VariableOptions {
    pub arguments: bool,
    pub locals: bool,
    pub statics: bool,
    pub in_scope_only: bool,
    pub use_dynamic: DynamicValueType,
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
pub enum DynamicValueType {
    NoDynamicValues = 0,
    DynamicCanRunTarget = 1,
    DynamicDontRunTarget = 2,
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
    pub fn load_address(&self, target: &SBTarget) -> u64 {
        cpp!(unsafe [self as "SBAddress*", target as "SBTarget*"] -> u64 as "uint64_t" {
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
    pub fn symbol(&self) -> Option<SBSymbol> {
        let symbol = cpp!(unsafe [self as "SBAddress*"] -> SBSymbol as "SBSymbol" {
            return self->GetSymbol();
        });
        if symbol.is_valid() {
            Some(symbol)
        } else {
            None
        }
    }
    pub fn get_description(&self, description: &mut SBStream) -> bool {
        cpp!(unsafe [self as "SBAddress*", description as "SBStream*"] -> bool as "bool" {
            return self->GetDescription(*description);
        })
    }
}

impl fmt::Debug for SBAddress {
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
    pub fn error(&self) -> SBError {
        cpp!(unsafe [self as "SBValue*"] -> SBError as "SBError" {
            return self->GetError();
        })
    }
    pub fn name(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBValue*"] -> *const c_char as "const char*" {
            return self->GetName();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn type_name(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBValue*"] -> *const c_char as "const char*" {
            return self->GetTypeName();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn display_type_name(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBValue*"] -> *const c_char as "const char*" {
            return self->GetDisplayTypeName();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn is_synthetic(&self) -> bool {
        cpp!(unsafe [self as "SBValue*"] -> bool as "bool" {
            return self->IsSynthetic();
        })
    }
    pub fn value_type(&self) -> ValueType {
        cpp!(unsafe [self as "SBValue*"] -> ValueType as "uint32_t" {
            return self->GetValueType();
        })
    }
    pub fn value(&self) -> Option<&CStr> {
        let ptr = cpp!(unsafe [self as "SBValue*"] -> *const c_char as "const char*" {
            return self->GetValue();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr)) }
        }
    }
    pub fn summary(&self) -> Option<&CStr> {
        let ptr = cpp!(unsafe [self as "SBValue*"] -> *const c_char as "const char*" {
            return self->GetSummary();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr)) }
        }
    }
    pub fn num_children(&self) -> u32 {
        cpp!(unsafe [self as "SBValue*"] -> u32 as "uint32_t" {
            return self->GetNumChildren();
        })
    }
    pub fn child_at_index(&self, index: u32) -> SBValue {
        cpp!(unsafe [self as "SBValue*", index as "uint32_t"] -> SBValue as "SBValue" {
            return self->GetChildAtIndex(index);
        })
    }
    pub fn children<'a>(&'a self) -> impl Iterator<Item = SBValue> + 'a {
        SBIterator::new(self.num_children(), move |index| self.child_at_index(index))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
pub enum ValueType {
    Invalid = 0,
    VariableGlobal = 1,      // globals variable
    VariableStatic = 2,      // static variable
    VariableArgument = 3,    // function argument variables
    VariableLocal = 4,       // function local variables
    Register = 5,            // stack frame register value
    RegisterSet = 6,         // A collection of stack frame register values
    ConstResult = 7,         // constant result variables
    VariableThreadLocal = 8, // thread local storage variable
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBValueList as "SBValueList");

unsafe impl Send for SBValueList {}

impl SBValueList {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBValueList*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn len(&self) -> usize {
        cpp!(unsafe [self as "SBValueList*"] -> usize as "size_t" {
            return self->GetSize();
        })
    }
    pub fn value_at_index(&self, index: u32) -> SBValue {
        cpp!(unsafe [self as "SBValueList*", index as "uint32_t"] -> SBValue as "SBValue" {
            return self->GetValueAtIndex(index);
        })
    }
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = SBValue> + 'a {
        SBIterator::new(self.len() as u32, move |index| self.value_at_index(index))
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBSymbol as "SBSymbol");

unsafe impl Send for SBSymbol {}

impl SBSymbol {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBSymbol*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn name(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBSymbol*"] -> *const c_char as "const char*" {
            return self->GetName();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn display_name(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBSymbol*"] -> *const c_char as "const char*" {
            return self->GetDisplayName();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn mangled_name(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBSymbol*"] -> *const c_char as "const char*" {
            return self->GetMangledName();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn start_address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBSymbol*"] -> SBAddress as "SBAddress" {
            return self->GetStartAddress();
        })
    }
    pub fn end_address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBSymbol*"] -> SBAddress as "SBAddress" {
            return self->GetEndAddress();
        })
    }
    pub fn instructions(&self, target: &SBTarget) -> SBInstructionList {
        let target = target.clone();
        cpp!(unsafe [self as "SBSymbol*", target as "SBTarget"] -> SBInstructionList as "SBInstructionList" {
            return self->GetInstructions(target);
        })
    }
    pub fn get_description(&self, description: &mut SBStream) -> bool {
        cpp!(unsafe [self as "SBSymbol*", description as "SBStream*"] -> bool as "bool" {
            return self->GetDescription(*description);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBInstruction as "SBInstruction");

unsafe impl Send for SBInstruction {}

impl SBInstruction {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBInstruction*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn address(&self) -> SBAddress {
        cpp!(unsafe [self as "SBInstruction*"] -> SBAddress as "SBAddress" {
            return self->GetAddress();
        })
    }
    pub fn mnemonic(&self, target: &SBTarget) -> &str {
        let target = target.clone();
        let ptr = cpp!(unsafe [self as "SBInstruction*", target as "SBTarget"] -> *const c_char as "const char*" {
            return self->GetMnemonic(target);
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn operands(&self, target: &SBTarget) -> &str {
        let target = target.clone();
        let ptr = cpp!(unsafe [self as "SBInstruction*", target as "SBTarget"] -> *const c_char as "const char*" {
            return self->GetOperands(target);
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn comment(&self, target: &SBTarget) -> &str {
        let target = target.clone();
        let ptr = cpp!(unsafe [self as "SBInstruction*", target as "SBTarget"] -> *const c_char as "const char*" {
            return self->GetComment(target);
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn byte_size(&self) -> usize {
        cpp!(unsafe [self as "SBInstruction*"] -> usize as "size_t" {
            return self->GetByteSize();
        })
    }
    pub fn data(&self, target: &SBTarget) -> SBData {
        let target = target.clone();
        cpp!(unsafe [self as "SBInstruction*", target as "SBTarget"] -> SBData as "SBData" {
            return self->GetData(target);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBData as "SBData");

unsafe impl Send for SBData {}

impl SBData {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBData*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn byte_size(&self) -> usize {
        cpp!(unsafe [self as "SBData*"] -> usize as "size_t" {
            return self->GetByteSize();
        })
    }
    pub fn read_raw_data(&self, offset: u64, buffer: &mut [u8]) -> Result<(), SBError> {
        let ptr = buffer.as_ptr();
        let size = buffer.len();
        let mut error = SBError::new();
        cpp!(unsafe [self as "SBData*", mut error as "SBError", offset as "offset_t",
                     ptr as "void*", size as "size_t"] -> usize as "size_t" {
            return self->ReadRawData(error, offset, ptr, size);
        });
        if error.success() {
            Ok(())
        } else {
            Err(error)
        }
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBInstructionList as "SBInstructionList");

unsafe impl Send for SBInstructionList {}

impl SBInstructionList {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBInstructionList*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn len(&self) -> usize {
        cpp!(unsafe [self as "SBInstructionList*"] -> usize as "size_t" {
            return self->GetSize();
        })
    }
    pub fn instruction_at_index(&self, index: u32) -> SBInstruction {
        cpp!(unsafe [self as "SBInstructionList*", index as "uint32_t"] -> SBInstruction as "SBInstruction" {
            return self->GetInstructionAtIndex(index);
        })
    }
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = SBInstruction> + 'a {
        SBIterator::new(self.len() as u32, move |index| self.instruction_at_index(index))
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBCommandInterpreter as "SBCommandInterpreter");

unsafe impl Send for SBCommandInterpreter {}

impl SBCommandInterpreter {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBCommandInterpreter*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn handle_command(
        &self, command: &str, result: &mut SBCommandReturnObject, add_to_history: bool,
    ) -> ReturnStatus {
        with_cstr(command, |command| {
            cpp!(unsafe [self as "SBCommandInterpreter*", command as "const char*",
                         result as "SBCommandReturnObject*", add_to_history as "bool"] -> ReturnStatus as "ReturnStatus" {
                return self->HandleCommand(command, *result, add_to_history);
            })
        })
    }
    pub fn handle_command_with_context(
        &self, command: &str, context: &SBExecutionContext, result: &mut SBCommandReturnObject, add_to_history: bool,
    ) -> ReturnStatus {
        with_cstr(command, |command| {
            cpp!(unsafe [self as "SBCommandInterpreter*", command as "const char*", context as "SBExecutionContext*",
                         result as "SBCommandReturnObject*", add_to_history as "bool"] -> ReturnStatus as "ReturnStatus" {
                return self->HandleCommand(command, *context, *result, add_to_history);
            })
        })
    }
}

#[repr(u32)]
pub enum ReturnStatus {
    Invalid = 0,
    SuccessFinishNoResult = 1,
    SuccessFinishResult = 2,
    SuccessContinuingNoResult = 3,
    SuccessContinuingResult = 4,
    Started = 5,
    Failed = 6,
    Quit = 7,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBExecutionContext as "SBExecutionContext");

unsafe impl Send for SBExecutionContext {}

impl SBExecutionContext {
    pub fn new() -> SBExecutionContext {
        cpp!(unsafe [] -> SBExecutionContext as "SBExecutionContext" {
            return SBExecutionContext();
        })
    }
    pub fn from_target(target: &SBTarget) -> SBExecutionContext {
        cpp!(unsafe [target as "SBTarget*"] -> SBExecutionContext as "SBExecutionContext" {
            return SBExecutionContext(*target);
        })
    }
    pub fn from_process(process: &SBProcess) -> SBExecutionContext {
        cpp!(unsafe [process as "SBProcess*"] -> SBExecutionContext as "SBExecutionContext" {
            return SBExecutionContext(*process);
        })
    }
    pub fn from_thread(thread: &SBThread) -> SBExecutionContext {
        cpp!(unsafe [thread as "SBThread*"] -> SBExecutionContext as "SBExecutionContext" {
            return SBExecutionContext(*thread);
        })
    }
    pub fn from_frame(frame: &SBFrame) -> SBExecutionContext {
        cpp!(unsafe [frame as "SBFrame*"] -> SBExecutionContext as "SBExecutionContext" {
            return SBExecutionContext(*frame);
        })
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

cpp_class!(pub unsafe struct SBCommandReturnObject as "SBCommandReturnObject");

unsafe impl Send for SBCommandReturnObject {}

impl SBCommandReturnObject {
    pub fn new() -> SBCommandReturnObject {
        cpp!(unsafe [] -> SBCommandReturnObject as "SBCommandReturnObject" {
            return SBCommandReturnObject();
        })
    }
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn clear(&self) {
        cpp!(unsafe [self as "SBCommandReturnObject*"] {
            return self->Clear();
        })
    }
    pub fn status(&self) -> ReturnStatus {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> ReturnStatus as "ReturnStatus" {
            return self->GetStatus();
        })
    }
    pub fn succeeded(&self) -> bool {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> bool as "bool" {
            return self->Succeeded();
        })
    }
    pub fn has_result(&self) -> bool {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> bool as "bool" {
            return self->HasResult();
        })
    }
    pub fn output_size(&self) -> usize {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> usize as "size_t" {
            return self->GetOutputSize();
        })
    }
    pub fn error_size(&self) -> usize {
        cpp!(unsafe [self as "SBCommandReturnObject*"] -> usize as "size_t" {
            return self->GetErrorSize();
        })
    }
    pub fn output(&self) -> &CStr {
        let ptr = cpp!(unsafe [self as "SBCommandReturnObject*"] -> *const c_char as "const char*" {
            return self->GetOutput();
        });
        if ptr.is_null() {
            Default::default()
        } else {
            unsafe { CStr::from_ptr(ptr) }
        }
    }
    pub fn error(&self) -> &CStr {
        let ptr = cpp!(unsafe [self as "SBCommandReturnObject*"] -> *const c_char as "const char*" {
            return self->GetError();
        });
        if ptr.is_null() {
            Default::default()
        } else {
            unsafe { CStr::from_ptr(ptr) }
        }
    }
}
