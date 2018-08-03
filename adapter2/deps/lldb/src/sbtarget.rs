use super::*;

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
