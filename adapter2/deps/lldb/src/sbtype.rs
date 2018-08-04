use super::*;

cpp_class!(pub unsafe struct SBType as "SBType");

unsafe impl Send for SBType {}

impl SBType {
    pub fn is_valid(&self) -> bool {
        cpp!(unsafe [self as "SBType*"] -> bool as "bool" {
            return self->IsValid();
        })
    }
    pub fn type_class(&self) -> TypeClass {
        cpp!(unsafe [self as "SBType*"] -> TypeClass as "TypeClass" {
            return self->GetTypeClass();
        })
    }
    pub fn name(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBType*"] -> *const c_char as "const char*" {
            return self->GetName();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
    pub fn display_name(&self) -> &str {
        let ptr = cpp!(unsafe [self as "SBType*"] -> *const c_char as "const char*" {
            return self->GetDisplayTypeName();
        });
        unsafe { CStr::from_ptr(ptr).to_str().unwrap() }
    }
}

impl fmt::Debug for SBType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_descr(f, |descr| {
            cpp!(unsafe [self as "SBType*", descr as "SBStream*"] -> bool as "bool" {
                return self->GetDescription(*descr, eDescriptionLevelFull);
            })
        })
    }
}

bitflags! {
    pub struct TypeClass : u32 {
        const Invalid = (0);
        const Array = (1 << 0);
        const BlockPointer = (1 << 1);
        const Builtin = (1 << 2);
        const Class = (1 << 3);
        const ComplexFloat = (1 << 4);
        const ComplexInteger = (1 << 5);
        const Enumeration = (1 << 6);
        const Function = (1 << 7);
        const MemberPointer = (1 << 8);
        const ObjCObject = (1 << 9);
        const ObjCInterface = (1 << 10);
        const ObjCObjectPointer = (1 << 11);
        const Pointer = (1 << 12);
        const Reference = (1 << 13);
        const Struct = (1 << 14);
        const Typedef = (1 << 15);
        const Union = (1 << 16);
        const Vector = (1 << 17);
        // Define the last type class as the MSBit of a 32 bit value
        const Other = (1 << 31);
        // Define a mask that can be used for any type when finding types
        const Any = !0;
    }
}
