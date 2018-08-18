use failure;
use lldb;
use std::io;
use std::option;
use std::error::Error as ErrorTrait;

#[derive(Fail, Debug)]
pub enum Error {
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
        Error::SBError(sberr.message().into())
    }
}
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Internal(err.description().into())
    }
}
