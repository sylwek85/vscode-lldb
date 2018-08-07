use std::mem;
use std::os::raw::{c_int, c_ulong, c_void};
use std::slice;

use regex;
use lldb::*;

use crate::debug_session::Evaluated;
use crate::lldb::*;
use crate::must_initialize::*;

pub fn initialize(interpreter: &SBCommandInterpreter) {
    let mut command_result = SBCommandReturnObject::new();
    let command = format!("command script import '/home/chega/NW/vscode-lldb/adapter2/codelldb.py'");
    interpreter.handle_command(&command, &mut command_result, false);
    info!("{:?}", command_result);
}

type EvalResult = Result<Evaluated, String>;

pub fn evaluate(
    interpreter: &SBCommandInterpreter, script: &str, simple_expr: bool, context: &SBExecutionContext,
) -> EvalResult {
    extern "C" fn callback(ty: c_int, data: *const c_void, len: usize, result_ptr: *mut EvalResult) {
        unsafe {
            *result_ptr = match ty {
                1 => {
                    let sbvalue = data as *const SBValue;
                    Ok(Evaluated::SBValue((*sbvalue).clone()))
                }
                2 => {
                    let bytes = slice::from_raw_parts(data as *const u8, len);
                    Ok(Evaluated::String(String::from_utf8_lossy(bytes).into_owned()))
                }
                3 => {
                    let bytes = slice::from_raw_parts(data as *const u8, len);
                    Err(String::from_utf8_lossy(bytes).into_owned())
                }
                _ => unreachable!(),
            }
        }
    }

    let mut command_result = SBCommandReturnObject::new();
    let mut eval_result = Err(String::new());

    let command = format!(
        "script codelldb.evaluate('{}', {}, {:#X}, {:#X})",
        script,
        if simple_expr { "True" } else { "False" },
        callback as *mut c_void as usize,
        &mut eval_result as *mut EvalResult as usize
    );
    let result = interpreter.handle_command_with_context(&command, &context, &mut command_result, false);

    info!("{:?}", command_result);
    info!("{:?}", eval_result);
    eval_result
}

pub fn module_loaded(interpreter: &SBCommandInterpreter, module: &SBModule) {
    let mut command_result = SBCommandReturnObject::new();
    let command = format!(
        "script codelldb.module_loaded({:#X},)", module as *const SBModule as usize);
    let result = interpreter.handle_command(&command, &mut command_result, false);
}
