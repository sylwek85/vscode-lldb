import sys
import logging
import re
import lldb

PY2 = sys.version_info[0] == 2
if PY2:
    # python2-based LLDB accepts utf8-encoded ascii strings only.
    to_lldb_str = lambda s: s.encode('utf8', 'backslashreplace') if isinstance(s, unicode) else s
    xrange = xrange
else:
    to_lldb_str = str
    xrange = range

log = logging.getLogger('rust')

module = sys.modules[__name__]
serial = 0


def __lldb_init_module(debugger, internal_dict):
    global module
    log.info('Initializing, module name=%s', __name__)

    #debugger.HandleCommand('script import adapter.formatters.rust')
    module.rust_category = debugger.CreateCategory('Rust')
    #rust_category.AddLanguage(lldb.eLanguageTypeRust)
    rust_category.SetEnabled(True)

    attach_summary_to_type(get_array_summary, r'^.*\[[0-9]+\]$', True)

    attach_synthetic_to_type(StrSliceSynthProvider, '&str')

    attach_synthetic_to_type(StdStringSynthProvider, 'collections::string::String')
    attach_synthetic_to_type(StdStringSynthProvider, 'alloc::string::String')

    attach_synthetic_to_type(StdVectorSynthProvider, r'^collections::vec::Vec<.+>$', True)
    attach_synthetic_to_type(StdVectorSynthProvider, r'^alloc::vec::Vec<.+>$', True)

    attach_synthetic_to_type(SliceSynthProvider, r'^&(mut\s*)?\[.*\]$', True)

    attach_synthetic_to_type(StdCStringSynthProvider, 'std::ffi::c_str::CString')
    attach_synthetic_to_type(StdCStrSynthProvider, 'std::ffi::c_str::CStr')

    attach_synthetic_to_type(StdOsStringSynthProvider, 'std::ffi::os_str::OsString')
    attach_synthetic_to_type(StdOsStrSynthProvider, 'std::ffi::os_str::OsStr')

    attach_synthetic_to_type(StdPathBufSynthProvider, 'std::path::PathBuf')
    attach_synthetic_to_type(StdPathSynthProvider, 'std::path::Path')

    target = debugger.GetSelectedTarget()
    modules = target.modules
    log.info('### analyzing %d modules', len(modules))
    for module in modules:
        analyze_module(module)

def analyze_module(sbmodule):
    for cu in module.compile_units:
        if cu.GetLanguage() == lldb.eLanguageTypeRust:
            log.info('### analyzing unit %s', cu.file)
            types = cu.GetTypes(lldb.eTypeClassUnion | lldb.eTypeClassStruct)
            for ty in types:
                #log.info('### analyzing type %s (%s)', ty.GetName(), ty.GetDisplayTypeName())
                analyze_type(ty)

# Enums and tuples cannot be recognized based on type name.
# These require deeper runtime analysis to tease them apart.
ENUM_DISCRIMINANT = 'RUST$ENUM$DISR'
ENCODED_ENUM_PREFIX = 'RUST$ENCODED$ENUM$'

def analyze_type(obj_type):
    num_fields = obj_type.GetNumberOfFields()
    if num_fields == 0:
        return
    type_class = obj_type.GetTypeClass()
    if type_class == lldb.eTypeClassUnion:
        if num_fields == 1:
            first_variant_name = obj_type.GetFieldAtIndex(0).GetName()
            if first_variant_name is None: # Singleton
                attach_summary_to_type(get_singleton_enum_summary, obj_type.GetDisplayTypeName())
            elif first_variant_name.startswith(ENCODED_ENUM_PREFIX): # Zero-optimized enum
                provider_class = make_encoded_enum_provider_class(first_variant_name)
                attach_synthetic_to_type(provider_class, obj_type.GetDisplayTypeName())
        else: # Regular enum
            attach_synthetic_to_type(RegularEnumProvider, obj_type.GetDisplayTypeName())
    elif type_class == lldb.eTypeClassStruct:
        first_field_name = obj_type.GetFieldAtIndex(0).GetName()
        if first_field_name == ENUM_DISCRIMINANT: # Enum variant
            attach_summary_to_type(get_enum_variant_summary, obj_type.GetDisplayTypeName())
        elif first_field_name == '__0': # Tuple variant or tuple struct
            attach_summary_to_type(get_tuple_summary, obj_type.GetDisplayTypeName())

def attach_synthetic_to_type(synth_class, type_name, is_regex=False):
    global rust_category
    global module
    log.debug('attaching synthetic %s to "%s", is_regex=%s', synth_class.__name__, type_name, is_regex)
    synth = lldb.SBTypeSynthetic.CreateWithClassName(__name__ + '.' + synth_class.__name__)
    synth.SetOptions(lldb.eTypeOptionCascade)
    rust_category.AddTypeSynthetic(lldb.SBTypeNameSpecifier(type_name, is_regex), synth)

    summary_fn = lambda valobj, dict: get_synth_summary(synth_class, valobj, dict)
    # LLDB accesses summary fn's by name, so we need to create a unique one.
    summary_fn.__name__ = '_get_synth_summary_' + synth_class.__name__
    setattr(module, summary_fn.__name__, summary_fn)
    attach_summary_to_type(summary_fn, type_name, is_regex)

def attach_summary_to_type(summary_fn, type_name, is_regex=False):
    global rust_category
    log.debug('attaching summary %s to "%s", is_regex=%s', summary_fn.__name__, type_name, is_regex)
    summary = lldb.SBTypeSummary.CreateWithFunctionName(__name__ + '.' + summary_fn.__name__)
    summary.SetOptions(lldb.eTypeOptionCascade)
    rust_category.AddTypeSummary(lldb.SBTypeNameSpecifier(type_name, is_regex), summary)

# 'get_summary' is annoyingly not a part of the standard LLDB synth provider API.
# This trick allows us to share data extraction logic between synth providers and their
# sibling summary providers.
def get_synth_summary(synth_class, valobj, dict):
    synth = synth_class(valobj.GetNonSyntheticValue(), dict)
    synth.update()
    summary = synth.get_summary()
    return to_lldb_str(summary)

# Chained GetChildMemberWithName lookups
def gcm(valobj, *chain):
    for name in chain:
        valobj = valobj.GetChildMemberWithName(name)
    return valobj

def string_from_ptr(pointer, length):
    if length <= 0:
        return u''
    error = lldb.SBError()
    process = pointer.GetProcess()
    data = process.ReadMemory(pointer.GetValueAsUnsigned(), length, error)
    if error.Success():
        return data.decode('utf8', 'replace')
    else:
        log.error('%s', error.GetCString())

def get_obj_summary(valobj, unavailable='{...}'):
    summary = valobj.GetSummary()
    if summary is not None:
        return summary
    summary = valobj.GetValue()
    if summary is not None:
        return summary
    return unavailable

def sequence_summary(childern, maxsize=32):
    s = ''
    for child in childern:
        if len(s) > 0: s += ', '
        s +=  get_obj_summary(child)
        if len(s) > maxsize:
            s += ', ...'
            break
    return s

def get_unqualified_type_name(type_name):
    if type_name[0] in unqual_type_markers:
        return type_name
    return unqual_type_regex.match(type_name).group(1)
#
unqual_type_markers = ["(", "[", "&", "*"]
unqual_type_regex = re.compile(r'^(?:\w+::)*(\w+).*', re.UNICODE)

def dump_type(ty):
    log.info('type %s: size=%d', ty.GetName(), ty.GetByteSize())

# ----- Summaries -----

def get_singleton_enum_summary(valobj, dict):
    return get_obj_summary(valobj.GetChildAtIndex(0))

def get_enum_variant_summary(valobj, dict):
    obj_type = valobj.GetType()
    num_fields = obj_type.GetNumberOfFields()
    unqual_type_name = get_unqualified_type_name(obj_type.GetName())
    if num_fields == 1:
        return unqual_type_name
    elif obj_type.GetFieldAtIndex(1).GetName().startswith('__'): # tuple variant
        fields = ', '.join([get_obj_summary(valobj.GetChildAtIndex(i)) for i in range(1, num_fields)])
        return '%s(%s)' % (unqual_type_name, fields)
    else: # struct variant
        fields = [valobj.GetChildAtIndex(i) for i in range(1, num_fields)]
        fields = ', '.join(['%s:%s' % (f.GetName(), get_obj_summary(f)) for f in fields])
        return '%s{%s}' % (unqual_type_name, fields)

def get_tuple_summary(valobj, dict):
    fields = [get_obj_summary(valobj.GetChildAtIndex(i)) for i in range(0, valobj.GetNumChildren())]
    return '(%s)' % ', '.join(fields)

def get_array_summary(valobj, dict):
    return '(%d) [%s]' % (valobj.GetNumChildren(), sequence_summary(valobj))

# ----- Synth providers ------

class RustSynthProvider(object):
    def __init__(self, valobj, dict={}):
        self.valobj = valobj
        self._real_class = self.__class__
        self.__class__ = RustSynthProvider.Uninitialized

    def initialize(self): return None
    def update(self): return True
    def num_children(self): return 0
    def has_children(self): return False
    def get_child_at_index(self, index): return None
    def get_child_index(self, name): return None
    def get_summary(self): return None

    class Uninitialized(object):
        def __do_init(self, update=False):
            self.__class__ = self._real_class
            try:
                if not update:
                    log.warning('Synth provider method has been called before update()')
                self.initialize()
            except Exception as e:
                log.error('Error during RustSynthProvider initialization: %s', e)
                self.__class__ = RustSynthProvider # This object is in a broken state, so fall back to default impls.
            return self
        def update(self):
            return self.__do_init(True).update()
        def num_children(self):
            return self.__do_init().num_children()
        def has_children(self):
            return self.__do_init().has_children()
        def get_child_at_index(self, index):
            return self.__do_init().get_child_at_index(index)
        def get_child_index(self, name):
            return self.__do_init().get_child_index(name)
        def get_summary(self):
            return self.__do_init().get_summary()


def make_encoded_enum_provider_class(variant_name):
    # 'Encoded' enums always have two variants, of which one contains no data,
    # and the other one contains a field (not necessarily at the top level) that
    # implements Zeroable.  This field is then used as a two-state discriminant.
    last_separator_index = variant_name.rfind("$")
    start_index = len(ENCODED_ENUM_PREFIX)
    indices_substring = variant_name[start_index:last_separator_index].split("$")

    class EncodedEnumProvider(RustSynthProvider):
        disr_field_indices = [int(index) for index in indices_substring]
        null_variant_name = variant_name[last_separator_index + 1:]

        def initialize(self):
            discriminant = self.valobj.GetChildAtIndex(0)
            for disr_field_index in self.disr_field_indices:
                discriminant = discriminant.GetChildAtIndex(disr_field_index)
            # Recurse down the first field of the discriminant till we reach a non-struct type,
            for i in xrange(20): # ... but limit the depth, just in case.
                if discriminant.GetType().GetTypeClass() != lldb.eTypeClassStruct:
                    break
                discriminant = discriminant.GetChildAtIndex(0)
            self.is_null_variant = discriminant.GetValueAsUnsigned() == 0
            if not self.is_null_variant:
                self.variant = self.valobj.GetChildAtIndex(0)
            return True

        def num_children(self):
            return 0 if self.is_null_variant else self.variant.GetNumChildren()

        def has_children(self):
            return False if self.is_null_variant else self.variant.MightHaveChildren()

        def get_child_at_index(self, index):
            return self.variant.GetChildAtIndex(index)

        def get_child_index(self, name):
            return self.variant.GetIndexOfChildWithName(name)

        def get_summary(self):
            if self.is_null_variant:
                return self.null_variant_name
            else:
                unqual_type_name = get_unqualified_type_name(self.valobj.GetChildAtIndex(0).GetType().GetName())
                return '%s%s' % (unqual_type_name, get_obj_summary(self.variant))

    global serial
    EncodedEnumProvider.__name__ += str(serial)
    serial += 1
    setattr(module, EncodedEnumProvider.__name__, EncodedEnumProvider)
    return EncodedEnumProvider

class RegularEnumProvider(RustSynthProvider):
    def initialize(self):
        # Regular enums are represented as unions of structs, containing discriminant in the
        # first field.
        discriminant = self.valobj.GetChildAtIndex(0).GetChildAtIndex(0).GetValueAsUnsigned()
        self.variant = self.valobj.GetChildAtIndex(discriminant)
        return True

    def num_children(self):
        return max(0, self.variant.GetNumChildren() - 1)

    def has_children(self):
        return self.num_children() > 0

    def get_child_at_index(self, index):
        return self.variant.GetChildAtIndex(index + 1)

    def get_child_index(self, name):
        return self.variant.GetIndexOfChildWithName(name) - 1

    def get_summary(self):
        return get_obj_summary(self.variant)

# Base class for providers that represent array-like objects
class ArrayLikeSynthProvider(RustSynthProvider):
    def initialize(self):
        ptr, len = self.ptr_and_len(self.valobj)
        self.ptr = ptr
        self.len = len
        self.item_type = self.ptr.GetType().GetPointeeType()
        self.item_size = self.item_type.GetByteSize()
        return True

    def ptr_and_len(self):
        raise Error('ptr_and_len must be overridden')

    def num_children(self):
        return self.len

    def has_children(self):
        return True

    def get_child_at_index(self, index):
        try:
            if not 0 <= index < self.len:
                return None
            offset = index * self.item_size
            return self.ptr.CreateChildAtOffset('[%s]' % index, offset, self.item_type)
        except Exception as e:
            log.error('%s', e)
            raise

    def get_child_index(self, name):
        try:
            return int(name.lstrip('[').rstrip(']'))
        except Exception as e:
            log.error('%s', e)
            raise

    def get_summary(self):
        return '(%d)' % (self.len,)

class StdVectorSynthProvider(ArrayLikeSynthProvider):
    def ptr_and_len(self, vec):
        return (
            gcm(vec, 'buf', 'ptr', 'pointer', '__0'),
            gcm(vec, 'len').GetValueAsUnsigned()
        )
    def get_summary(self):
        try:
            return '(%d) vec![%s]' % (self.len, sequence_summary((self.get_child_at_index(i) for i in xrange(self.len))))
        except Exception as e:
            log.error('%s', e)
            raise

class SliceSynthProvider(ArrayLikeSynthProvider):
    def ptr_and_len(self, vec):
        return (
            gcm(vec, 'data_ptr'),
            gcm(vec, 'length').GetValueAsUnsigned()
        )
    def get_summary(self):
        return '(%d) &[%s]' % (self.len, sequence_summary((self.get_child_at_index(i) for i in xrange(self.len))))

# Base class for *String providers
class StringLikeSynthProvider(ArrayLikeSynthProvider):
    def get_summary(self):
        # Limit string length to 1000 characters to cope with uninitialized values whose
        # length field contains garbage.
        strval = string_from_ptr(self.ptr, min(self.len, 1000))
        if strval == None:
            return None
        if self.len > 1000: strval += u'...'
        return u'"%s"' % strval

class StrSliceSynthProvider(StringLikeSynthProvider):
     def ptr_and_len(self, valobj):
         return (
            gcm(valobj, 'data_ptr'),
            gcm(valobj, 'length').GetValueAsUnsigned()
         )

class StdStringSynthProvider(StringLikeSynthProvider):
    def ptr_and_len(self, valobj):
        vec = gcm(valobj, 'vec')
        return (
            gcm(vec, 'buf', 'ptr', 'pointer', '__0'),
            gcm(vec, 'len').GetValueAsUnsigned()
        )

class StdCStringSynthProvider(StringLikeSynthProvider):
    def ptr_and_len(self, valobj):
        vec = gcm(valobj, 'inner')
        return (
            gcm(vec, 'data_ptr'),
            gcm(vec, 'length').GetValueAsUnsigned() - 1
        )

class StdOsStringSynthProvider(StringLikeSynthProvider):
    def ptr_and_len(self, valobj):
        vec = gcm(valobj, 'inner', 'inner')
        tmp = gcm(vec, 'bytes') # Windows OSString has an extra layer
        if tmp.IsValid():
            vec = tmp
        return (
            gcm(vec, 'buf', 'ptr', 'pointer', '__0'),
            gcm(vec, 'len').GetValueAsUnsigned()
        )

class FFISliceSynthProvider(StringLikeSynthProvider):
    def ptr_and_len(self, valobj):
        process = valobj.GetProcess()
        slice_ptr = valobj.GetLoadAddress()
        data_ptr_type = valobj.GetType().GetBasicType(lldb.eBasicTypeChar).GetPointerType()
        # Unsized slice objects have incomplete debug info, so here we just assume standard slice
        # reference layout: [<pointer to data>, <data size>]
        error = lldb.SBError()
        return (
            valobj.CreateValueFromAddress('data', slice_ptr, data_ptr_type),
            process.ReadPointerFromMemory(slice_ptr + process.GetAddressByteSize(), error)
        )

class StdCStrSynthProvider(FFISliceSynthProvider):
    def ptr_and_len(self, valobj):
        ptr, len = FFISliceSynthProvider.ptr_and_len(self, valobj)
        return (ptr, len-1) # drop terminaing '\0'

class StdOsStrSynthProvider(FFISliceSynthProvider):
    pass

class StdPathBufSynthProvider(StdOsStringSynthProvider):
    def ptr_and_len(self, valobj):
        return StdOsStringSynthProvider.ptr_and_len(self, gcm(valobj, 'inner'))

class StdPathSynthProvider(FFISliceSynthProvider):
    pass
