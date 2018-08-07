use super::*;

cpp_class!(pub unsafe struct SBModule as "SBModule");

unsafe impl Send for SBModule {}

impl SBModule {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBModule*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
}

impl fmt::Debug for SBModule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_descr(f, |descr| {
            cpp!(unsafe [self as "SBModule*", descr as "SBStream*"] -> bool as "bool" {
                return self->GetDescription(*descr);
            })
        })
    }
}
