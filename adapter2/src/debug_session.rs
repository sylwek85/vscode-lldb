use debug_protocol::*;
use failure;
use lldb;
use std::option;
use std::sync::mpsc::SyncSender;

#[derive(Fail, Debug)]
enum Error {
    #[fail(display = "Whoops! Something that was supposed to have been initialized, wasn't.")]
    NotInitialized,
    #[fail(display = "{}", _0)]
    SBError(String),
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

pub struct DebugSession {
    send_message: SyncSender<ProtocolMessage>,
    debugger: Option<lldb::SBDebugger>,
    target: Option<lldb::SBTarget>,
    launch_args: Option<LaunchRequestArguments>,
}

impl DebugSession {
    pub fn new(send_message: SyncSender<ProtocolMessage>) -> Self {
        lldb::SBDebugger::initialize();
        DebugSession {
            send_message,
            debugger: None,
            target: None,
            launch_args: None,
        }
    }

    pub fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::Request(request) => self.handle_request(request),
            ProtocolMessage::Response(response) => self.handle_response(response),
            _ => (), //warn!("No handler for {} message", message.command);
        };
    }

    fn handle_response(&mut self, response: Response) {}

    fn handle_request(&mut self, request: Request) {
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
            RequestArguments::configurationDone(args) => self
                .handle_configuration_done(args)
                .map(|r| ResponseBody::configurationDone),
            RequestArguments::launch(args) => {
                self.handle_launch(args);
                return;
            } // launch responds asynchronously
            _ => panic!(),
        };
        let response = match result {
            Ok(body) => ProtocolMessage::Response(Response {
                request_seq: request.seq,
                success: true,
                message: None,
                body: Some(body),
            }),
            Err(err) => ProtocolMessage::Response(Response {
                request_seq: request.seq,
                success: false,
                body: None,
                message: Some(format!("{}", err)),
            }),
        };
        self.send_message.send(response);
    }

    fn handle_initialize(&mut self, args: InitializeRequestArguments) -> Result<Capabilities, Error> {
        self.debugger = Some(lldb::SBDebugger::create(false));
        let caps = Capabilities {
            supports_configuration_done_request: Some(true),
            supports_evaluate_for_hovers: Some(true),
            supports_function_breakpoints: Some(true),
            supports_conditional_breakpoints: Some(true),
            supports_hit_conditional_breakpoints: Some(true),
            supports_set_variable: Some(true),
            supports_completions_request: Some(true),
            supports_delayed_stack_trace_loading: Some(true),
            support_terminate_debuggee: Some(true),
            supports_log_points: Some(true),
            ..Default::default()
            //supportsStepBack': self.parameters.get('reverseDebugging', False),
            //exception_breakpoint_filters: exc_filters,
        };
        Ok(caps)
    }

    fn handle_set_breakpoints(&mut self, args: SetBreakpointsArguments) -> Result<SetBreakpointsResponseBody, Error> {
        let response = SetBreakpointsResponseBody { breakpoints: vec![] };
        Ok(response)
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

    fn handle_configuration_done(&mut self, args: ConfigurationDoneArguments) -> Result<(), Error> {
        if let Some(ref launch_args) = self.launch_args {}
        Ok(())
    }

    fn handle_launch(&mut self, args: LaunchRequestArguments) -> Result<(), Error> {
        self.target = Some(self.debugger.as_ref()?.create_target(&args.program, None, None, false)?);
        self.launch_args = Some(args);
        self.send_event(EventBody::initialized);
        Ok(())
    }

    fn send_event(&mut self, event_body: EventBody) {
        let event = ProtocolMessage::Event(Event {
            seq: 0,
            body: event_body,
        });
        self.send_message.send(event);
    }
}
