use debug_protocol::*;
use failure;
use lldb::{self, BreakpointID};
use must_initialize::{Initialized, MustInitialize, NotInitialized};
use std::boxed::FnBox;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::option;
use std::path::{self, Path};

//use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use futures::sync::mpsc;
use worker_thread::{WorkerThread, CancellationToken};

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
impl From<lldb::SBError> for Error {
    fn from(sberr: lldb::SBError) -> Self {
        Error::SBError(sberr.error_string().into())
    }
}

type AsyncResponder = FnBox(&mut DebugSession) -> Result<ResponseBody, Error>;

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
        resolved_line: u32,
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

pub struct DebugSession {
    send_message: mpsc::Sender<ProtocolMessage>,
    debugger: MustInitialize<lldb::SBDebugger>,
    target: MustInitialize<lldb::SBTarget>,
    process: MustInitialize<lldb::SBProcess>,
    listener: MustInitialize<lldb::SBListener>,
    listener_thread: MustInitialize<WorkerThread>,
    on_configuration_done: Option<(u32, Box<AsyncResponder>)>,
    line_breakpoints: HashMap<FileId, HashMap<i64, BreakpointID>>,
    fn_breakpoints: HashMap<String, BreakpointID>,
    breakpoints: HashMap<BreakpointID, BreakpointInfo>,
}

unsafe impl Send for DebugSession {}

impl DebugSession {
    pub fn new(send_message: mpsc::Sender<ProtocolMessage>) -> Self {
        lldb::SBDebugger::initialize();
        DebugSession {
            send_message,
            debugger: NotInitialized,
            target: NotInitialized,
            process: NotInitialized,
            listener: NotInitialized,
            listener_thread: NotInitialized,
            on_configuration_done: None,
            line_breakpoints: HashMap::new(),
            fn_breakpoints: HashMap::new(),
            breakpoints: HashMap::new(),
        }
    }

    pub fn handle_message(&mut self, message: ProtocolMessage) {
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
            RequestArguments::threads => self.handle_threads().map(|r| ResponseBody::threads(r)),
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
        self.send_message.try_send(response).map_err(|err| panic!("Could not send response: {}", err));
    }

    fn send_event(&mut self, event_body: EventBody) {
        let event = ProtocolMessage::Event(Event {
            seq: 0,
            body: event_body,
        });
        self.send_message.try_send(event).map_err(|err| panic!("Could not send event: {}", err));
    }

    fn handle_initialize(&mut self, args: InitializeRequestArguments) -> Result<Capabilities, Error> {
        self.debugger = Initialized(lldb::SBDebugger::create(false));
        self.debugger.set_async(true);

        // let (sender, recver) = sync_channel::<lldb::SBEvent>(100);

        // WorkerThread::spawn(move |token| {
        //     let listener = lldb::SBListener::new_with_name("DebugSession");
        //     let mut event = lldb::SBEvent::new();
        //     while !token.is_cancelled() {
        //         if listener.wait_for_event(1, &mut event) {
        //             sender.send(event);
        //             event = lldb::SBEvent::new();
        //         }
        //     }
        // });

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
            let bp = if let Some(bp_id) = existing_bps.get(&req.line).cloned() {
                self.target.find_breakpoint_by_id(bp_id)
            } else {
                let file_name = Path::new(file_path).file_name().unwrap().to_str().unwrap();
                let bp = self.target.breakpoint_create_by_location(file_name, req.line as u32);
                existing_bps.insert(req.line, bp.id());
                bp
            };
            breakpoints.push(Breakpoint {
                id: Some(bp.id() as i64),
                verified: true,
                column: None,
                end_column: None,
                line: None,
                end_line: None,
                message: None,
                source: None,
            });
        }
        breakpoints
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
        Ok(Box::new(move |s: &mut DebugSession| s.complete_launch(args)))
    }

    fn complete_launch(&mut self, args: LaunchRequestArguments) -> Result<ResponseBody, Error> {
        let mut launch_info = lldb::SBLaunchInfo::new();
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
}
