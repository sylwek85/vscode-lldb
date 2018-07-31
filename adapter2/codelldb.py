import lldb
import ctypes

RESULT_CALLBACK = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p)

def invoke(result, callback, closure):
    RESULT_CALLBACK(callback)(result, closure)
