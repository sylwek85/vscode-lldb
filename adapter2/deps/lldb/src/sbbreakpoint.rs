use super::*;

cpp_class!(pub unsafe struct SBBreakpoint as "SBBreakpoint");

unsafe impl Send for SBBreakpoint {}

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
