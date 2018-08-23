use super::*;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::Mutex;

cpp_class!(pub unsafe struct SBBreakpoint as "SBBreakpoint");

unsafe impl Send for SBBreakpoint {}

type SBBreakpointHitCallback = FnMut(&SBProcess, &SBThread, &SBBreakpointLocation) + Send;

lazy_static! {
    static ref CALLBACKS: Mutex<HashMap<BreakpointID, Box<SBBreakpointHitCallback>>> = { Mutex::new(HashMap::new()) };
}

impl SBBreakpoint {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBBreakpoint*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn id(&self) -> BreakpointID {
        cpp!(unsafe [self as "SBBreakpoint*"] -> BreakpointID as "break_id_t" {
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
    pub fn condition(&self) -> Option<&str> {
        let ptr = cpp!(unsafe [self as "SBBreakpoint*"] -> *const c_char as "const char*" {
            return self->GetCondition();
        });
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(ptr).to_str().unwrap()) }
        }
    }
    pub fn set_condition(&self, condition: &str) {
        with_cstr(condition, |condition| {
            cpp!(unsafe [self as "SBBreakpoint*", condition as "const char*"] {
                self->SetCondition(condition);
            });
        });
    }
    pub fn set_callback<F>(&self, callback: F)
    where
        F: FnMut(&SBProcess, &SBThread, &SBBreakpointLocation) + Send + 'static,
    {
        unsafe extern "C" fn callback_thunk(
            _: *mut c_void, process: *const SBProcess, thread: *const SBThread, location: *const SBBreakpointLocation,
        ) {
            let bp_id = (*location).breakpoint().id();
            let mut callbacks = CALLBACKS.lock().unwrap();
            if let Some(callback) = callbacks.get_mut(&bp_id) {
                callback(&*process, &*thread, &*location);
            }
        }

        let bp_id = self.id();
        let mut callbacks = CALLBACKS.lock().unwrap();
        callbacks.insert(bp_id, Box::new(callback));

        let cb = &callback_thunk;
        cpp!(unsafe [self as "SBBreakpoint*", cb as "SBBreakpointHitCallback"] {
            self->SetCallback(cb, nullptr);
        });
    }
    pub fn remove_callback(breakpoint_id: BreakpointID) {
        let mut callbacks = CALLBACKS.lock().unwrap();
        callbacks.remove(&breakpoint_id);
    }
}

impl fmt::Debug for SBBreakpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_descr(f, |descr| {
            cpp!(unsafe [self as "SBBreakpoint*", descr as "SBStream*"] -> bool as "bool" {
                return self->GetDescription(*descr);
            })
        })
    }
}
