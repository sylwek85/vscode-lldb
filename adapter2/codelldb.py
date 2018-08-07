import sys
import lldb
import traceback
import logging
from ctypes import *
from value import Value

logging.basicConfig(level=logging.INFO, filename='/tmp/codelldb.log', datefmt='%H:%M:%S',
                    format='[%(asctime)s %(name)s] %(message)s')

#============================================================================================

RESULT_CALLBACK = CFUNCTYPE(None, c_int, c_void_p, c_size_t, c_void_p)

def evaluate(script, simple_expr, callback_addr, param_addr):
    callback = RESULT_CALLBACK(callback_addr)

    if simple_expr:
        eval_globals = {}
        eval_locals = PyEvalContext(lldb.frame)
        eval_globals['__frame_vars'] = eval_locals
    else:
        import __main__
        eval_globals = getattr(__main__, lldb.debugger.GetInstanceName() + '_dict')
        eval_globals['__frame_vars'] = PyEvalContext(lldb.frame)
        eval_locals = {}

    try:
        result = eval(script, eval_globals, eval_locals)
        result = Value.unwrap(result)
        if isinstance(result, lldb.SBValue):
            callback(1, long(result.this), 0, param_addr)
        else:
            s = str(result)
            callback(2, s, len(s), param_addr)
    except Exception as e:
        s = traceback.format_exc()
        callback(3, s, len(s), param_addr)

def find_var_in_frame(sbframe, name):
    val = sbframe.FindVariable(name)
    if not val.IsValid():
        for val_type in [lldb.eValueTypeVariableGlobal,
                         lldb.eValueTypeVariableStatic,
                         lldb.eValueTypeRegister,
                         lldb.eValueTypeConstResult]:
            val = sbframe.FindValue(name, val_type)
            if val.IsValid():
                break
    if not val.IsValid():
        val = sbframe.GetValueForVariablePath(name)
    return val

# A dictionary-like object that fetches values from SBFrame (and caches them).
class PyEvalContext(dict):
    def __init__(self, sbframe):
        self.sbframe = sbframe

    def __missing__(self, name):
        val = find_var_in_frame(self.sbframe, name)
        if val.IsValid():
            val = Value(val)
            self.__setitem__(name, val)
            return val
        else:
            raise KeyError(name)

#============================================================================================

module_loaded_observers = []

# observer: Callable[SBModule]
def register_on_module_loaded(observer):
    module_loaded_observers.append(observer)

ASSIGN_SBMODULE = CFUNCTYPE(None, c_void_p, c_void_p)

def module_loaded(sbmodule_addr, assign_sbmodule_addr):
    pass
    #assign_sbmodule = ASSIGN_SBMODULE(assign_sbmodule_addr)
    # SWIG does not provide a method for wrapping raw pointers from Python,
    # so we create a dummy module object, then call back into Rust code to
    # overwrite it with the module we need wrapped.
    #sbmodule = lldb.SBModule()
    #assign_sbmodule(long(sbmodule.this), sbmodule_addr)
    #print 'module_loaded ', sbmodule
    # for observer in module_loaded_observers:
    #     try:
    #         observer(sbmodule)
    #     except Error as err:
    #         log.error('on_module_loaded observer %s raised %s', observer, err)
