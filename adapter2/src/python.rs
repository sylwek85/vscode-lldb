use std::os::raw::c_void;
use lldb::*;

fn initialize(interpreter: &SBCommandInterpreter) {
    let command = format!("command script import '/home/chega/NW/vscode-lldb/adapter2/codelldb.py'");
    interpreter.handle_command(&command, &mut result, false);
}

fn invoke_in_frame<T, F>(&mut self, script: &str, frame: Option<&SBFrame>, closure: F)
where
    F: FnOnce(T),
{
    extern "C" fn callback(result: *mut result, closure: *mut F) {
        (*closure)(result)
    }

    let context = match frame {
        Some(frame) => SBExecutionContext::from_frame(&frame),
        None => match self.process {
            Initialized(ref process) => SBExecutionContext::from_process(&process),
            NotInitialized => SBExecutionContext::new(),
        },
    };
    let mut result = SBCommandReturnObject::new();

    let command = format!("script codelldb.invoke({}, {:#X}",
        script, callback as *mut c_void, &mut closure as *mut c_void);
    let result = interpreter.handle_command_with_context(command, &context, &mut result, false);
}

