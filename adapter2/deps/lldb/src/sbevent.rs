use super::*;

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
        debug_descr(f, |descr| {
            cpp!(unsafe [self as "SBEvent*", descr as "SBStream*"] -> bool as "bool" {
                return self->GetDescription(*descr);
            })
        })
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

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
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
