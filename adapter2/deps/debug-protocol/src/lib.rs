#![allow(non_camel_case_types)]

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

mod generated;

pub use generated::{
    AttachRequestArguments, Breakpoint, BreakpointEventBody, CompletionsArguments, CompletionsResponseBody,
    ContinueArguments, ContinueResponseBody, ContinuedEventBody, DisconnectArguments, EvaluateArguments,
    EvaluateResponseBody, ExitedEventBody, InitializeRequestArguments, ModuleEventBody, NextArguments, OutputEventBody,
    PauseArguments, ScopesArguments, ScopesResponseBody, SetBreakpointsArguments, SetBreakpointsResponseBody,
    SetExceptionBreakpointsArguments, SetFunctionBreakpointsArguments, SetVariableArguments, SetVariableResponseBody,
    Source, SourceArguments, SourceBreakpoint, SourceResponseBody, StackFrame, StackTraceArguments,
    StackTraceResponseBody, StepBackArguments, StepInArguments, StepOutArguments, StoppedEventBody,
    TerminatedEventBody, Thread, ThreadEventBody, ThreadsResponseBody, VariablesArguments, VariablesResponseBody,
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ProtocolMessage {
    #[serde(rename = "request")]
    Request(Request),
    #[serde(rename = "response")]
    Response(Response),
    #[serde(rename = "event")]
    Event(Event),
    #[serde(skip)]
    Unknown(serde_json::Value),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    pub seq: u32,
    #[serde(flatten)]
    pub arguments: RequestArguments,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub request_seq: u32,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(flatten)]
    pub body: Option<ResponseBody>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub seq: u32,
    #[serde(flatten)]
    pub body: EventBody,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", content = "arguments")]
pub enum RequestArguments {
    initialize(InitializeRequestArguments),
    launch(LaunchRequestArguments),
    attach(AttachRequestArguments),
    setBreakpoints(SetBreakpointsArguments),
    setFunctionBreakpoints(SetFunctionBreakpointsArguments),
    setExceptionBreakpoints(SetExceptionBreakpointsArguments),
    configurationDone,
    pause(PauseArguments),
    #[serde(rename = "continue")]
    continue_(ContinueArguments),
    next(NextArguments),
    stepIn(StepInArguments),
    stepOut(StepOutArguments),
    stepBack(StepBackArguments),
    reverseContinue,
    threads,
    stackTrace(StackTraceArguments),
    scopes(ScopesArguments),
    source(SourceArguments),
    variables(VariablesArguments),
    completions(CompletionsArguments),
    evaluate(EvaluateArguments),
    setVariable(SetVariableArguments),
    disconnect(DisconnectArguments),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", content = "body")]
pub enum ResponseBody {
    Async,
    initialize(Capabilities),
    launch,
    attach,
    setBreakpoints(SetBreakpointsResponseBody),
    setFunctionBreakpoints(SetBreakpointsResponseBody),
    setExceptionBreakpoints,
    configurationDone,
    pause,
    #[serde(rename = "continue")]
    continue_(ContinueResponseBody),
    next,
    stepIn,
    stepOut,
    stepBack,
    reverseContinue,
    threads(ThreadsResponseBody),
    stackTrace(StackTraceResponseBody),
    scopes(ScopesResponseBody),
    source(SourceResponseBody),
    variables(VariablesResponseBody),
    completions(CompletionsResponseBody),
    evaluate(EvaluateResponseBody),
    setVariable(SetVariableResponseBody),
    disconnect,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event", content = "body")]
pub enum EventBody {
    initialized,
    output(OutputEventBody),
    breakpoint(BreakpointEventBody),
    module(ModuleEventBody),
    thread(ThreadEventBody),
    stopped(StoppedEventBody),
    continued(ContinuedEventBody),
    exited(ExitedEventBody),
    terminated(TerminatedEventBody),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchRequestArguments {
    #[serde(rename = "noDebug")]
    pub no_debug: Option<bool>,
    pub program: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Capabilities {
    #[serde(rename = "supportsConfigurationDoneRequest")]
    pub supports_configuration_done_request: bool,
    #[serde(rename = "supportsFunctionBreakpoints")]
    pub supports_function_breakpoints: bool,
    #[serde(rename = "supportsConditionalBreakpoints")]
    pub supports_conditional_breakpoints: bool,
    #[serde(rename = "supportsHitConditionalBreakpoints")]
    pub supports_hit_conditional_breakpoints: bool,
    #[serde(rename = "supportsEvaluateForHovers")]
    pub supports_evaluate_for_hovers: bool,
    #[serde(rename = "supportsSetVariable")]
    pub supports_set_variable: bool,
    #[serde(rename = "supportsCompletionsRequest")]
    pub supports_completions_request: bool,
    #[serde(rename = "supportTerminateDebuggee")]
    pub support_terminate_debuggee: bool,
    #[serde(rename = "supportsDelayedStackTraceLoading")]
    pub supports_delayed_stack_trace_loading: bool,
    #[serde(rename = "supportsLogPoints")]
    pub supports_log_points: bool,
}

impl Default for Breakpoint {
    fn default() -> Self {
        Breakpoint {
            id: None,
            verified: false,
            column: None,
            end_column: None,
            line: None,
            end_line: None,
            message: None,
            source: None,
        }
    }
}

impl Default for StackFrame {
    fn default() -> Self {
        StackFrame {
            id: 0,
            name: String::new(),
            source: None,
            line: 0,
            column: 0,
            end_column: None,
            end_line: None,
            module_id: None,
            presentation_hint: None,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &[u8]) {
        let _msg = serde_json::from_slice::<ProtocolMessage>(s).unwrap();
    }

    #[test]
    fn test1() {
        parse(br#"{"command":"initialize","arguments":{"clientID":"vscode","clientName":"Visual Studio Code","adapterID":"lldb","pathFormat":"path","linesStartAt1":true,"columnsStartAt1":true,"supportsVariableType":true,"supportsVariablePaging":true,"supportsRunInTerminalRequest":true,"locale":"en-us"},"type":"request","seq":1}"#);
        parse(br#"{"request_seq":1,"command":"initialize","body":{"supportsDelayedStackTraceLoading":true,"supportsEvaluateForHovers":true,"exceptionBreakpointFilters":[{"filter":"rust_panic","default":true,"label":"Rust: on panic"}],"supportsCompletionsRequest":true,"supportsConditionalBreakpoints":true,"supportsStepBack":false,"supportsConfigurationDoneRequest":true,"supportTerminateDebuggee":true,"supportsLogPoints":true,"supportsFunctionBreakpoints":true,"supportsHitConditionalBreakpoints":true,"supportsSetVariable":true},"type":"response","success":true}"#);
    }

    #[test]
    fn test2() {
        parse(br#"{"command":"launch","arguments":{"type":"lldb","request":"launch","name":"Debug tests in types_lib","args":[],"cwd":"/home/chega/NW/vscode-lldb/debuggee","initCommands":["platform shell echo 'init'"],"env":{"TEST":"folder"},"sourceMap":{"/checkout/src":"/home/chega/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/src"},"program":"/home/chega/NW/vscode-lldb/debuggee/target/debug/types_lib-d6a67ab7ca515c6b","debugServer":41025,"_displaySettings":{"showDisassembly":"always","displayFormat":"auto","dereferencePointers":true,"toggleContainerSummary":false,"containerSummary":true},"__sessionId":"81865613-a1ee-4a66-b449-a94165625fd2"},"type":"request","seq":2}"#);
        parse(br#"{"request_seq":2,"command":"launch","body":null,"type":"response","success":true}"#);
    }

    #[test]
    fn test3() {
        parse(br#"{"type":"event","event":"initialized","seq":0}"#);
        parse(br#"{"body":{"reason":"started","threadId":7537},"type":"event","event":"thread","seq":0}"#);
    }

    #[test]
    fn test4() {
        parse(br#"{"command":"scopes","arguments":{"frameId":1000},"type":"request","seq":12}"#);
        parse(br#"{"request_seq":12,"command":"scopes","body":{"scopes":[{"variablesReference":1001,"name":"Local","expensive":false},{"variablesReference":1002,"name":"Static","expensive":false},{"variablesReference":1003,"name":"Global","expensive":false},{"variablesReference":1004,"name":"Registers","expensive":false}]},"type":"response","success":true}"#);
    }
}
