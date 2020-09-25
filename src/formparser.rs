//! This module implements the form parsing. It supports url-encoded forms
//! as well as multipart uploads.

use std::{fmt::{self, Formatter}, io::{Cursor, Read}, string::FromUtf8Error};

use headers::{ContentType, HeaderMapExt};
use hyper::{Body, Request};
use mime::{self, Mime, Name};
use url::form_urlencoded;
use multipart::server::Multipart;

use crate::{datastructures::MultiDict, helpers};

#[derive(Debug)]
pub enum Error {
    StreamReadError(hyper::Error),
    NoBoundaryError,
    MultipartParseError(std::io::Error),
    MultipartStringDecodingError(FromUtf8Error),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

const WWW_FORM_URLENCODED: (Name, Name) = (mime::APPLICATION, mime::WWW_FORM_URLENCODED);
const MULTIPART_FORMDATA: (Name, Name) = (mime::MULTIPART, mime::FORM_DATA);

/// This type implements parsing of form data for Pencil. It can parse
/// multipart and url encoded form data.
pub async fn parse(request: &mut Request<Body>) -> Result<(MultiDict<String>, MultiDict<Vec<u8>>), Error> {
    let headers = request.headers();
    let mime: Mime = match headers.typed_get::<ContentType>() {
        Some(ctype) => ctype.into(),
        None => return Ok((MultiDict::new(), MultiDict::new())),
    };
    let mimetype = (mime.type_(), mime.subtype());

    let body = match mimetype {
        WWW_FORM_URLENCODED | MULTIPART_FORMDATA => {
            let body = request.body_mut();
            helpers::load_body(body)
                .await
                .map_err(|e| Error::StreamReadError(e))?
        },
        _ => return Ok((MultiDict::new(), MultiDict::new())),
    };
    let mut form = MultiDict::new();
    let mut files = MultiDict::new();

    match mimetype {
        WWW_FORM_URLENCODED => {
            for (k, v) in form_urlencoded::parse(&body).into_owned() {
                form.add(k, v);
            }
        },

        MULTIPART_FORMDATA => {
            let body = Cursor::new(body);
            let boundary = mime.get_param(mime::BOUNDARY).ok_or(Error::NoBoundaryError)?.as_str();
            let mut multipart = Multipart::with_body(body, boundary);
            while let Some(mut field) = multipart.read_entry()
                .map_err(|e| Error::MultipartParseError(e))?
            {
                if field.is_text() {
                    let mut data = Vec::new();
                    field.data.read_to_end(&mut data).expect("TODO");
                    form.add(field.headers.name.to_string(), String::from_utf8(data)
                        .map_err(|e| Error::MultipartStringDecodingError(e))?);
                } else {
                    let mut data = Vec::new();
                    field.data.read_to_end(&mut data).expect("TODO");
                    files.add(field.headers.name.to_string(), data);
                }
            }
        }

        _ => unreachable!(),
    }
    Ok((form, files))
}