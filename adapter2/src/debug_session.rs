use globset;
use regex;
use serde_json;

use std;
use std::borrow::Cow;
use std::boxed::FnBox;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::mem;
use std::option;
use std::path::{self, Component, Path, PathBuf};
use std::rc::Rc;
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;

use futures::prelude::*;
use futures::stream;
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio;
use tokio::prelude::*;
use tokio_threadpool::blocking;

use crate::cancellation::{CancellationSource, CancellationToken};
use crate::debug_protocol::*;
use crate::disassembly;
use crate::error::Error;
use crate::handles::{self, Handle, HandleTree};
use crate::must_initialize::{Initialized, MustInitialize, NotInitialized};
use crate::python;
use crate::source_map;
use lldb::*;

type AsyncResponder = FnBox(&mut DebugSessionInner) -> Result<ResponseBody, Error>;

#[derive(Hash, Eq, PartialEq, Debug)]
enum FileId {
    Filename(String),
    Disassembly(Handle),
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

enum Container {
    StackFrame(SBFrame),
    Locals(SBFrame),
    Statics(SBFrame),
    Globals(SBFrame),
    Registers(SBFrame),
    Container(SBValue),
}

enum ExprType {
    Native,
    Python,
    Simple,
}

#[derive(Debug)]
pub enum Evaluated {
    SBValue(SBValue),
    String(String),
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
    source_breakpoints: HashMap<FileId, HashMap<i64, BreakpointID>>,
    fn_breakpoints: HashMap<String, BreakpointID>,
    breakpoints: HashMap<BreakpointID, BreakpointInfo>,
    var_refs: HandleTree<Container>,
    disassembly: MustInitialize<disassembly::AddressSpace>,
    known_threads: HashSet<ThreadID>,
    source_map: source_map::SourceMap,
    source_map_cache: HashMap<(Cow<'static, str>, Cow<'static, str>), Option<Rc<String>>>,
    show_disassembly: Option<bool>,
    suppress_missing_files: bool,
    deref_pointers: bool,
    container_summary: bool,
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
            source_breakpoints: HashMap::new(),
            fn_breakpoints: HashMap::new(),
            breakpoints: HashMap::new(),
            var_refs: HandleTree::new(),
            disassembly: NotInitialized,
            known_threads: HashSet::new(),
            source_map: source_map::SourceMap::empty(),
            source_map_cache: HashMap::new(),
            show_disassembly: None,
            suppress_missing_files: true,
            deref_pointers: true,
            container_summary: true,
        };
        let inner = Arc::new(Mutex::new(inner));

        // Dispatch incoming requests to inner.handle_message()
        let inner2 = inner.clone();
        let sink_to_inner = receiver_in
            .for_each(move |msg: ProtocolMessage| {
                let inner2 = inner2.clone();
                future::poll_fn(move || {
                    let msg = msg.clone();
                    blocking(|| {
                        inner2.lock().unwrap().handle_message(msg);
                    })
                }).map_err(|_| ())
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
        #[cfg_attr(rustfmt, rustfmt_skip)]
        let result = match request.arguments {
            RequestArguments::initialize(args) =>
                self.handle_initialize(args)
                    .map(|r| ResponseBody::initialize(r)),
            RequestArguments::setBreakpoints(args) =>
                self.handle_set_breakpoints(args)
                    .map(|r| ResponseBody::setBreakpoints(r)),
            RequestArguments::setFunctionBreakpoints(args) =>
                self.handle_set_function_breakpoints(args)
                    .map(|r| ResponseBody::setFunctionBreakpoints(r)),
            RequestArguments::setExceptionBreakpoints(args) =>
                self.handle_set_exception_breakpoints(args)
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
            RequestArguments::configurationDone =>
                self.handle_configuration_done()
                    .map(|r| ResponseBody::configurationDone),
            RequestArguments::threads =>
                self.handle_threads()
                    .map(|r| ResponseBody::threads(r)),
            RequestArguments::stackTrace(args) =>
                self.handle_stack_trace(args)
                    .map(|r| ResponseBody::stackTrace(r)),
            RequestArguments::scopes(args) =>
                self.handle_scopes(args)
                    .map(|r| ResponseBody::scopes(r)),
            RequestArguments::variables(args) =>
                self.handle_variables(args)
                    .map(|r| ResponseBody::variables(r)),
            RequestArguments::evaluate(args) =>
                self.handle_evaluate(args)
                    .map(|r| ResponseBody::evaluate(r)),
            RequestArguments::pause(args) =>
                self.handle_pause(args)
                    .map(|_| ResponseBody::pause),
            RequestArguments::continue_(args) =>
                self.handle_continue(args)
                    .map(|r| ResponseBody::continue_(r)),
            RequestArguments::next(args) =>
                self.handle_next(args)
                    .map(|r| ResponseBody::next),
            RequestArguments::stepIn(args) =>
                self.handle_step_in(args)
                    .map(|r| ResponseBody::stepIn),
            RequestArguments::stepOut(args) =>
                self.handle_step_out(args)
                    .map(|r| ResponseBody::stepOut),
            RequestArguments::source(args) =>
                self.handle_source(args)
                    .map(|r| ResponseBody::source(r)),
            RequestArguments::disconnect(args) =>
                self.handle_disconnect(args)
                    .map(|_| ResponseBody::disconnect),
            RequestArguments::displaySettings(args) =>
                self.handle_display_settings(args)
                    .map(|_| ResponseBody::displaySettings),
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
        python::initialize(&self.debugger.command_interpreter());

        let caps = Capabilities {
            supports_configuration_done_request: true,
            supports_evaluate_for_hovers: false, // TODO
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

        let requested_bps = args.breakpoints.as_ref().unwrap();
        let mut old_existing_bps = self.source_breakpoints.remove(&file_id).unwrap_or_default();

        let mut existing_bps = HashMap::new();
        for (line, bp_id) in old_existing_bps.drain() {
            if !requested_bps.iter().any(|rbp| rbp.line == line) {
                self.target.breakpoint_delete(bp_id);
                self.breakpoints.remove(&bp_id);
            } else {
                existing_bps.insert(line, bp_id);
            }
        }

        let breakpoints = self.set_source_breakpoints(
            &mut existing_bps,
            &args.breakpoints.as_ref()?,
            args.source.path.as_ref()?,
        );

        self.source_breakpoints.insert(file_id, existing_bps);

        let response = SetBreakpointsResponseBody { breakpoints };
        Ok(response)
    }

    fn set_source_breakpoints(
        &mut self, existing_bps: &mut HashMap<i64, BreakpointID>, requested_bps: &[SourceBreakpoint], file_path: &str,
    ) -> Vec<Breakpoint> {
        let mut breakpoints = vec![];
        for req in requested_bps {
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
                                adapter_data: Some(json!(bp_info.id)),
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
        if let Some(le) = bp_loc.address().line_entry() {
            if let BreakpointKind::Source {
                ref mut resolved_line, ..
            } = bp_info.kind
            {
                *resolved_line = Some(le.line());
            }
        }
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
        self.disassembly = Initialized(disassembly::AddressSpace::new(&self.target));
        self.send_event(EventBody::initialized);
        Ok(Box::new(move |s: &mut DebugSessionInner| s.complete_launch(args)))
    }

    fn complete_launch(&mut self, args: LaunchRequestArguments) -> Result<ResponseBody, Error> {
        let mut launch_info = SBLaunchInfo::new();
        if let Some(ds) = args.display_settings {
            self.update_display_settings(&ds);
        }
        if let Some(args) = args.args {
            launch_info.set_arguments(args.iter().map(|a| a.as_ref()), false);
        }
        if let Some(env) = args.env {
            let env: Vec<String> = env.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
            launch_info.set_environment_entries(env.iter().map(|s| s.as_ref()), true);
        }
        if let Some(cwd) = args.cwd {
            launch_info.set_working_directory(&cwd);
        }
        if let Some(stop_on_entry) = args.stop_on_entry {
            launch_info.set_launch_flags(launch_info.launch_flags() | LaunchFlag::StopAtEntry);
        }
        if let Some(source_map) = args.source_map {
            let iter = source_map.iter().map(|(k, v)| (k, v.as_ref()));
            self.source_map = source_map::SourceMap::new(iter)?;
        }
        launch_info.set_listener(&self.event_listener);
        self.process = Initialized(self.target.launch(&launch_info)?);
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

            let handle = self
                .var_refs
                .create(None, "[frame]", Container::StackFrame(frame.clone()));
            let mut stack_frame: StackFrame = Default::default();

            stack_frame.id = handle.get() as i64;
            let pc_address = frame.pc_address();
            stack_frame.name = if let Some(name) = frame.function_name() {
                name.to_owned()
            } else {
                format!("{:X}", pc_address.file_address())
            };

            if !self.in_disassembly(&frame) {
                if let Some(le) = frame.line_entry() {
                    let fs = le.file_spec();
                    if let Some(local_path) = self.map_filespec_to_local(&fs) {
                        stack_frame.line = le.line() as i64;
                        stack_frame.column = le.column() as i64;
                        stack_frame.source = Some(Source {
                            name: Some(fs.filename().to_owned()),
                            path: Some(local_path.as_ref().clone()),
                            ..Default::default()
                        });
                    }
                }
            } else {
                let pc_addr = frame.pc_address();
                let dasm = match self.disassembly.get_by_address(&pc_addr) {
                    Some(dasm) => dasm,
                    None => {
                        debug!("Creating disassembly for {:?}", pc_addr);
                        self.disassembly.create_from_address(&pc_addr)
                    }
                };
                stack_frame.line = dasm.line_num_by_address(pc_addr.load_address(&self.target)) as i64;
                stack_frame.column = 0;
                stack_frame.source = Some(Source {
                    name: Some(dasm.source_name().to_owned()),
                    source_reference: Some(handles::to_i64(Some(dasm.handle()))),
                    ..Default::default()
                });
            }
            stack_frames.push(stack_frame);
        }

        Ok(StackTraceResponseBody {
            stack_frames: stack_frames,
            total_frames: Some(thread.num_frames() as i64),
        })
    }

    fn in_disassembly(&mut self, frame: &SBFrame) -> bool {
        match self.show_disassembly {
            Some(v) => v,
            None => if let Some(le) = frame.line_entry() {
                self.map_filespec_to_local(&le.file_spec()).is_none()
            } else {
                true
            },
        }
    }

    fn handle_scopes(&mut self, args: ScopesArguments) -> Result<ScopesResponseBody, Error> {
        let frame_id = Handle::new(args.frame_id as u32).unwrap();
        if let Some(Container::StackFrame(frame)) = self.var_refs.get(frame_id) {
            let frame = frame.clone();
            let locals_handle = self
                .var_refs
                .create(Some(frame_id), "[locs]", Container::Locals(frame.clone()));
            let locals = Scope {
                name: "Local".into(),
                variables_reference: locals_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let statics_handle = self
                .var_refs
                .create(Some(frame_id), "[stat]", Container::Statics(frame.clone()));
            let statics = Scope {
                name: "Static".into(),
                variables_reference: statics_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let globals_handle = self
                .var_refs
                .create(Some(frame_id), "[glob]", Container::Globals(frame.clone()));
            let globals = Scope {
                name: "Global".into(),
                variables_reference: globals_handle.get() as i64,
                expensive: false,
                ..Default::default()
            };
            let registers_handle = self
                .var_refs
                .create(Some(frame_id), "[regs]", Container::Registers(frame));
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
        let container_handle = handles::from_i64(args.variables_reference).unwrap();

        if let Some(container) = self.var_refs.get(container_handle) {
            let variables = match container {
                Container::Locals(frame) => {
                    let ret_val = frame.thread().stop_return_value();
                    let variables = frame.variables(&VariableOptions {
                        arguments: true,
                        locals: true,
                        statics: false,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = ret_val.into_iter().chain(variables.iter());
                    self.convert_scope_values(&mut vars_iter, "", Some(container_handle))
                }
                Container::Statics(frame) => {
                    let variables = frame.variables(&VariableOptions {
                        arguments: false,
                        locals: false,
                        statics: true,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = variables.iter().filter(|v| v.value_type() != ValueType::VariableStatic);
                    self.convert_scope_values(&mut vars_iter, "", Some(container_handle))
                }
                Container::Globals(frame) => {
                    let variables = frame.variables(&VariableOptions {
                        arguments: false,
                        locals: false,
                        statics: true,
                        in_scope_only: true,
                        use_dynamic: DynamicValueType::NoDynamicValues,
                    });
                    let mut vars_iter = variables.iter(); //.filter(|v| v.value_type() != ValueType::VariableGlobal);
                    self.convert_scope_values(&mut vars_iter, "", Some(container_handle))
                }
                Container::Registers(frame) => {
                    let list = frame.registers();
                    let mut vars_iter = list.iter();
                    self.convert_scope_values(&mut vars_iter, "", Some(container_handle))
                }
                Container::Container(var) => {
                    let container_eval_name = self.compose_container_eval_name(container_handle);
                    let var = var.clone();
                    let mut vars_iter = var.children();
                    let mut variables =
                        self.convert_scope_values(&mut vars_iter, &container_eval_name, Some(container_handle));
                    // If synthetic, add [raw] view.
                    if var.is_synthetic() {
                        let raw_var = var.non_synthetic_value();
                        let handle =
                            self.var_refs
                                .create(Some(container_handle), "[raw]", Container::Container(raw_var));
                        let raw = Variable {
                            name: "[raw]".to_owned(),
                            value: var.type_name().unwrap_or_default().to_owned(),
                            variables_reference: handles::to_i64(Some(handle)),
                            ..Default::default()
                        };
                        variables.push(raw);
                    }
                    variables
                }
                Container::StackFrame(_) => vec![],
            };
            Ok(VariablesResponseBody { variables: variables })
        } else {
            Err(Error::Internal(format!(
                "Invalid variabes reference: {}",
                container_handle
            )))
        }
    }

    fn compose_container_eval_name(&self, container_handle: Handle) -> String {
        let mut eval_name = String::new();
        let mut container_handle = Some(container_handle);
        while let Some(h) = container_handle {
            let (parent_handle, key, value) = self.var_refs.get_full_info(h).unwrap();
            match value {
                Container::Container(var) if var.value_type() != ValueType::RegisterSet => {
                    eval_name = compose_eval_name(key, eval_name);
                    container_handle = parent_handle;
                }
                _ => break,
            }
        }
        eval_name
    }

    fn convert_scope_values(
        &mut self, vars_iter: &mut Iterator<Item = SBValue>, container_eval_name: &str,
        container_handle: Option<Handle>,
    ) -> Vec<Variable> {
        let mut variables = vec![];
        let mut variables_idx = HashMap::new();
        for var in vars_iter {
            if let Some(name) = var.name() {
                let dtype = var.type_name();
                let value = self.get_var_value_str(&var, container_handle.is_some());
                let handle = self.get_var_handle(container_handle, name, &var);

                let eval_name = if var.prefer_synthetic_value() {
                    Some(compose_eval_name(container_eval_name, name))
                } else {
                    var.expression_path().map(|p| {
                        let mut p = p;
                        p.insert_str(0, "/nat ");
                        p
                    })
                };

                let variable = Variable {
                    name: name.to_owned(),
                    value: value,
                    type_: dtype.map(|v| v.to_owned()),
                    variables_reference: handles::to_i64(handle),
                    evaluate_name: eval_name,
                    ..Default::default()
                };

                // Ensure proper shadowing
                if let Some(idx) = variables_idx.get(&variable.name) {
                    variables[*idx] = variable;
                } else {
                    variables_idx.insert(variable.name.clone(), variables.len());
                    variables.push(variable);
                }
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
                    .create(parent_handle, key, Container::Container(var.clone())),
            )
        } else {
            None
        }
    }

    // Get a displayable string from a SBValue
    fn get_var_value_str(&self, var: &SBValue, is_container: bool) -> String {
        // TODO: let mut var: Cow<&SBValue> = var.into(); ???
        let mut value_opt: Option<String> = None;
        let mut var2: Option<SBValue> = None;
        let mut var = var;
        // TODO: formats
        // TODO: && format == eFormatDefault
        if self.deref_pointers {
            let type_class = var.type_().type_class();
            if type_class.intersects(TypeClass::Pointer | TypeClass::Reference) {
                if var.value_as_unsigned(0) == 0 {
                    value_opt = Some("<null>".to_owned());
                } else {
                    if var.is_synthetic() {
                        value_opt = var.summary().map(|s| into_string_lossy(s));
                    } else {
                        var2 = Some(var.dereference());
                        var = var2.as_ref().unwrap();
                    }
                }
            }
        }

        // Try value, then summary
        if value_opt.is_none() {
            value_opt = var.value().map(|s| into_string_lossy(s));
            if value_opt.is_none() {
                value_opt = var.summary().map(|s| into_string_lossy(s));
            }
        }

        let value_str = match value_opt {
            Some(s) => s,
            None => {
                if is_container {
                    if self.container_summary {
                        self.get_container_summary(var)
                    } else {
                        "{...}".to_owned()
                    }
                } else {
                    "<not available>".to_owned()
                }
            }
        };

        value_str
    }

    fn get_container_summary(&self, var: &SBValue) -> String {
        const MAX_LENGTH: usize = 32;

        let mut summary = String::from("{");
        let mut empty = true;
        for child in var.children() {
            if let Some(name) = child.name() {
                if let Some(Ok(value)) = child.value().map(|s| s.to_str()) {
                    if empty {
                        empty = false;
                    } else {
                        summary.push_str(", ");
                    }

                    if name.starts_with("[") {
                        summary.push_str(value);
                    } else {
                        write!(summary, "{}:{}", name, value);
                    }
                }
            }

            if summary.len() > MAX_LENGTH {
                summary.push_str(", ...");
                break;
            }
        }
        if empty {
            summary.push_str("...");
        }
        summary.push_str("}");
        summary
    }

    fn handle_evaluate(&mut self, args: EvaluateArguments) -> Result<EvaluateResponseBody, Error> {
        let frame: Option<&SBFrame> = args.frame_id.map(|id| {
            let handle = handles::from_i64(id).unwrap();
            if let Some(Container::StackFrame(ref frame)) = self.var_refs.get(handle) {
                frame
            } else {
                panic!("Invalid frameId");
            }
        });

        let context = args.context.as_ref().map(|s| s.as_str());
        let mut expression: &str = &args.expression;

        if let Some("repl") = context {
            if !expression.starts_with("?") {
                // LLDB command
                let result = self.execute_command_in_frame(expression, frame);
                let text = if result.succeeded() {
                    result.output()
                } else {
                    result.error()
                };
                let response = EvaluateResponseBody {
                    result: into_string_lossy(text),
                    ..Default::default()
                };
                return Ok(response);
            } else {
                expression = &expression[1..]; // drop '?'
            }
        }
        // Expression
        self.evaluate_expr_in_frame(expression, frame).map(|val| match val {
            Evaluated::SBValue(sbval) => {
                let handle = self.get_var_handle(None, expression, &sbval);
                EvaluateResponseBody {
                    result: self.get_var_value_str(&sbval, handle.is_some()),
                    type_: sbval.type_name().map(|s| s.to_owned()),
                    variables_reference: handles::to_i64(handle),
                    ..Default::default()
                }
            }
            Evaluated::String(s) => EvaluateResponseBody {
                result: s,
                ..Default::default()
            },
        })
    }

    // Evaluates expr in the context of frame (or in global context if frame is None)
    // Returns expressions.Value or SBValue on success, SBError on failure.
    fn evaluate_expr_in_frame(&self, expr: &str, frame: Option<&SBFrame>) -> Result<Evaluated, Error> {
        let (expr, ty) = self.get_expression_type(expr);
        match ty {
            ExprType::Native => {
                let result = match frame {
                    Some(frame) => frame.evaluate_expression(expr),
                    None => self.target.evaluate_expression(expr),
                };
                let error = result.error();
                if error.success() {
                    Ok(Evaluated::SBValue(result))
                } else {
                    Err(error.into())
                }
            }
            ExprType::Python => {
                let interpreter = self.debugger.command_interpreter();
                let context = self.context_from_frame(frame);
                match python::evaluate(&interpreter, &expr, false, &context) {
                    Ok(val) => Ok(val),
                    Err(s) => Err(Error::UserError(s)),
                }
            }
            ExprType::Simple => {
                let interpreter = self.debugger.command_interpreter();
                let context = self.context_from_frame(frame);
                match python::evaluate(&interpreter, &expr, true, &context) {
                    Ok(val) => Ok(val),
                    Err(s) => Err(Error::UserError(s)),
                }
            }
        }
    }

    // Classify expression by evaluator type
    fn get_expression_type(&self, expr: &'a str) -> (&'a str, ExprType) {
        if expr.starts_with("/nat ") {
            (&expr[5..], ExprType::Native)
        } else if expr.starts_with("/py ") {
            (&expr[4..], ExprType::Python)
        } else if expr.starts_with("/se ") {
            (&expr[4..], ExprType::Simple)
        } else {
            // TODO: expressions config
            (expr, ExprType::Simple)
        }
    }

    fn execute_command_in_frame(&self, command: &str, frame: Option<&SBFrame>) -> SBCommandReturnObject {
        let context = self.context_from_frame(frame);
        let mut result = SBCommandReturnObject::new();
        let interp = self.debugger.command_interpreter();
        interp.handle_command_with_context(command, &context, &mut result, false);
        // TODO: multiline
        result
    }

    fn context_from_frame(&self, frame: Option<&SBFrame>) -> SBExecutionContext {
        match frame {
            Some(frame) => SBExecutionContext::from_frame(&frame),
            None => match self.process {
                Initialized(ref process) => SBExecutionContext::from_process(&process),
                NotInitialized => SBExecutionContext::new(),
            },
        }
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
        self.before_resume();
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
        let frame = thread.frame_at_index(0);
        if !self.in_disassembly(&frame) {
            thread.step_over();
        } else {
            thread.step_instruction(true);
        }
        Ok(())
    }

    fn handle_step_in(&mut self, args: StepInArguments) -> Result<(), Error> {
        self.before_resume();
        let thread = self.process.thread_by_id(args.thread_id as ThreadID)?;
        let frame = thread.frame_at_index(0);
        if !self.in_disassembly(&frame) {
            thread.step_into();
        } else {
            thread.step_instruction(false);
        }
        Ok(())
    }

    fn handle_step_out(&mut self, args: StepOutArguments) -> Result<(), Error> {
        self.before_resume();
        let thread = self.process.thread_by_id(args.thread_id as ThreadID)?;
        thread.step_out();
        Ok(())
    }

    fn handle_source(&mut self, args: SourceArguments) -> Result<SourceResponseBody, Error> {
        let handle = handles::from_i64(args.source_reference).unwrap();
        let dasm = self.disassembly.get_by_handle(handle).unwrap();
        Ok(SourceResponseBody {
            content: dasm.get_source_text(),
            mime_type: Some("text/x-lldb.disassembly".to_owned()),
        })
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

    fn handle_display_settings(&mut self, args: DisplaySettingsArguments) -> Result<(), Error> {
        self.update_display_settings(&args);
        self.refresh_client_display();
        Ok(())
    }

    fn update_display_settings(&mut self, args: &DisplaySettingsArguments) {
        self.show_disassembly = match args.show_disassembly {
            None => None,
            Some(ShowDisassembly::Auto) => None,
            Some(ShowDisassembly::Always) => Some(true),
            Some(ShowDisassembly::Never) => Some(false),
        };
    }

    // Fake target start/stop to force VSCode to refresh UI state.
    fn refresh_client_display(&mut self) {
        let thread_id = self.process.selected_thread().thread_id();
        self.send_event(EventBody::continued(ContinuedEventBody {
            thread_id: thread_id as i64,
            all_threads_continued: Some(true),
        }));
        self.send_event(EventBody::stopped(StoppedEventBody {
            thread_id: Some(thread_id as i64),
            //preserve_focus_hint: Some(true),
            all_threads_stopped: Some(true),
            ..Default::default()
        }));
    }

    fn before_resume(&mut self) {
        self.var_refs.reset();
    }

    fn handle_debug_event(&mut self, event: SBEvent) {
        debug!("Debug event: {:?}", event);
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
        self.update_threads();
        // Find thread that has caused this stop
        let mut stopped_thread = None;
        // Check the currently selected thread first
        let selected_thread = self.process.selected_thread();
        stopped_thread = match selected_thread.stop_reason() {
            StopReason::Invalid | // br
            StopReason::None => None,
            _ => Some(selected_thread),
        };
        // Fall back to scanning all threads in the process
        if stopped_thread.is_none() {
            for thread in self.process.threads() {
                match thread.stop_reason() {
                    StopReason::Invalid | // br
                    StopReason::None => (),
                    _ => {
                        self.process.set_selected_thread(&thread);
                        stopped_thread = Some(thread);
                        break;
                    }
                }
            }
        }
        // Analyze stop reason
        let (stop_reason_str, description) = match stopped_thread {
            Some(ref stopped_thread) => {
                let stop_reason = stopped_thread.stop_reason();
                match stop_reason {
                    StopReason::Breakpoint => ("breakpoint", None),
                    StopReason::Trace | // br
                    StopReason::PlanComplete => ("step", None),
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
            }
            None => ("unknown", None),
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

    // Notify VSCode about target threads that started or exited since the last stop.
    fn update_threads(&mut self) {
        let threads = self.process.threads().map(|t| t.thread_id()).collect::<HashSet<_>>();
        let started = threads.difference(&self.known_threads).cloned().collect::<Vec<_>>();
        let exited = self.known_threads.difference(&threads).cloned().collect::<Vec<_>>();
        for tid in exited {
            self.send_event(EventBody::thread(ThreadEventBody {
                thread_id: tid as i64,
                reason: "exited".to_owned(),
            }));
        }
        for tid in started {
            self.send_event(EventBody::thread(ThreadEventBody {
                thread_id: tid as i64,
                reason: "started".to_owned(),
            }));
        }
        self.known_threads = threads;
    }

    fn map_filespec_to_local(&mut self, filespec: &SBFileSpec) -> Option<Rc<String>> {
        if !filespec.is_valid() {
            return None;
        } else {
            let directory = filespec.directory();
            let filename = filespec.filename();
            match self.source_map_cache.get(&(directory.into(), filename.into())) {
                Some(localized) => localized.clone(),
                None => {
                    let mut localized = self.source_map.to_local(filespec.path());
                    if let Some(ref path) = localized {
                        if self.suppress_missing_files && !path.is_file() {
                            localized = None;
                        }
                    }
                    let localized = localized.map(|path| Rc::new(path.to_string_lossy().into_owned()));
                    self.source_map_cache.insert(
                        (directory.to_owned().into(), filename.to_owned().into()),
                        localized.clone(),
                    );
                    localized
                }
            }
        }
    }
}

fn compose_eval_name<'a, 'b, A, B>(prefix: A, suffix: B) -> String
where
    A: Into<Cow<'a, str>>,
    B: Into<Cow<'b, str>>,
{
    let prefix = prefix.into();
    let suffix = suffix.into();
    if prefix.as_ref().is_empty() {
        suffix.into_owned()
    } else if suffix.as_ref().is_empty() {
        prefix.into_owned()
    } else if suffix.as_ref().starts_with("[") {
        (prefix + suffix).into_owned()
    } else {
        (prefix + "." + suffix).into_owned()
    }
}

fn into_string_lossy(cstr: &std::ffi::CStr) -> String {
    cstr.to_string_lossy().into_owned()
}
