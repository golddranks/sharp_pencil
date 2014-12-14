// This module implements the central application object.
// Copyright (c) 2014 by Shipeng Feng.
// Licensed under the BSD License, see LICENSE for more details.

use std::collections::HashMap;
use std::error::Error;
use std::io::File;

use http::server::request::RequestUri::AbsolutePath;

use types::{
    PencilValue,
        PenString,
        PenResponse,

    PencilError,
        PenHTTPError,
        PenUserError,

    PencilResult,
    ViewFunc,
};
use wrappers::{
    Request,
    Response,
};
use helpers::PathBound;
use helpers;
use config;
use logging;
use serving::run_server;
use routing::{Map, Rule};
use testing::PencilClient;
use errors::{HTTPError, InternalServerError};


/// The pencil type.
#[deriving(Clone)]
pub struct Pencil {
    pub root_path: String,
    pub static_folder: String,
    pub static_url_path: String,
    pub config: config::Config,
    pub url_map: Map,
    // A dictionary of all view functions registered.
    pub view_functions: HashMap<String, ViewFunc>,
    pub before_request_funcs: Vec<String>,
    pub after_request_funcs: Vec<String>,
    pub teardown_request_funcs: Vec<String>,
    pub error_handlers: HashMap<&'static str, PencilResult>,
}

/// The pencil object acts as the central application object.
impl Pencil {
    /// Create a new pencil object.
    pub fn new(root_path: &str) -> Pencil {
        Pencil {
            root_path: root_path.to_string(),
            static_folder: String::from_str("static"),
            static_url_path: String::from_str("/static"),
            config: config::Config::new(),
            url_map: Map::new(),
            view_functions: HashMap::new(),
            before_request_funcs: vec![],
            after_request_funcs: vec![String::from_str("after")],
            teardown_request_funcs: vec![],
            error_handlers: HashMap::new(),
        }
    }

    /// Set global log level based on the application's debug flag.
    pub fn set_log_level(&self) {
        logging::set_log_level(self);
    }

    /// A shortcut that is used to register a view function for a given
    /// URL rule.
    pub fn route(&mut self, rule: &'static str, methods: &[&str], endpoint: &str, view_func: ViewFunc) {
        self.add_url_rule(rule, methods, endpoint, view_func);
    }

    /// Connects a URL rule.
    pub fn add_url_rule(&mut self, rule: &'static str, methods: &[&str], endpoint: &str, view_func: ViewFunc) {
        let url_rule = Rule::new(rule, methods, endpoint);
        self.url_map.add(url_rule);
        self.view_functions.insert(endpoint.to_string(), view_func);
    }

    /// Registers a function to run before each request.
    pub fn before_request(&mut self, f: String) {
        self.before_request_funcs.push(f);
    }

    /// Registers a function to run after each request.  Your function
    /// must take a response object and modify it.
    pub fn after_request(&mut self, f: String) {
        self.after_request_funcs.push(f);
    }

    /// Registers a function to run at the end of each request,
    /// regardless of whether there was an error or not.
    pub fn teardown_request(&mut self, f: String) {
        self.teardown_request_funcs.push(f);
    }

    /// Registers a function as one error handler.
    pub fn register_error_handler(&mut self, error_desc: &'static str, f: PencilResult) {
        // TODO: seperate http code and others
        self.error_handlers.insert(error_desc, f);
    }

    /// Creates a test client for this application, you can use it
    /// like this:
    ///
    /// ```ignore
    /// let client = app.test_client();
    /// let response = client.get('/');
    /// assert!(response.code, 200);
    /// ```
    pub fn test_client(&self) -> PencilClient {
        PencilClient::new(self)
    }

    /// Called before the actual request dispatching, you can return value
    /// from here and stop the further request handling.
    fn preprocess_request(&self) {
        for x in self.before_request_funcs.iter() {
            println!("{}", x);
        }
    }

    /// Does the request dispatching.  Matches the URL and returns the return
    /// value of the view.
    fn dispatch_request(&self, request: Request) -> PencilResult {
        let request_url = match request.request_uri {
            AbsolutePath(ref url) => {
                println!("{}", url);
                url.clone()
            },
            _ => {
                println!("{}", "WTF!");
                "wtf".to_string()
            },
        };
        let url_adapter = self.url_map.bind(request_url, String::from_str("GET"));
        let rv = match url_adapter.captures() {
            Ok(caps) => {
                let (rule, params) = caps;
                for p in params.iter() {
                    println!("{}", p);
                }
                match self.view_functions.get(&rule.endpoint) {
                    Some(&view_func) => view_func(request, params),
                    None => Ok(PenString(String::from_str("No such handler"))),
                }
            },
            _ => Ok((PenString(String::from_str("404")))),
        };
        return rv;
    }

    /// Converts the return value from a view function to a real
    /// response object.
    fn make_response(&self, rv: PencilValue) -> Response {
        return helpers::make_response(rv);
    }

    /// Modify the response object before it's sent to the HTTP server.
    fn process_response(&self, response: &mut Response) {
        // TODO: reverse order
        for x in self.after_request_funcs.iter() {
            response.body.push_str(x.as_slice());
        }
    }

    /// Called after the actual request dispatching.
    pub fn do_teardown_request(&self) {
        // TODO: reverse order
        for x in self.teardown_request_funcs.iter() {
            println!("{}", x);
        }
    }

    /// This method is called whenever an error occurs that should be handled.
    fn handle_user_error(&self, e: PencilError) -> PencilResult {
        match e {
            PenHTTPError(e) => self.handle_http_error(e),
            PenUserError(e) => match self.error_handlers.get(e.description()) {
                Some(handler) => handler.clone(),
                None => Err(PenUserError(e)),
            }
        }
    }

    /// Handles an HTTP error.
    fn handle_http_error(&self, e: HTTPError) -> PencilResult {
        match self.error_handlers.get(e.description()) {
            Some(handler) => handler.clone(),
            None => Ok(PenResponse(e.to_response())),
        }
    }

    /// Default error handing that kicks in when an error occurs that is not
    /// handled.
    fn handle_error(&self, e: PencilError) -> PencilValue {
        self.log_error(&e);
        match self.error_handlers.get(e.description()) {
            Some(handler) => {
                match handler.clone() {
                    Ok(value) => value,
                    Err(_) => {
                        let e = InternalServerError;
                        PenResponse(e.to_response())
                    }
                }
            },
            None => {
                let e = InternalServerError;
                PenResponse(e.to_response())
            }
        }
    }

    /// Logs an error.
    fn log_error(&self, e: &PencilError) {
        println!("{}", e.description());
    }

    /// Dispatches the request and performs request pre and postprocessing
    /// as well as HTTP error handling.
    fn full_dispatch_request(&self, request: Request) -> Result<Response, PencilError> {
        self.preprocess_request();
        let rv = match self.dispatch_request(request) {
            Ok(value) => Ok(value),
            Err(e) => self.handle_user_error(e),
        };
        match rv {
            Ok(value) => {
                let mut response = self.make_response(value);
                self.process_response(&mut response);
                Ok(response)
            },
            Err(e) => Err(e),
        }
    }

    /// The actual application.  Middlewares can be applied here.
    /// You can do this:
    ///     application.app = MyMiddleware(application.app)
    pub fn handle_request(&self, request: Request) -> Response {
        // let url_adapter = self.create_url_adapter(request);
        // request.url_rule, request.view_args = url_adapter.match()
        // or
        // request.routing_error = e
        let response = match self.full_dispatch_request(request) {
            Ok(response) => response,
            Err(e) => self.make_response(self.handle_error(e)),
        };
        return response;
    }

    /// Runs the application on a local development server.
    pub fn run(self) {
        run_server(self);
    }
}

impl PathBound for Pencil {
    fn open_resource(&self, resource: &str) -> File {
        let mut path = Path::new(self.root_path.as_slice());
        path.push(resource);
        return File::open(&path).unwrap();
    }
}
