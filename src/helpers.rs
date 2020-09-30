//! This module implements various helpers.

use std::fs::File;
use std::path::{Path, PathBuf};

use std::io::{Seek, Read};
use std::io::SeekFrom::{End, Start};          
use hyper::header::{Location, ContentType, Range, ContentRange, ContentLength};
use hyper::header::ByteRangeSpec::{FromTo, Last, AllFrom};
use hyper::header::ContentRangeSpec::{Bytes};

use mime_guess::guess_mime_type;
use mime::Mime;

use wrappers::Response;
use types::{
    PenHTTPError,
    PencilResult,
    UserError,
};
use http_errors::{
    HTTPError,
        NotFound,
};


/// Path bound trait.
pub trait PathBound {
    /// Opens a resource from the root path folder.  Consider the following
    /// folder structure:
    ///
    /// ```ignore
    /// /myapp.rs
    /// /user.sql
    /// /templates
    ///     /index.html
    /// ```
    ///
    /// If you want to open the `user.sql` file you should do the following:
    ///
    /// ```rust,no_run
    /// use std::io::Read;
    ///
    /// use sharp_pencil::PathBound;
    ///
    ///
    /// fn main() {
    ///     let app = sharp_pencil::Pencil::new("/web/demo");
    ///     let mut file = app.open_resource("user.sql");
    ///     let mut content = String::from("");
    ///     file.read_to_string(&mut content).unwrap();
    /// }
    /// ```
    fn open_resource(&self, resource: &str) -> File;
}


/// Safely join directory and filename, otherwise this returns None.
pub fn safe_join(directory: &str, filename: &str) -> Option<PathBuf> {
    let directory = Path::new(directory);
    let filename = Path::new(filename);
    match filename.to_str() {
        Some(filename_str) => {
            if filename.is_absolute() | (filename_str == "..") | (filename_str.starts_with("../")) {
                None
            } else {
                Some(directory.join(filename_str))
            }
        },
        None => None,
    }
}


/// One helper function that can be used to return HTTP Error inside a view function.
pub fn abort(code: u16) -> PencilResult {
    Err(PenHTTPError(HTTPError::new(code)))
}


/// Returns a response that redirects the client to the target location.
pub fn redirect(location: &str, code: u16) -> PencilResult {
    let mut response = Response::from(format!(
"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 3.2 Final//EN\">
<title>Redirecting...</title>
<h1>Redirecting...</h1>
<p>You should be redirected automatically to target URL: 
<a href=\"{}\">{}</a>.  If not click the link.
", location, location));
    response.status_code = code;
    response.set_content_type("text/html");
    response.headers.set(Location(location.to_string()));
    Ok(response)
}


/// Replace special characters "&", "<", ">" and (") to HTML-safe characters.
pub fn escape(s: String) -> String {
    s.replace("&", "&amp;").replace("<", "&lt;")
     .replace(">", "&gt;").replace("\"", "&quot;")
}

/// Sends the contents of a file to the client.  Please never pass filenames to this
/// function from user sources without checking them first.  Set `as_attachment` to
/// `true` if you want to send this file with a `Content-Disposition: attachment`
/// header.  This will return `NotFound` if filepath is not one file.
pub fn send_file(filepath: &str, mimetype: Mime, as_attachment: bool) -> PencilResult {
    let filepath = Path::new(filepath);
    if !filepath.is_file() {
        return Err(PenHTTPError(NotFound));
    }
    let file = match File::open(&filepath) {
        Ok(file) => file,
        Err(e) => {
            return Err(UserError::new(format!("couldn't open {}: {}", filepath.display(), e)).into());
        }
    };
    let mut response: Response = file.into();
    response.headers.set(ContentType(mimetype));
    if as_attachment {
        match filepath.file_name() {
            Some(file) => {
                match file.to_str() {
                    Some(filename) => {
                        let content_disposition = format!("attachment; filename={}", filename);
                        response.headers.set_raw("Content-Disposition", vec![content_disposition.as_bytes().to_vec()]);
                    },
                    None => {
                        return Err(UserError::new("filename unavailable, required for sending as attachment.").into());
                    }
                }
            },
            None => {
                return Err(UserError::new("filename unavailable, required for sending as attachment.").into());
            }
        }
    }
    Ok(response)
}


/// Sends the contents of a file to the client, supporting HTTP Range requests, so it allows only partial files
/// to be requested and sent. This doesn't support multiranges at the moment.
/// Please never pass filenames to this
/// function from user sources without checking them first.  Set `as_attachment` to
/// `true` if you want to send this file with a `Content-Disposition: attachment`
/// header.  This will return `NotFound` if filepath is not one file.
pub fn send_file_range(filepath: &str, mimetype: Mime, as_attachment: bool, range: Option<&Range>)
    -> PencilResult
{
    let filepath = Path::new(filepath);
    if !filepath.is_file() {
        return Err(PenHTTPError(NotFound));
    }
    let mut file = match File::open(&filepath) {
        Ok(file) => file,
        Err(e) => {
            return Err(UserError::new(format!("couldn't open {}: {}", filepath.display(), e)).into());
        }
    };

    let len = file.metadata().map_err(|_| PenHTTPError(HTTPError::InternalServerError))?.len();
    let mut response: Response = match range {
        Some(&Range::Bytes(ref vec_ranges)) => {
            if vec_ranges.len() != 1 { return Err(PenHTTPError(HTTPError::NotImplemented)) };
            match vec_ranges[0] {
                FromTo(s, e) => {
                    file.seek(Start(s))
                        .map_err(|_| PenHTTPError(HTTPError::InternalServerError))?;
                    let mut resp = Response::new(file.take(e-s+1));
                    resp.status_code = 206;
                    resp.headers.set(ContentLength(e-s+1));
                    resp.headers.set(ContentRange(
                        Bytes{range: Some((s, e)), instance_length: Some(len)}
                    ));
                    resp
                },
                AllFrom(s) => {
                    file.seek(Start(s))
                        .map_err(|_| PenHTTPError(HTTPError::InternalServerError))?;
                    let mut resp = Response::new(file);
                    resp.status_code = 206;
                    resp.headers.set(ContentLength(len-s));
                    resp.headers.set(ContentRange(
                        Bytes{range: Some((s, len-1)), instance_length: Some(len)}
                    ));
                    resp
                },
                Last(l) => {
                    file.seek(End(-(l as i64)))
                        .map_err(|_| PenHTTPError(HTTPError::InternalServerError))?;
                    let mut resp = Response::new(file);
                    resp.status_code = 206;
                    resp.headers.set(ContentLength(l));
                    resp.headers.set(ContentRange(
                        Bytes{range: Some((len-l, len-1)), instance_length: Some(len)}
                    ));
                    resp
                },
            }
        },
        Some(_) => return Err(PenHTTPError(HTTPError::NotImplemented)),
        None => {
            let mut resp = Response::new(file);
            resp.headers.set(ContentLength(len));
            resp
        },
    };

    response.headers.set(ContentType(mimetype));
    if as_attachment {
        match filepath.file_name() {
            Some(file) => {
                match file.to_str() {
                    Some(filename) => {
                        let content_disposition = format!("attachment; filename={}", filename);
                        response.headers.set_raw("Content-Disposition", vec![content_disposition.as_bytes().to_vec()]);
                    },
                    None => {
                        return Err(UserError::new("filename unavailable, required for sending as attachment.").into());
                    }
                }
            },
            None => {
                return Err(UserError::new("filename unavailable, required for sending as attachment.").into());
            }
        }
    }
    Ok(response)
}


/// Send a file from a given directory with `send_file`.  This is a secure way to
/// quickly expose static files from an folder.  This will guess the mimetype
/// for you.
pub fn send_from_directory(directory: &str, filename: &str,
                           as_attachment: bool) -> PencilResult {
    match safe_join(directory, filename) {
        Some(filepath) => {
            let mimetype = guess_mime_type(filepath.as_path());
            match filepath.as_path().to_str() {
                Some(filepath) => {
                    send_file(filepath, mimetype, as_attachment)
                },
                None => {
                    Err(PenHTTPError(NotFound))
                }
            }
        },
        None => {
            Err(PenHTTPError(NotFound))
        }
    }
}

/// Send a file from a given directory with `send_file`, supporting HTTP Range requests, so it allows only partial files
/// to be requested and sent. This doesn't support multiranges at the moment. This is a secure way to
/// quickly expose static files from an folder.  This will guess the mimetype
/// for you.
pub fn send_from_directory_range(directory: &str, filename: &str,
                           as_attachment: bool, range: Option<&Range>)
    -> PencilResult
{
    match safe_join(directory, filename) {
        Some(filepath) => {
            let mimetype = guess_mime_type(filepath.as_path());
            match filepath.as_path().to_str() {
                Some(filepath) => {
                    send_file_range(filepath, mimetype, as_attachment, range)
                },
                None => {
                    Err(PenHTTPError(NotFound))
                }
            }
        },
        None => {
            Err(PenHTTPError(NotFound))
        }
    }
}
