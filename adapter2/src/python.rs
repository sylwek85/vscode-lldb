use crate::lldb::*;
use crate::must_initialize::*;
use std::mem;
use std::os::raw::{c_int, c_ulong, c_void};
use std::slice;

pub fn initialize(interpreter: &SBCommandInterpreter) {
    let mut command_result = SBCommandReturnObject::new();
    let command = format!("command script import '/home/chega/NW/vscode-lldb/adapter2/codelldb.py'");
    interpreter.handle_command(&command, &mut command_result, false);
    info!("{:?}", command_result);
}

pub fn evaluate(
    interpreter: &SBCommandInterpreter, script: &str, context: &SBExecutionContext,
) -> Result<PythonValue, String> {
    type EvalResult = Result<PythonValue, String>;
    extern "C" fn callback(result_ptr: *mut EvalResult, ty: c_int, data: *const c_void, len: usize) {
        unsafe {
            *result_ptr = match ty {
                1 => {
                    let sbvalue = data as *const SBValue;
                    Ok(PythonValue::SBValue((*sbvalue).clone()))
                }
                2 => {
                    let bytes = slice::from_raw_parts(data as *const u8, len);
                    Ok(PythonValue::String(String::from_utf8_lossy(bytes).into_owned()))
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
        "script codelldb.evaluate('{}', {:#X}, {:#X})",
        script, callback as *mut c_void as usize, &mut eval_result as *mut EvalResult as usize
    );
    let result = interpreter.handle_command_with_context(&command, &context, &mut command_result, false);

    info!("{:?}", command_result);
    info!("{:?}", eval_result);
    eval_result
}

#[derive(Debug)]
pub enum PythonValue {
    SBValue(SBValue),
    String(String),
}
