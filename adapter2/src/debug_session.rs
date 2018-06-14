use debug_protocol::*;
use std::error::Error;
use lldb;

type Result<T> = ::std::result::Result<T, Box<Error>>;

#[derive(Default)]
pub struct DebugSession {
    debugger: Option<lldb::SBDebugger>,
    launch_args: Option<LaunchRequestArguments>,
}

impl DebugSession {
    pub fn new() -> Self {
        DebugSession {
            ..Default::default()
        }
    }

    pub fn handle_message(&mut self, message: ProtocolMessage) {
        match message {
            ProtocolMessage::request(request) => self.handle_request(request),
            ProtocolMessage::response(response) => self.handle_response(response),
            _ => (),
        };
    }

    fn handle_response(&mut self, response: Response) {}

    fn handle_request(&mut self, request: Request) {
        let body = match request.arguments {
            RequestArguments::initialize(args) => self.handle_initialize(args),
            RequestArguments::launch(args) => self.handle_launch(args),
            RequestArguments::configurationDone(args) => self.handle_configuration_done(args),
            _ => panic!(),
        };
    }

    fn handle_initialize(&mut self, args: InitializeRequestArguments) -> Result<ResponseBody> {
        self.debugger = Some(lldb::SBDebugger::create(true));
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
        Ok(ResponseBody::initialize(caps))
    }

    fn handle_launch(&mut self, args: LaunchRequestArguments) -> Result<ResponseBody> {
        self.launch_args = Some(args);
        self.target = self.debugger?.create_target(&args.program, None, None, false)?;
        self.send_event(EventBody::initialized);
        Ok(ResponseBody::Async)
    }

    fn handle_configuration_done(
        &mut self,
        args: ConfigurationDoneArguments,
    ) -> Result<ResponseBody> {
        if let Some(ref launch_args) = self.launch_args {

        }
        Ok(ResponseBody::configurationDone)
    }

    fn send_event(&mut self, event: EventBody) {}
}
