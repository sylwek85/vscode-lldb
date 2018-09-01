use failure;
use lldb;
use serde_json;
use std::error::Error as ErrorTrait;
use std::io;
use std::option;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "Internal debugger error: {}", _0)]
    Internal(String),
    #[fail(display = "Debugger protocol error: {}", _0)]
    Protocol(String),
    #[fail(display = "An error caused by user action: {}", _0)]
    UserError(String),
}

impl From<option::NoneError> for Error {
    fn from(_: option::NoneError) -> Self {
        Error::Internal("Expected Option::Some, found None".into())
    }
}
impl From<lldb::SBError> for Error {
    fn from(err: lldb::SBError) -> Self {
        Error::Internal(err.description().into())
    }
}
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Internal(err.description().into())
    }
}
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Internal(err.description().into())
    }
}
