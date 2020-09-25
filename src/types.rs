//! This module implements a number of types.

use std::collections::HashMap;
use std::error;
use std::convert;
use std::fmt;

use crate::wrappers::{Request, Response};
pub use crate::http_errors::HTTPError;

pub use self::PencilError::{
    PenHTTPError,
    PenUserError
};


/// The Pencil User Error type.
#[derive(Clone, Debug)]
pub struct UserError {
    pub desc: String,
}

impl UserError {
    pub fn new<T>(desc: T) -> UserError where T: AsRef<str> {
        UserError {
            desc: desc.as_ref().to_owned(),
        }
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.desc)
    }
}

impl error::Error for UserError {
    fn description(&self) -> &str {
        &self.desc
    }
}


/// The Pencil Error type.
#[derive(Clone, Debug)]
pub enum PencilError {
    PenHTTPError(HTTPError),
    PenUserError(UserError),
}

impl convert::From<HTTPError> for PencilError {
    fn from(err: HTTPError) -> PencilError {
        PenHTTPError(err)
    }
}

impl convert::From<UserError> for PencilError {
    fn from(err: UserError) -> PencilError {
        PenUserError(err)
    }
}

impl fmt::Display for PencilError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PenHTTPError(ref err) => f.write_str(&err.to_string()),
            PenUserError(ref err) => f.write_str(&err.to_string()),
        }
    }
}

impl error::Error for PencilError {

    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            PenHTTPError(ref err) => Some(&*err as &dyn error::Error),
            PenUserError(_) => None,
        }
    }
}


/// The Pencil Result type.
pub type PencilResult = Result<Response, PencilError>;


/// View arguments type.
pub type ViewArgs = HashMap<String, String>;
/// View function type.
pub type ViewFunc = fn(&mut Request) -> PencilResult;


/// HTTP Error handler type.
pub type HTTPErrorHandler = dyn Fn(HTTPError) -> PencilResult + Send + Sync;
/// User Error handler type.
pub type UserErrorHandler = dyn Fn(UserError) -> PencilResult + Send + Sync;

/// Before request func type.
pub type BeforeRequestFunc = dyn Fn(&mut Request) -> Option<PencilResult> + Send + Sync;


/// After request func type.
pub type AfterRequestFunc = dyn Fn(&Request, &mut Response) + Send + Sync;


/// Teardown request func type.
pub type TeardownRequestFunc = dyn Fn(Option<&PencilError>) + Send + Sync;
