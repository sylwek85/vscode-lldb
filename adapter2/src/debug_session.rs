use std;
use std::boxed::FnBox;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::mem;
use std::option;
use std::path::{self, Path};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

use futures::prelude::*;
use futures::stream;
use futures::sync::mpsc;
use tokio;
use tokio::prelude::*;
use tokio_threadpool::blocking;

use debug_protocol::*;
use failure;
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
        Error::SBError(sberr.error_string().into())
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

struct DebugSessionInner {
    send_message: mpsc::Sender<ProtocolMessage>,
    event_listener: SBListener,
    debugger: MustInitialize<SBDebugger>,
    target: MustInitialize<SBTarget>,
    process: MustInitialize<SBProcess>,
    on_configuration_done: Option<(u32, Box<AsyncResponder>)>,
    line_breakpoints: HashMap<FileId, HashMap<i64, BreakpointID>>,
    fn_breakpoints: HashMap<String, BreakpointID>,
    breakpoints: HashMap<BreakpointID, BreakpointInfo>,
}

pub struct DebugSession {
    inner: Arc<Mutex<DebugSessionInner>>,
    sender_in: mpsc::Sender<ProtocolMessage>,
    receiver_out: mpsc::Receiver<ProtocolMessage>,
}

impl DebugSession {
    pub fn new() -> Self {
        let (sender_in, receiver_in) = mpsc::channel(10);
        let (sender_out, receiver_out) = mpsc::channel(10);

        let inner = DebugSessionInner {
            send_message: sender_out,
            debugger: NotInitialized,
            target: NotInitialized,
            process: NotInitialized,
            event_listener: SBListener::new_with_name("DebugSession"),
            on_configuration_done: None,
            line_breakpoints: HashMap::new(),
            fn_breakpoints: HashMap::new(),
            breakpoints: HashMap::new(),
        };
        let inner = Arc::new(Mutex::new(inner));

        // Dispatch incoming requests to inner.handle_message()
        let inner2 = inner.clone();
        let sink_to_inner = tokio::spawn(receiver_in.for_each(move |msg| {
            inner2.lock().unwrap().handle_message(msg);
            Ok(())
        }));
        mem::forget(sink_to_inner);

        // Create a thread listening on inner's event_listener
        let (mut sender, mut receiver) = mpsc::channel(10);
        let listener = inner.lock().unwrap().event_listener.clone();
        thread::spawn(move || {
            let mut event = SBEvent::new();
            while sender.poll_ready().is_ok() {
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
        let event_listener_to_inner = tokio::spawn(receiver.for_each(move |event| {
            inner2.lock().unwrap().handle_debug_event(event);
            Ok(())
        }));
        mem::forget(event_listener_to_inner);

        DebugSession {
            inner,
            sender_in,
            receiver_out,
        }
    }
}

impl Stream for DebugSession {
    type Item = ProtocolMessage;
    type Error = ();
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.receiver_out.poll()
    }
}

impl Sink for DebugSession {
    type SinkItem = ProtocolMessage;
    type SinkError = ();
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.sender_in.start_send(item).map_err(|err| panic!("{:?}", err))
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.sender_in.poll_complete().map_err(|err| panic!("{:?}", err))
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////////

unsafe impl Send for DebugSession {}
unsafe impl Send for DebugSessionInner {}

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
        launch_info.set_listener(&self.event_listener);
        self.process = Initialized(self.target.launch(launch_info)?);
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
        for i in start_frame..start_frame + levels {
            let frame = thread.frame_at_index(i as u32);
            if !frame.is_valid() {
                break;
            }
        }
        unimplemented!()
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
}
