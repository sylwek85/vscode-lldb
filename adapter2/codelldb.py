import lldb
import traceback
from ctypes import *

RESULT_CALLBACK = CFUNCTYPE(None, c_void_p, c_int, c_void_p, c_size_t)

def evaluate(script, callback_addr, param_addr):
    callback = RESULT_CALLBACK(callback_addr)
    eval_locals = PyEvalContext(lldb.frame)
    eval_globals = {}
    eval_globals['__frame_vars'] = eval_locals
    try:
        result = eval(script, eval_globals, eval_locals)
        if isinstance(result, lldb.SBValue):
            callback(param_addr, 1, long(result.this), 0)
        else:
            s = str(result)
            callback(param_addr, 2, s, len(s))
    except Exception as e:
        s = traceback.format_exc()
        callback(param_addr, 3, s, len(s))


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
