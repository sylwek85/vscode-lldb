#![allow(non_camel_case_types)]

pub use debugserver_types::{
    AttachRequestArguments, BreakpointEventBody, Capabilities, CompletionsArguments, CompletionsResponseBody,
    ConfigurationDoneArguments, ContinueArguments, ContinueResponseBody, ContinuedEventBody, DisconnectArguments,
    EvaluateArguments, EvaluateResponseBody, ExitedEventBody, InitializeRequestArguments, ModuleEventBody,
    NextArguments, OutputEventBody, PauseArguments, ScopesArguments, ScopesResponseBody, SetBreakpointsArguments,
    SetBreakpointsResponseBody, SetExceptionBreakpointsArguments, SetFunctionBreakpointsArguments,
    SetVariableArguments, SetVariableResponseBody, SourceArguments, SourceResponseBody, StackTraceArguments,
    StackTraceResponseBody, StepBackArguments, StepInArguments, StepOutArguments, StoppedEventBody,
    TerminatedEventBody, ThreadEventBody, ThreadsResponseBody, VariablesArguments, VariablesResponseBody,
};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ProtocolMessage {
    request(Request),
    response(Response),
    event(Event),
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
    pub message: Option<String>,
    #[serde(flatten)]
    pub body: ResponseBody,
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
    configurationDone(ConfigurationDoneArguments),
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
    setExceptionBreakpoints(SetBreakpointsResponseBody),
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
    pub no_debug: bool,
    pub program: String,
}

////////////////////////////////////////////////////////////////////////////////////

fn parse(s: &[u8]) {
    let msg = serde_json::from_slice::<ProtocolMessage>(s).unwrap();
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
