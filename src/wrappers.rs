//! This module implements simple request and response objects.

use std::fmt;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write, Take};
use std::convert;

use hyper::{self, Method, Body};
use hyper::Request as HttpRequest;
use hyper::header::HeaderMap;
use headers::{ContentLength, ContentType, Cookie, HeaderMapExt, Host, SetCookie};
use futures_util::StreamExt;

use mime::Mime;
use url::form_urlencoded;
use serde_json;
use typemap::TypeMap;

use crate::{app::Pencil, helpers};
use crate::datastructures::MultiDict;
use crate::httputils::{get_name_by_http_code, get_content_type};
use crate::routing::{Rule, MapAdapterMatched, MapAdapter};
use crate::types::ViewArgs;
use crate::http_errors::HTTPError;
use crate::formparser;
use lazycell::LazyCell;


/// Request type.
pub struct Request<'r> {
    pub app: &'r Pencil,
    pub request: HttpRequest<Body>,
    /// The URL rule that matched the request. This is
    /// going to be `None` if nothing matched.
    pub url_rule: Option<Rule>,
    /// A dict of view arguments that matched the request.
    pub view_args: ViewArgs,
    /// If matching the URL requests a redirect, this will be the redirect.
    pub routing_redirect: Option<(String, u16)>,
    /// If matching the URL failed, this will be the routing error.
    pub routing_error: Option<HTTPError>,
    /// Storage for data of extensions.
    pub extensions_data: TypeMap,
    args: LazyCell<MultiDict<String>>,
    form: LazyCell<MultiDict<String>>,
    files: LazyCell<MultiDict<Vec<u8>>>,
    cached_json: LazyCell<Option<serde_json::Value>>
}

impl<'r> Request<'r> {
    /// Create a `Request`.
    pub fn new(app: &'r Pencil, request: HttpRequest<Body>) -> Result<Request<'r>, Body> {
        /* // TODO: do we need this?
        let host = match request.headers().typed_get::<Host>() {
            Some(host) => host.to_string(),
            None => {
                return Err("No host specified in your request".into());
            }
        };
        let url = match uri {
            AbsolutePath(ref path) => {
                let url_string = format!("http://{}{}", get_host_value(&host), path);
                match Url::parse(&url_string) {
                    Ok(url) => url,
                    Err(e) => return Err(format!("Couldn't parse requested URL: {}", e))
                }
            },
            AbsoluteUri(ref url) => {
                url.clone()
            },
            Authority(_) | Star => {
                return Err("Unsupported request URI".into());
            }
        };*/
        Ok(Request {
            app,
            request,
            url_rule: None,
            view_args: HashMap::new(),
            routing_redirect: None,
            routing_error: None,
            extensions_data: TypeMap::new(),
            args: LazyCell::new(),
            form: LazyCell::new(),
            files: LazyCell::new(),
            cached_json: LazyCell::new(),
        })
    }

    /// Get the url adapter for this request.
    pub fn url_adapter(&self) -> MapAdapter {
        self.app.url_map.bind(self.host(), self.path(), self.query_string(), self.method())
    }

    /// Match the request, set the `url_rule` and `view_args` field.
    pub fn match_request(&mut self) {
        let url_adapter = self.app.url_map.bind(self.host(), self.path(), self.query_string(), self.method());
        match url_adapter.matched() {
            MapAdapterMatched::MatchedRule((rule, view_args)) => {
                self.url_rule = Some(rule);
                self.view_args = view_args;
            },
            MapAdapterMatched::MatchedRedirect((redirect_url, redirect_code)) => {
                self.routing_redirect = Some((redirect_url, redirect_code));
            },
            MapAdapterMatched::MatchedError(routing_error) => {
                self.routing_error = Some(routing_error);
            },
        }
    }

    /// The endpoint that matched the request.
    pub fn endpoint(&self) -> Option<String> {
        match self.url_rule {
            Some(ref rule) => Some(rule.endpoint.clone()),
            None => None,
        }
    }

    /// The current module name.
    pub fn module_name(&self) -> Option<String> {
        if let Some(endpoint) = self.endpoint() {
            if endpoint.contains('.') {
                let v: Vec<&str> = endpoint.rsplitn(2, '.').collect();
                return Some(v[1].to_string());
            }
        }
        None
    }

    /// The parsed URL parameters.
    pub fn args(&self) -> &MultiDict<String> {
        if !self.args.filled() {
            let mut args = MultiDict::new();
            if let Some(query) = self.query_string() {
                let pairs = form_urlencoded::parse(query.as_bytes());
                for (k, v) in pairs.into_owned() {
                    args.add(k, v);
                }
            }
            self.args.fill(args).expect("This was checked to be empty!");
        }
        self.args.borrow().expect("This is checked to be always filled")
    }

    /// Parses the incoming JSON request data.
    pub async fn get_json(&mut self) -> &Option<serde_json::Value> {
        if !self.cached_json.filled() {
            let body = self.request.body_mut().by_ref();
            let body_bytes = helpers::load_body(body).await.expect("TODO");
            let rv = serde_json::from_slice(&body_bytes).ok();
            self.cached_json.fill(rv).expect("This was checked to be empty!");
        }
        self.cached_json.borrow().expect("This is checked to be always filled")
    }

    /// This method is used internally to retrieve submitted data.
    async fn load_form_data(&mut self) -> Result<(), formparser::Error> {
        if self.form.filled() {
            return Ok(())
        }
        let (form, files) = formparser::parse(&mut self.request).await?;
        self.form.fill(form).expect("This was checked to be empty!");
        self.files.fill(files).expect("This was checked to be empty!");
        Ok(())
    }

    /// The form parameters.
    pub async fn form(&mut self) -> &MultiDict<String> {
        self.load_form_data().await.expect("TODO");
        self.form.borrow().expect("This is always checked to be filled.")
    }

    /// All uploaded files.
    pub async fn files(&mut self) -> &MultiDict<Vec<u8>> {
        self.load_form_data().await.expect("TODO");
        self.files.borrow().expect("This is always checked to be filled.")
    }

    /// The headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.request.headers()
    }

    /// Requested path.
    pub fn path(&self) -> String {
        self.request.uri().path().to_owned()
    }

    /// Requested path including the query string.
    pub fn full_path(&self) -> String {
        self.request.uri().path_and_query().expect("TODO").to_string()
    }

    /// The host including the port if available.
    pub fn host(&self) -> String {
        self.request.headers().typed_get::<Host>().map(|h| h.to_string()).unwrap_or_default()
    }

    /// The query string.
    pub fn query_string(&self) -> Option<String> {
        self.request.uri().query().map(|q| q.to_owned())
    }

    /// The retrieved cookies.
    pub fn cookies(&self) -> Option<Cookie> {
        self.request.headers().typed_get::<Cookie>()
    }

    /// The request method.
    pub fn method(&self) -> Method {
        self.request.method().clone()
    }

    /// URL scheme (http or https)
    pub fn scheme(&self) -> String {
        String::from("http")
    }

    /// Just the host with scheme.
    pub fn host_url(&self) -> String {
        self.scheme() + "://" + &self.host() + "/"
    }

    /// The current url.
    pub fn url(&self) -> String {
        self.host_url() + &self.full_path().trim_start_matches('/')
    }

    /// The current url without the query string.
    pub fn base_url(&self) -> String {
        self.host_url() + &self.path().trim_start_matches('/')
    }

    /// Whether the request is secure (https).
    pub fn is_secure(&self) -> bool {
        self.scheme() == "https"
    }
}

impl<'r> fmt::Debug for Request<'r> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Pencil Request '{}' {}>", self.url(), self.method())
    }
}

/// The response body.
pub struct ResponseBody<'a>(Box<dyn Write + 'a>);

impl<'a> ResponseBody<'a> {
    /// Create a new ResponseBody.
    pub fn new<W: Write + 'a>(writer: W) -> ResponseBody<'a> {
        ResponseBody(Box::new(writer))
    }
}

impl<'a> Write for ResponseBody<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}


/// A trait which writes the body of one response.
pub trait BodyWrite: Send {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()>;
}

impl BodyWrite for Vec<u8> {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        body.write_all(self)
    }
}

impl<'a> BodyWrite for &'a [u8] {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        body.write_all(self)
    }
}

impl BodyWrite for String {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        self.as_bytes().write_body(body)
    }
}

impl<'a> BodyWrite for &'a str {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        self.as_bytes().write_body(body)
    }
}

impl BodyWrite for File {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        io::copy(self, body).map(|_| ())
    }
}

impl BodyWrite for Take<File> {
    fn write_body(&mut self, body: &mut ResponseBody) -> io::Result<()> {
        io::copy(self, body).map(|_| ())
    }
}



/// Response type.  It is just one container with a couple of parameters
/// (headers, body, status code etc).
pub struct Response {
    /// The HTTP Status code number
    pub status_code: u16,
    pub headers: HeaderMap,
    pub body: Option<Body>,
}

impl Response {
    /// Create a `Response`. By default, the status code is 200
    /// and content type is "text/html; charset=UTF-8".
    /// Remember to set content length if necessary.
    /// Mostly you should just get a response that is converted
    /// from other types, which set the content length automatically.
    /// For example:
    ///
    /// ```rust,ignore
    /// // Content length is set automatically
    /// let response = Response::from("Hello");
    /// ```
    pub fn new(body: Body) -> Response {
        let mut response = Response {
            status_code: 200,
            headers: HeaderMap::new(),
            body: Some(body),
        };
        let mime: Mime = "text/html; charset=UTF-8".parse().unwrap();
        response.headers.typed_insert(ContentType::from(mime));
        response
    }

    /// Create an empty response without body.
    pub fn new_empty() -> Response {
        Response {
            status_code: 200,
            headers: HeaderMap::new(),
            body: None,
        }
    }

    /// Get status name.
    pub fn status_name(&self) -> &str {
        match get_name_by_http_code(self.status_code) {
            Some(name) => name,
            None => "UNKNOWN",
        }
    }

    /// Returns the response content type if available.
    pub fn content_type(&self) -> Option<ContentType> {
        self.headers.typed_get::<ContentType>()
    }

    /// Set response content type.  If the mimetype passed is a
    /// mimetype starting with `text/` or something that needs a charset,
    /// the charset(UTF-8) parameter is appended to it.
    pub fn set_content_type(&mut self, mimetype: &str) {
        let mimetype = get_content_type(mimetype, "UTF-8");
        let mime: Mime = (&mimetype).parse().unwrap();
        self.headers.typed_insert(ContentType::from(mime));
    }

    /// Returns the response content length if available.
    pub fn content_length(&self) -> Option<usize> {
        self.headers.typed_get::<ContentLength>().map(|l| l.0 as usize)
    }

    /// Set content length.
    pub fn set_content_length(&mut self, value: usize) {
        self.headers.typed_insert(ContentLength(value as u64));
    }

    /// Sets cookie.
    pub fn set_cookie(&mut self, cookie: SetCookie) {
        self.headers.typed_insert(cookie);
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Pencil Response [{}]>", self.status_code)
    }
}

impl convert::From<Vec<u8>> for Response {
    /// Convert to response body.  The content length is set
    /// automatically.
    fn from(bytes: Vec<u8>) -> Response {
        let content_length = bytes.len();
        let mut response = Response::new(Body::from(bytes));
        response.set_content_length(content_length);
        response
    }
}

impl<'a> convert::From<&'a [u8]> for Response {
    /// Convert to response body.  The content length is set
    /// automatically.
    fn from(bytes: &'a [u8]) -> Response {
        bytes.to_vec().into()
    }
}

impl<'a> convert::From<&'a str> for Response {
    /// Convert to response body.  The content length is set
    /// automatically.
    fn from(s: &'a str) -> Response {
        s.to_owned().into()
    }
}

impl convert::From<String> for Response {
    /// Convert a new string to response body.  The content length is set
    /// automatically.
    fn from(s: String) -> Response {
        s.into_bytes().into()
    }
}

impl convert::From<File> for Response {
    /// Convert to response body.  The content length is set
    /// automatically if file size is available from metadata.
    fn from(mut f: File) -> Response {
        let content_length = match f.metadata() {
            Ok(metadata) => {
                Some(metadata.len())
            },
            Err(_) => None
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).expect("TODO"); // TODO this is blocking!
        let mut response = Response::new(Body::from(buf));
        if let Some(content_length) = content_length {
            response.set_content_length(content_length as usize);
        }
        response
    }
}
