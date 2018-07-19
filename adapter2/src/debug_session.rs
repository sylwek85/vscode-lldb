use globset;
use regex;
use serde_json;

use std;
use std::boxed::FnBox;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::mem;
use std::option;
use std::path::{self, Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

use futures::prelude::*;
use futures::stream;
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio;
use tokio::prelude::*;
use tokio_threadpool::blocking;

use cancellation::{CancellationSource, CancellationToken};
use debug_protocol::*;
use failure;
use handles::{self, Handle, HandleTree, VPath};
use launch_config::*;
use lldb::*;
use must_initialize::{Initialized, MustInitialize, NotInitialized};

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "Whoops! Something that was supposed to have been initialized, wasn't.")]
    NotInitialized,
    #[fail(display = "{}", _0)]
    SBError(String),
    #[fail(display = "{}", _0)]
    Internal(String),
    #[fail(display = "{}", _0)]
    UserError(String),
}
impl From<option::NoneError> for Error {
    fn from(_: option::NoneError) -> Self {
        Error::NotInitialized
    }
}
impl From<SBError> for Error {
    fn from(sberr: SBError) -> Self {
        Error::SBError(sberr.message().into())
    }
}

type AsyncResponder = FnBox(&mut DebugSessionInner) -> Result<ResponseBody, Error>;

#[derive(Hash, Eq, PartialEq, Debug)]
struct SourceRef(u32);

#[derive(Hash, Eq, PartialEq, Debug)]
enum FileId {
    Filename(String),
    Disassembly(SourceRef),
}

enum BreakpointKind {
    Source {
        file_path: String,
        resolved_line: Option<u32>,
        valid_locations: Vec<BreakpointID>,
    },
    Function,
    Assembly {
        address: u64,
        adapter_data: Vec<u8>,
    },
    Exception,
}

struct BreakpointInfo {
    id: BreakpointID,
    kind: BreakpointKind,
    condition: Option<String>,
    log_message: Option<String>,
    ignore_count: u32,
}

enum VarsScope {
    StackFrame(SBFrame),
    Locals(SBFrame),
    Statics(SBFrame),
    Globals(SBFrame),
    Registers(SBFrame),
    Container(SBValue),
}

struct DebugSessionInner {
    send_message: mpsc::Sender<ProtocolMessage>,
    shutdown: CancellationSource,
    event_listener: SBListener,
    debugger: MustInitialize<SBDebugger>,
    target: MustInitialize<SBTarget>,
    process: MustInitialize<SBProcess>,
    process_launched: bool,
    on_configuration_done: Option<(u32, Box<AsyncResponder>)>,
    line_breakpoints: HashMap<FileId, HashMap<i64, BreakpointID>>,
    fn_breakpoints: HashMap<String, BreakpointID>,
    breakpoints: HashMap<BreakpointID, BreakpointInfo>,
    var_refs: HandleTree<VarsScope>,
    source_map: MustInitialize<Vec<(regex::Regex, Option<String>)>>,
    source_map_cache: HashMap<(String, String), Option<String>>,
}

pub struct DebugSession {
    inner: Arc<Mutex<DebugSessionInner>>,
    sender_in: mpsc::Sender<ProtocolMessage>,
    receiver_out: mpsc::Receiver<ProtocolMessage>,
    shutdown_token: CancellationToken,
}

impl DebugSession {
    pub fn new() -> Self {
        let (sender_in, receiver_in) = mpsc::channel(10);
        let (sender_out, receiver_out) = mpsc::channel(10);
        let shutdown = CancellationSource::new();
        let shutdown_token = shutdown.cancellation_token();

        let inner = DebugSessionInner {
            send_message: sender_out,
            shutdown: shutdown,
            debugger: NotInitialized,
            target: NotInitialized,
            process: NotInitialized,
            process_launched: false,
            event_listener: SBListener::new_with_name("DebugSession"),
            on_configuration_done: None,
            line_breakpoints: HashMap::new(),
            fn_breakpoints: HashMap::new(),
            breakpoints: HashMap::new(),
            var_refs: HandleTree::new(),
            source_map: NotInitialized,
            source_map_cache: HashMap::new(),
        };
        let inner = Arc::new(Mutex::new(inner));

        // Dispatch incoming requests to inner.handle_message()
        let inner2 = inner.clone();
        let sink_to_inner = receiver_in
            .for_each(move |msg| {
                inner2.lock().unwrap().handle_message(msg);
                Ok(())
            })
            .then(|r| {
                info!("### sink_to_inner resolved");
                r
            });
        tokio::spawn(sink_to_inner);

        // Create a thread listening on inner's event_listener
        let (mut sender, mut receiver) = mpsc::channel(10);
        let listener = inner.lock().unwrap().event_listener.clone();
        let token2 = shutdown_token.clone();
        thread::spawn(move || {
            let mut event = SBEvent::new();
            while sender.poll_ready().is_ok() && !token2.is_cancelled() {
                if listener.wait_for_event(1, &mut event) {
                    if sender.try_send(event).is_err() {
                        break;
                    }
                    event = SBEvent::new();
                }
            }
        });
        // Dispatch incoming events to inner.handle_debug_event()
        let inner2 = inner.clone();
        let event_listener_to_inner = receiver
            .for_each(move |event| {
                inner2.lock().unwrap().handle_debug_event(event);
                Ok(())
            })
            .then(|r| {
                info!("### event_listener_to_inner resolved");
                r
            });
        tokio::spawn(event_listener_to_inner);

        DebugSession {
            inner,
            sender_in,
            receiver_out,
            shutdown_token,
        }
    }
}

impl Stream for DebugSession {
    type Item = ProtocolMessage;
    type Error = ();
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.receiver_out.poll() {
            Ok(Async::NotReady) if self.shutdown_token.is_cancelled() => {
                error!("Stream::poll after shutdown");
                Ok(Async::Ready(None))
            }
            Ok(r) => Ok(r),
            Err(e) => Err(e),
        }
    }
}

impl Sink for DebugSession {
    type SinkItem = ProtocolMessage;
    type SinkError = ();
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.shutdown_token.is_cancelled() {
            error!("Sink::start_send after shutdown");
            Err(())
        } else {
            self.sender_in.start_send(item).map_err(|err| panic!("{:?}", err))
        }
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        if self.shutdown_token.is_cancelled() {
            error!("Sink::poll_complete after shutdown");
            Err(())
        } else {
            self.sender_in.poll_complete().map_err(|err| panic!("{:?}", err))
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////////

unsafe impl Send for DebugSession {}
unsafe impl Send for DebugSessionInner {}

impl Drop for DebugSession {
    fn drop(&mut self) {
        info!("### Dropping DebugSession");
    }
}

impl Drop for DebugSessionInner {
    fn drop(&mut self) {
        info!("### Dropping DebugSessionInner");
    }
}

impl DebugSessionInner {
    fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::Request(request) => self.handle_request(request),
            ProtocolMessage::Response(response) => self.handle_response(response),
            ProtocolMessage::Event(event) => error!("No handler for event message: {:?}", event),
            ProtocolMessage::Unknown(unknown) => error!("Received unknown message: {:?}", unknown),
        };
    }

    fn handle_response(&mut self, response: Response) {}

    fn handle_request(&mut self, request: Request) {
        //info!("Received message: {:?}", request);
        let result = match request.arguments {
            RequestArguments::initialize(args) => self.handle_initialize(args).map(|r| ResponseBody::initialize(r)),
            RequestArguments::setBreakpoints(args) => self
                .handle_set_breakpoints(args)
                .map(|r| ResponseBody::setBreakpoints(r)),
            RequestArguments::setFunctionBreakpoints(args) => self
                .handle_set_function_breakpoints(args)
                .map(|r| ResponseBody::setFunctionBreakpoints(r)),
            RequestArguments::setExceptionBreakpoints(args) => self
                .handle_set_exception_breakpoints(args)
                .map(|r| ResponseBody::setExceptionBreakpoints),
            RequestArguments::launch(args) => {
                match self.handle_launch(args) {
                    Ok(responder) => {
                        self.on_configuration_done = Some((request.seq, responder));
                        return; // launch responds asynchronously
                    }
                    Err(err) => Err(err),
                }
            }
            RequestArguments::attach(args) => {
                match self.handle_attach(args) {
                    Ok(responder) => {
                        self.on_configuration_done = Some((request.seq, responder));
                        return; // attach responds asynchronously
                    }
                    Err(err) => Err(err),
                }
            }
            RequestArguments::configurationDone => self
                .handle_configuration_done()
                .map(|r| ResponseBody::configurationDone),
            RequestArguments::threads => self // br
                .handle_threads()
                .map(|r| ResponseBody::threads(r)),
            RequestArguments::stackTrace(args) => self // br
                .handle_stack_trace(args)
                .map(|r| ResponseBody::stackTrace(r)),
            RequestArguments::scopes(args) => self // br
                .handle_scopes(args)
                .map(|r| ResponseBody::scopes(r)),
            RequestArguments::variables(args) => self // br
                .handle_variables(args)
                .map(|r| ResponseBody::variables(r)),
            RequestArguments::continue_(args) => self // br
                .handle_continue(args)
                .map(|r| ResponseBody::continue_(r)),
            RequestArguments::next(args) => self // br
                .handle_next(args)
                .map(|r| ResponseBody::next),
            RequestArguments::stepIn(args) => self // br
                .handle_step_in(args)
                .map(|r| ResponseBody::stepIn),
            RequestArguments::stepOut(args) => self // br
                .handle_step_out(args)
                .map(|r| ResponseBody::stepOut),
            RequestArguments::disconnect(args) => self // br
                .handle_disconnect(args)
                .map(|_| ResponseBody::disconnect),
            _ => {
                error!("No handler for request message: {:?}", request);
                Err(Error::Internal("Not implemented.".into()))
            }
        };
        self.send_response(request.seq, result);
    }

    fn send_response(&mut self, request_seq: u32, result: Result<ResponseBody, Error>) {
        let response = match result {
            Ok(body) => ProtocolMessage::Response(Response {
                request_seq: request_seq,
                success: true,
                message: None,
                body: Some(body),
            }),
            Err(err) => ProtocolMessage::Response(Response {
                request_seq: request_seq,
                success: false,
                body: None,
                message: Some(format!("{}", err)),
            }),
        };
        self.send_message
            .try_send(response)
            .map_err(|err| panic!("Could not send response: {}", err));
    }

    fn send_event(&mut self, event_body: EventBody) {
        let event = ProtocolMessage::Event(Event {
            seq: 0,
            body: event_body,
        });
        self.send_message
            .try_send(event)
            .map_err(|err| panic!("Could not send event: {}", err));
    }

    fn handle_initialize(&mut self, args: InitializeRequestArguments) -> Result<Capabilities, Error> {
        self.debugger = Initialized(SBDebugger::create(false));
        self.debugger.set_async(true);

        let caps = Capabilities {
            supports_configuration_done_request: true,
            supports_evaluate_for_hovers: true,
            supports_function_breakpoints: true,
            supports_conditional_breakpoints: true,
            supports_hit_conditional_breakpoints: true,
            supports_set_variable: true,
            supports_completions_request: true,
            supports_delayed_stack_trace_loading: true,
            support_terminate_debuggee: true,
            supports_log_points: true,
        };
        Ok(caps)
    }

    fn handle_set_breakpoints(&mut self, args: SetBreakpointsArguments) -> Result<SetBreakpointsResponseBody, Error> {
        let file_id = FileId::Filename(args.source.path.as_ref()?.clone());
        let file_bps = self.line_breakpoints.remove(&file_id).unwrap_or_default();
        let breakpoints =
            self.set_source_breakpoints(file_bps, &args.breakpoints.as_ref()?, args.source.path.as_ref()?);
        let response = SetBreakpointsResponseBody { breakpoints };
        Ok(response)
    }

    fn set_source_breakpoints(
        &mut self, mut existing_bps: HashMap<i64, BreakpointID>, req_bps: &[SourceBreakpoint], file_path: &str,
    ) -> Vec<Breakpoint> {
        let mut breakpoints = vec![];
        for req in req_bps {
            let mut bp_resp = Breakpoint { ..Default::default() };

            let bp = if let Some(bp_id) = existing_bps.get(&req.line).cloned() {
                let bp = self.target.find_breakpoint_by_id(bp_id);
                bp_resp.id = Some(bp.id() as i64);
                bp_resp.verified = true;
                bp
            } else {
                let file_name = Path::new(file_path).file_name().unwrap().to_str().unwrap();
                let bp = self.target.breakpoint_create_by_location(file_name, req.line as u32);

                let mut bp_info = BreakpointInfo {
                    id: bp.id(),
                    kind: BreakpointKind::Source {
                        file_path: file_path.to_owned(),
                        resolved_line: None,
                        valid_locations: vec![],
                    },
                    condition: None,
                    log_message: None,
                    ignore_count: 0,
                };
                existing_bps.insert(req.line, bp_info.id);
                bp_resp.id = Some(bp_info.id as i64);

                // Filter locations on full source file path
                for bp_loc in bp.locations() {
                    if !self.is_valid_source_bp_location(&bp_loc, &mut bp_info) {
                        bp_loc.set_enabled(false);
                        //info!("Disabled BP location {}", bp_loc);
                    }
                }
                match bp_info.kind {
                    BreakpointKind::Source { resolved_line, .. } => {
                        if let Some(line) = resolved_line {
                            bp_resp.verified = true;
                            bp_resp.line = Some(line as i64);
                            bp_resp.source = Some(Source {
                                name: Some(file_name.to_owned()),
                                path: Some(file_path.to_owned()),
                                ..Default::default()
                            })
                        }
                    }
                    _ => unreachable!(),
                }
                bp
            };
            // TODO: set condition, etc
            breakpoints.push(bp_resp);
        }
        breakpoints
    }

    fn is_valid_source_bp_location(&mut self, bp_loc: &SBBreakpointLocation, bp_info: &mut BreakpointInfo) -> bool {
        // TODO
        true
    }

    fn handle_set_function_breakpoints(
        &mut self, args: SetFunctionBreakpointsArguments,
    ) -> Result<SetBreakpointsResponseBody, Error> {
        let response = SetBreakpointsResponseBody { breakpoints: vec![] };
        Ok(response)
    }

    fn handle_set_exception_breakpoints(&mut self, args: SetExceptionBreakpointsArguments) -> Result<(), Error> {
        Ok(())
    }

    fn handle_launch(&mut self, args: LaunchRequestArguments) -> Result<Box<AsyncResponder>, Error> {
        self.target = Initialized(self.debugger.create_target(&args.program, None, None, false)?);
        self.send_event(EventBody::initialized);
        Ok(Box::new(move |s: &mut DebugSessionInner| s.complete_launch(args)))
    }

    fn complete_launch(&mut self, args: LaunchRequestArguments) -> Result<ResponseBody, Error> {
        let mut launch_info = SBLaunchInfo::new();
        match serde_json::from_value::<LaunchConfig>(serde_json::Value::Object(args.custom)) {
            Err(err) => {
                return Err(Error::UserError(format!("{}", err)));
            }
            Ok(launch_config) => {
                if let Some(args) = launch_config.args {
                    launch_info.set_arguments(args.iter().map(|a| a.as_ref()), false);
                }
                if let Some(env) = launch_config.env {
                    let env: Vec<String> = env.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                    launch_info.set_environment_entries(env.iter().map(|s| s.as_ref()), true);
                }
                if let Some(cwd) = launch_config.cwd {
                    launch_info.set_working_directory(&cwd);
                }
                if let Some(stop_on_entry) = launch_config.stop_on_entry {
                    launch_info.set_launch_flags(launch_info.launch_flags() | SBLaunchInfo::eLaunchFlagStopAtEntry);
                }
                if let Some(source_map) = launch_config.source_map {
                    self.source_map = Initialized(build_source_map(&source_map)?);
                }
            }
        }
        launch_info.set_listener(&self.event_listener);
        self.process = Initialized(self.target.launch(launch_info)?);
        self.process_launched = true;
        Ok(ResponseBody::launch)
    }

    fn handle_attach(&mut self, args: AttachRequestArguments) -> Result<Box<AsyncResponder>, Error> {
        unimplemented!()
    }

    fn handle_configuration_done(&mut self) -> Result<(), Error> {
        if let Some((request_seq, mut responder)) = self.on_configuration_done.take() {
            let result = responder.call_box((self,));
            self.send_response(request_seq, result);
        }
        Ok(())
    }

    fn handle_threads(&mut self) -> Result<ThreadsResponseBody, Error> {
        let mut response = ThreadsResponseBody { threads: vec![] };
        for thread in self.process.threads() {
            response.threads.push(Thread {
                id: thread.thread_id() as i64,
                name: format!("{}: tid={}", thread.index_id(), thread.thread_id()),
            });
        }
        Ok(response)
    }

    fn handle_stack_trace(&mut self, args: StackTraceArguments) -> Result<StackTraceResponseBody, Error> {
        let thread = self
            .process
            .thread_by_id(args.thread_id as ThreadID)
            .expect("Invalid thread id");

        let start_frame = args.start_frame.unwrap_or(0);
        let levels = args.levels.unwrap_or(std::i64::MAX);

        let mut stack_frames = vec![];
        for i in start_frame..(start_frame + levels) {
            let frame = thread.frame_at_index(i as u32);
            if !frame.is_valid() {
                break;
            }

            let handle = self.var_refs.create(None, "", VarsScope::StackFrame(frame.clone()));
            let mut stack_frame: StackFrame = Default::default();

            stack_frame.id = handle.get() as i64;
            let pc_address = frame.pc_address();
            stack_frame.name = if let Some(name) = frame.function_name() {
                name.to_owned()
            } else {
                format!("{:X}", pc_address.file_address())
            };
            if let Some(le) = frame.line_entry() {
                let fs = le.file_spec();
                if let Some(local_path) = self.map_filespec_to_local(&fs) {
                    stack_frame.line = le.line() as i64;
                    stack_frame.column = le.column() as i64;
                    stack_frame.source = Some(Source {
                        name: Some(fs.filename().to_owned()),
                        path: Some(fs.path()),
                        ..Default::default()
                    });
                }
            }
            stack_frames.push(stack_frame);
        }

        Ok(StackTraceResponseBody {
            stack_frames: stack_frames,
            total_frames: Some(thread.num_frames() as i64),
        })
    }

    fn handle_scopes(&mut self, args: ScopesArguments) -> Result<ScopesResponseBody, Error> {
        let frame_id = Handle::new(args.frame_id as u32).unwrap();
        if let Some(VarsScope::StackFrame(frame)) = self.var_refs.get(frame_id) {
            let frame = frame.clone();
            let locals_handle = self
                .var_refs
                .create(Some(frame_id), "[locs]", VarsScope::Locals(frame.clone()));
            let locals = Scope {
                name: "Local".into(),
                variables_reference: locals_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let statics_handle = self
                .var_refs
                .create(Some(frame_id), "[stat]", VarsScope::Statics(frame.clone()));
            let statics = Scope {
                name: "Static".into(),
                variables_reference: statics_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let globals_handle = self
                .var_refs
                .create(Some(frame_id), "[glob]", VarsScope::Globals(frame.clone()));
            let globals = Scope {
                name: "Global".into(),
                variables_reference: globals_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let registers_handle = self
                .var_refs
                .create(Some(frame_id), "[regs]", VarsScope::Registers(frame));
            let registers = Scope {
                name: "Registers".into(),
                variables_reference: registers_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            Ok(ScopesResponseBody {
                scopes: vec![locals, statics, globals, registers],
            })
        } else {
            Err(Error::Internal(format!("Invalid frame reference: {}", args.frame_id)))
        }
    }

    fn handle_variables(&mut self, args: VariablesArguments) -> Result<VariablesResponseBody, Error> {
        let container_handle = Handle::new(args.variables_reference as u32).unwrap();
        if let Some((container, container_vpath)) = self.var_refs.get_with_vpath(container_handle) {
            let variables = match container {
                VarsScope::Locals(frame) => {
                    let ret_val = frame.thread().stop_return_value();
                    let variables = frame.variables(&VariableOptions {
                        arguments: true,
                        locals: true,
                        statics: false,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = ret_val.into_iter().chain(variables.iter());
                    self.convert_scope_values(&mut vars_iter, Some(container_handle))
                }
                VarsScope::Statics(frame) => {
                    let variables = frame.variables(&VariableOptions {
                        arguments: false,
                        locals: false,
                        statics: true,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = variables.iter().filter(|v| v.value_type() != ValueType::VariableStatic);
                    self.convert_scope_values(&mut vars_iter, Some(container_handle))
                }
                VarsScope::Globals(frame) => {
                    let variables = frame.variables(&VariableOptions {
                        arguments: false,
                        locals: false,
                        statics: true,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = variables.iter(); //.filter(|v| v.value_type() != ValueType::VariableGlobal);
                    self.convert_scope_values(&mut vars_iter, Some(container_handle))
                }
                VarsScope::Registers(frame) => {
                    let list = frame.registers();
                    let mut vars_iter = list.iter();
                    self.convert_scope_values(&mut vars_iter, Some(container_handle))
                }
                VarsScope::Container(v) => {
                    let v = v.clone();
                    let mut vars_iter = v.children();
                    self.convert_scope_values(&mut vars_iter, Some(container_handle))
                }
                _ => vec![],
            };
            Ok(VariablesResponseBody { variables: variables })
        } else {
            Err(Error::Internal(format!(
                "Invalid variabes reference: {}",
                container_handle
            )))
        }
    }

    fn convert_scope_values(
        &mut self, vars_iter: &mut Iterator<Item = SBValue>, container_handle: Option<Handle>,
    ) -> Vec<Variable> {
        let mut variables = vec![];
        for var in vars_iter {
            if let Some(name) = var.name() {
                let dtype = var.type_name();
                let handle = self.get_var_handle(container_handle, name, &var);
                let value = self.get_var_value_str(&var, container_handle.is_some());
                variables.push(Variable {
                    name: name.to_owned(),
                    value: value,
                    type_: dtype.map(|v| v.to_owned()),
                    variables_reference: handles::to_i64(handle),
                    ..Default::default()
                });
            } else {
                error!(
                    "Dropped value {:?} {}",
                    var.type_name(),
                    self.get_var_value_str(&var, false)
                );
            }
        }
        variables
    }

    // Generate a handle for a variable.
    fn get_var_handle(&mut self, parent_handle: Option<Handle>, key: &str, var: &SBValue) -> Option<Handle> {
        if var.num_children() > 0 || var.is_synthetic() {
            Some(
                self.var_refs
                    .create(parent_handle, key, VarsScope::Container(var.clone())),
            )
        } else {
            None
        }
    }

    // Get a displayable string from a SBValue
    fn get_var_value_str(&self, var: &SBValue, is_container: bool) -> String {
        let mut value = None;
        // TODO: formats
        // TODO: pointers
        if value.is_none() {
            value = var.value().map(|s| s.to_string_lossy().into_owned());
            if value.is_none() {
                value = var.summary().map(|s| s.to_string_lossy().into_owned());
            }
        }

        let value_str = match value {
            Some(value) => value,
            None => {
                if is_container {
                    // TODO: Container summary
                    "{...}".to_owned()
                } else {
                    "<not available>".to_owned()
                }
            }
        };
        // TODO: encoding
        value_str
    }

    fn handle_pause(&mut self, args: PauseArguments) -> Result<(), Error> {
        let error = self.process.stop();
        if error.success() {
            Ok(())
        } else {
            Err(Error::UserError(error.message().into()))
        }
    }

    fn handle_continue(&mut self, args: ContinueArguments) -> Result<ContinueResponseBody, Error> {
        let error = self.process.resume();
        if error.success() {
            Ok(ContinueResponseBody {
                all_threads_continued: Some(true),
            })
        } else {
            Err(Error::UserError(error.message().into()))
        }
    }

    fn handle_next(&mut self, args: NextArguments) -> Result<(), Error> {
        self.before_resume();
        let thread = self.process.thread_by_id(args.thread_id as ThreadID)?;
        thread.step_over();
        Ok(())
    }

    fn handle_step_in(&mut self, args: StepInArguments) -> Result<(), Error> {
        self.before_resume();
        let thread = self.process.thread_by_id(args.thread_id as ThreadID)?;
        let error = thread.step_into();
        Ok(())
    }

    fn handle_step_out(&mut self, args: StepOutArguments) -> Result<(), Error> {
        self.before_resume();
        let thread = self.process.thread_by_id(args.thread_id as ThreadID)?;
        thread.step_out();
        Ok(())
    }

    fn handle_disconnect(&mut self, args: DisconnectArguments) -> Result<(), Error> {
        // TODO: exitCommands
        let terminate = args.terminate_debuggee.unwrap_or(self.process_launched);
        if terminate {
            self.process.kill();
        } else {
            self.process.detach();
        }
        self.shutdown.request_cancellation();
        Ok(())
    }

    fn before_resume(&mut self) {
        self.var_refs.reset();
    }

    fn handle_debug_event(&mut self, event: SBEvent) {
        debug!("Debug event: {}", event);
        if let Some(process_event) = event.as_process_event() {
            self.handle_process_event(&process_event);
        } else if let Some(bp_event) = event.as_breakpoint_event() {
            //self.notify_breakpoint(event);
        }
    }

    fn handle_process_event(&mut self, process_event: &SBProcessEvent) {
        let flags = process_event.as_event().flags();
        if flags & SBProcess::eBroadcastBitStateChanged != 0 {
            match process_event.process_state() {
                ProcessState::Running => self.send_event(EventBody::continued(ContinuedEventBody {
                    all_threads_continued: Some(true),
                    thread_id: 0,
                })),
                ProcessState::Stopped if !process_event.restarted() => self.notify_process_stopped(&process_event),
                ProcessState::Crashed => self.notify_process_stopped(&process_event),
                ProcessState::Exited => {
                    let exit_code = self.process.exit_status() as i64;
                    self.send_event(EventBody::exited(ExitedEventBody { exit_code }));
                    self.send_event(EventBody::terminated(TerminatedEventBody { restart: None }));
                }
                ProcessState::Detached => self.send_event(EventBody::terminated(TerminatedEventBody { restart: None })),
                _ => (),
            }
        }
    }

    fn notify_process_stopped(&mut self, event: &SBProcessEvent) {
        // Find thread that has caused this stop
        let mut stopped_thread = None;
        // Check the currently selected thread first
        let selected_thread = self.process.selected_thread();
        stopped_thread = match selected_thread.stop_reason() {
            StopReason::Invalid | StopReason::None => None,
            _ => Some(selected_thread),
        };
        // Fall back to scanning all threads in the process
        if stopped_thread.is_none() {
            for thread in self.process.threads() {
                match thread.stop_reason() {
                    StopReason::Invalid | StopReason::None => (),
                    _ => {
                        self.process.set_selected_thread(&thread);
                        stopped_thread = Some(thread);
                        break;
                    }
                }
            }
        }
        // Analyze stop reason
        let (stop_reason_str, description) = if let Some(ref stopped_thread) = stopped_thread {
            let stop_reason = stopped_thread.stop_reason();
            match stop_reason {
                StopReason::Breakpoint => ("breakpoint", None),
                StopReason::Trace | StopReason::PlanComplete => ("step", None),
                _ => {
                    // Print stop details for these types
                    let description = Some(stopped_thread.stop_description());
                    match stop_reason {
                        StopReason::Watchpoint => ("watchpoint", description),
                        StopReason::Signal => ("signal", description),
                        StopReason::Exception => ("exception", description),
                        _ => ("unknown", description),
                    }
                }
            }
        } else {
            ("unknown", None)
        };

        self.send_event(EventBody::stopped(StoppedEventBody {
            all_threads_stopped: Some(true),
            description: None,
            preserve_focus_hint: None,
            reason: stop_reason_str.to_owned(),
            text: description,
            thread_id: stopped_thread.map(|t| t.thread_id() as i64),
        }));
    }

    fn map_filespec_to_local(&mut self, filespec: &SBFileSpec) -> Option<String> {
        if !filespec.is_valid() {
            return None;
        } else {
            Some(normalize_path(&filespec.path()).into())
        }
    }

    // def map_filespec_to_local(self, filespec):
    //     if not filespec.IsValid():
    //         return None
    //     key = (filespec.GetDirectory(), filespec.GetFilename())
    //     local_path = self.filespec_cache.get(key, MISSING)
    //     if local_path is MISSING:
    //         local_path = self.map_filespec_to_local_uncached(filespec)
    //         log.info('Mapped "%s" to "%s"', filespec, local_path)
    //         if self.suppress_missing_sources and not os.path.isfile(local_path):
    //             local_path = None
    //         self.filespec_cache[key] = local_path
    //     return local_path

    fn map_filespec_to_local_uncached(&mut self, filespec: &SBFileSpec) -> Option<String> {
        if !filespec.is_valid() {
            return None;
        }
        let normalized = normalize_path(&filespec.path());
        for (remote_prefix, local_prefix) in self.source_map.iter() {
            if let Some(captures) = remote_prefix.captures(&normalized) {
                return match local_prefix {
                    Some(prefix) => {
                        let match_len = captures.get(1).unwrap().start();
                        let result = normalize_path(&format!("{}{}", prefix, &normalized[match_len..]));
                        Some(result)
                    }
                    None => None,
                };
            }
        }
        unimplemented!()
    }
}

fn normalize_path(path: &str) -> String {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => (),
            Component::Normal(comp) => normalized.push(comp),
            Component::CurDir => (),
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }
    normalized.to_str().unwrap().into()
}

fn build_source_map(
    source_map: &HashMap<String, Option<String>>,
) -> Result<Vec<(regex::Regex, Option<String>)>, Error> {
    let mut compiled_source_map = vec![];
    for (remote, local) in source_map {
        let glob = match globset::Glob::new(remote) {
            Ok(glob) => glob,
            Err(err) => return Err(Error::UserError(format!("Invalid glob pattern: {}", remote))),
        };
        let regex = regex::Regex::new(&format!("({}).*", glob.regex())).unwrap(); // TODO: use ?
        compiled_source_map.push((regex, local.clone()));
    }
    Ok(compiled_source_map)
}

#[test]
fn test_source_map() {
    let mut source_map = HashMap::new();
    source_map.insert("/foo/bar/*".to_owned(), Some("/hren".to_owned()));
    let compiled_source_map = build_source_map(&source_map);
}
