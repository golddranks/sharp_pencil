//! This module implements various helpers.

use std::{fs::File, ops::Bound};
use std::path::{Path, PathBuf};
use std::io::{Seek, Read};
use std::io::SeekFrom::Start; 

use futures_util::TryStreamExt;
use hyper::{Body, header::{LOCATION, CONTENT_DISPOSITION}};

use headers::{ContentLength, ContentRange, ContentType, HeaderMapExt, HeaderValue, Range};

use mime::Mime;

use crate::wrappers::Response;
use crate::types::{
    PenHTTPError,
    PencilResult,
    UserError,
};
use crate::http_errors::{
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
    response.headers.insert(LOCATION, HeaderValue::from_str(&location).expect("TODO"));
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
    response.headers.typed_insert(ContentType::from(mimetype));
    if as_attachment {
        match filepath.file_name() {
            Some(file) => {
                match file.to_str() {
                    Some(filename) => {
                        let content_disposition = format!("attachment; filename={}", filename);
                        response.headers.insert(CONTENT_DISPOSITION, HeaderValue::from_str(&content_disposition).expect("TODO"));
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
    let mut buf = Vec::new();
    let mut response: Response = match range {
        Some(range) => {
            let mut range_iter = range.iter();
            let one_range = (range_iter.next(), range_iter.next());
            if let (Some((start, end)), None) = one_range {
                let start = match start {
                    Bound::Unbounded => 0,
                    Bound::Included(start) => start,
                    Bound::Excluded(start) => start+1,
                    // TODO The suffix-length isn't taken into account by the headers library?
                };
                file.seek(Start(start))
                    .map_err(|_| PenHTTPError(HTTPError::InternalServerError))?;

                let end = match end {
                    Bound::Unbounded => len,
                    Bound::Included(end) => end+1,
                    Bound::Excluded(end) => end,
                };
                file.take(end-start).read_to_end(&mut buf).expect("TODO");

                let content_len = buf.len() as u64;
                let mut resp = Response::new(Body::from(buf));
                resp.status_code = 206;
                resp.headers.typed_insert(ContentLength(content_len));
                resp.headers.typed_insert(ContentRange::bytes(start..end, content_len).expect("TODO"));
                resp
            } else {
                file.read_to_end(&mut buf).expect("TODO");
                let mut resp = Response::new(Body::from(buf));
                resp.headers.typed_insert(ContentLength(len));
                resp
            }
        },
        None => {
            file.read_to_end(&mut buf).expect("TODO");
            let mut resp = Response::new(Body::from(buf));
            resp.headers.typed_insert(ContentLength(len));
            resp
        },
    };

    response.headers.typed_insert(ContentType::from(mimetype));
    if as_attachment {
        match filepath.file_name() {
            Some(file) => {
                match file.to_str() {
                    Some(filename) => {
                        let content_disposition = format!("attachment; filename={}", filename);
                        response.headers.insert(CONTENT_DISPOSITION, HeaderValue::from_str(&content_disposition).expect("TODO"));
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
            let mimetype = mime_guess::from_path(filepath.as_path()).first_or_octet_stream();
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
            let mimetype = mime_guess::from_path(filepath.as_path()).first_or_octet_stream();
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

pub async fn load_body(body: &mut Body) -> Result<Vec<u8>, hyper::Error> {
    body.try_fold(Vec::new(), |mut buf, chunk| async move {
        buf.extend(chunk);
        Ok(buf)
    }).await
}