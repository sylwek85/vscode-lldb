use super::*;

cpp_class!(pub unsafe struct SBTarget as "SBTarget");

unsafe impl Send for SBTarget {}

impl SBTarget {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBTarget*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn debugger(&self) -> SBDebugger {
        cpp!(unsafe [self as "SBTarget*"] -> SBDebugger as "SBDebugger" {
            return self->GetDebugger();
        })
    }
    pub fn broadcaster(&self) -> SBBroadcaster {
        cpp!(unsafe [self as "SBTarget*"] -> SBBroadcaster as "SBBroadcaster" {
            return self->GetBroadcaster();
        })
    }
    pub fn launch(&self, launch_info: &SBLaunchInfo) -> Result<SBProcess, SBError> {
        let mut error = SBError::new();

        let process = cpp!(unsafe [self as "SBTarget*", launch_info as "SBLaunchInfo*", mut error as "SBError"] -> SBProcess as "SBProcess" {
            return self->Launch(*launch_info, error);
        });
        if error.is_success() {
            Ok(process)
        } else {
            Err(error)
        }
    }
    pub fn find_breakpoint_by_id(&self, id: BreakpointID) -> Option<SBBreakpoint> {
        let bp = cpp!(unsafe [self as "SBTarget*", id as "break_id_t"] -> SBBreakpoint as "SBBreakpoint" {
            return self->FindBreakpointByID(id);
        });
        if bp.is_valid() {
            Some(bp)
        } else {
            None
        }
    }
    pub fn breakpoint_create_by_location(&self, file: &str, line: u32) -> SBBreakpoint {
        with_cstr(file, |file| {
            cpp!(unsafe [self as "SBTarget*", file as "const char*", line as "uint32_t"] -> SBBreakpoint as "SBBreakpoint" {
                return self->BreakpointCreateByLocation(file, line);
            })
        })
    }
    pub fn breakpoint_create_by_name(&self, name: &str) -> SBBreakpoint {
        with_cstr(name, |name| {
            cpp!(unsafe [self as "SBTarget*", name as "const char*"] -> SBBreakpoint as "SBBreakpoint" {
                return self->BreakpointCreateByName(name);
            })
        })
    }
    pub fn breakpoint_create_by_regex(&self, regex: &str) -> SBBreakpoint {
        with_cstr(regex, |regex| {
            cpp!(unsafe [self as "SBTarget*", regex as "const char*"] -> SBBreakpoint as "SBBreakpoint" {
                return self->BreakpointCreateByRegex(regex);
            })
        })
    }
    pub fn breakpoint_delete(&self, id: BreakpointID) -> bool {
        cpp!(unsafe [self as "SBTarget*", id as "break_id_t"] -> bool as "bool" {
            return self->BreakpointDelete(id);
        })
    }
    pub fn read_instructions(&self, base_addr: &SBAddress, count: u32) -> SBInstructionList {
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

impl fmt::Debug for SBTarget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_descr(f, |descr| {
            cpp!(unsafe [self as "SBTarget*", descr as "SBStream*"] -> bool as "bool" {
                return self->GetDescription(*descr, eDescriptionLevelBrief);
            })
        })
    }
}
