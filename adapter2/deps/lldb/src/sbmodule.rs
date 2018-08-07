
cpp_class!(pub unsafe struct SBModule as "SBModule");

unsafe impl Send for SBModule {}

impl SBModule {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBModule*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
}
