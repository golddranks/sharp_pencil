//! Modules are the recommended way to implement larger or more
//! pluggable applications.

use std::collections::HashMap;
use std::mem;
use std::path::PathBuf;

use headers::{HeaderMapExt, Range};
use hyper::Method;

use crate::http_errors::NotFound;
use crate::app::Pencil;
use crate::routing::Matcher;
use crate::types::ViewFunc;
use crate::types::{PencilResult, PencilError, HTTPError, UserError};
use crate::types::{BeforeRequestFunc, AfterRequestFunc, TeardownRequestFunc};
use crate::types::{HTTPErrorHandler, UserErrorHandler};
use crate::helpers::send_from_directory_range;
use crate::wrappers::{Request, Response};


/// Represents a module.
pub struct Module {
    /// The name of the module.
    pub name: String,
    /// The path where your module locates.
    pub root_path: String,
    /// The folder with static files that should be served at `static_url_path`.
    pub static_folder: Option<String>,
    /// The url path for the static files on the web.
    pub static_url_path: Option<String>,
    /// The folder that contains the templates that should be used for the module.
    pub template_folder: Option<String>,
    #[doc(hidden)]
    pub before_request_funcs: Vec<Box<BeforeRequestFunc>>,
    #[doc(hidden)]
    pub after_request_funcs: Vec<Box<AfterRequestFunc>>,
    #[doc(hidden)]
    pub teardown_request_funcs: Vec<Box<TeardownRequestFunc>>,
    #[doc(hidden)]
    pub http_error_handlers: HashMap<u16, Box<HTTPErrorHandler>>,
    #[doc(hidden)]
    pub user_error_handlers: HashMap<String, Box<UserErrorHandler>>,
    deferred_functions: Vec<Box<dyn Fn(&mut Pencil) + Send + Sync>>,
    deferred_routes: Vec<(Matcher, Vec<Method>, String, ViewFunc)>,
}

use std::fmt;

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Module {{ name: {}, root_path: {}, static_folder: {:?}, static_url_path: {:?}, template_folder: {:?} }}",
            self.name, self.root_path, self.static_folder, self.static_url_path, self.template_folder)
    }
}

impl Module {
    pub fn new(name: &str, root_path: &str) -> Module {
        Module {
            name: name.to_string(),
            root_path: root_path.to_string(),
            static_folder: None,
            static_url_path: None,
            template_folder: None,
            before_request_funcs: Vec::new(),
            after_request_funcs: Vec::new(),
            teardown_request_funcs: Vec::new(),
            http_error_handlers: HashMap::new(),
            user_error_handlers: HashMap::new(),
            deferred_functions: Vec::new(),
            deferred_routes: Vec::new(),
        }
    }

    fn record<F: Fn(&mut Pencil) + Send + Sync + 'static>(&mut self, f: F) {
        self.deferred_functions.push(Box::new(f));
    }

    /// The endpoint is automatically prefixed with the module's name.
    pub fn route<M: Into<Matcher>, N: AsRef<[Method]>>(&mut self, rule: M, methods: N, endpoint: &str, view_func: ViewFunc) {
        let mut methods_vec: Vec<Method> = Vec::new();
        methods_vec.extend(methods.as_ref().iter().cloned());
        if endpoint.contains('.') {
            panic!("Module endpoint should not contain dot");
        }
        let endpoint = format!("{}.{}", self.name, endpoint);
        self.deferred_routes.push((rule.into(), methods_vec, endpoint, view_func));
    }

    /// Before request for a module.  This is only executed before each request
    /// that is handled by a view function of that module.
    pub fn before_request<F: Fn(&mut Request) -> Option<PencilResult> + Send + Sync + 'static>(&mut self, f: F) {
        self.before_request_funcs.push(Box::new(f));
    }

    /// Before request for the app that this module is registered on.  This is
    /// executed before each request, even if outside of a module.
    pub fn before_app_request<F: Fn(&mut Request) -> Option<PencilResult> + Send + Sync + Clone + 'static>(&mut self, f: F) {
        self.record(move |app| app.before_request(f.clone())); // FIXME Clone can be removed when Box<FnOnce> is supported
    }

    /// After request for a module.  This is only executed after each request
    /// that is handled by a view function of that module.
    pub fn after_request<F: Fn(&Request, &mut Response) + Send + Sync + 'static>(&mut self, f: F) {
        self.after_request_funcs.push(Box::new(f));
    }

    /// After request for the app that this module is registered on.  This is
    /// executed after each request, even if outside of a module.
    pub fn after_app_request<F: Fn(&Request, &mut Response) + Send + Sync + Clone + 'static>(&mut self, f: F) {
        self.record(move |app| app.after_request(f.clone())); // FIXME Clone can be removed when Box<FnOnce> is supported
    }
 
    /// Teardown request for a module.  This is only executed when tearing down
    /// each request that is handled by a view function of that module.
    pub fn teardown_request<F: Fn(Option<&PencilError>) + Send + Sync + 'static>(&mut self, f: F) {
        self.teardown_request_funcs.push(Box::new(f));
    }

    /// Teardown request for the app that this module is registered on.  This is
    /// executed when tearing down each request, even if outside of a module.
    pub fn teardown_app_request<F: Fn(Option<&PencilError>) + Send + Sync + Clone + 'static>(&mut self, f: F) {
        self.record(move |app| app.teardown_request(f.clone())); // FIXME Clone can be removed when Box<FnOnce> is supported
    }

    /// Registers a http error handler that becomes active for this module only.
    pub fn httperrorhandler<F: Fn(HTTPError) -> PencilResult + Send + Sync + 'static>(&mut self, status_code: u16, f: F) {
        self.http_error_handlers.insert(status_code, Box::new(f));
    }

    /// Registers an user error handler that becomes active for this module only.
    pub fn usererrorhandler<F: Fn(UserError) -> PencilResult + Send + Sync + 'static>(&mut self, error_desc: &str, f: F) {
        self.user_error_handlers.insert(error_desc.to_string(), Box::new(f));
    }

    /// Registers a http error handler for all requests of the application.
    pub fn app_httperrorhandler<F: Fn(HTTPError) -> PencilResult + Send + Sync + Clone + 'static>(&mut self, status_code: u16, f: F) {
        self.record(move |app| app.httperrorhandler(status_code, f.clone()));
    }

    /// Registers an user error handler for all requests of the application.
    pub fn app_usererrorhandler<F: Fn(UserError) -> PencilResult + Send + Sync + Clone + 'static>(&mut self, error_desc: &str, f: F) {
        let desc = error_desc.to_string();
        self.record(move |app| app.register_user_error_handler(&desc, f.clone()));
    }

    /// Register this module.
    pub fn register(mut self, app: &mut Pencil) {
        if app.modules.contains_key(&self.name) {
            panic!("A module that is named {} already exists, name collision occurred.", self.name);
        }

        let static_url_path = match self.static_folder {
            Some(_) => {
                match self.static_url_path {
                    Some(ref static_url_path) => Some(static_url_path.clone()),
                    None => None,
                }
            },
            None => None
        };
        if let Some(static_url_path) = static_url_path {
            let mut rule = static_url_path.clone();
            rule = rule + "/<filename:path>";
            self.route(rule, &[Method::GET], "static", send_module_static_file);
        }
        let deferred_routes = mem::replace(&mut self.deferred_routes, Vec::new());
        for (matcher, methods, endpoint, view_func) in deferred_routes {
            app.add_url_rule(matcher, methods.as_ref(), &endpoint, view_func);
        }
        let deferred_functions = mem::replace(&mut self.deferred_functions, Vec::new());
        for deferred in deferred_functions {
            deferred(app);
        }

        app.modules.insert(self.name.clone(), self);
    }
}

/// View function used internally to send static files from the static folder
/// to the browser.
fn send_module_static_file(request: &mut Request) -> PencilResult {
    if let Some(module_name) = request.module_name() {
        if let Some(module) = request.app.modules.get(&module_name) {
            if let Some(ref module_static_folder) = module.static_folder {
                let mut static_path = PathBuf::from(&module.root_path);
                static_path.push(module_static_folder);
                let static_path_str = static_path.to_str().unwrap();
                let filename = request.view_args.get("filename").unwrap();
                let range = request.headers().typed_get::<Range>();
                return send_from_directory_range(static_path_str, filename, false, range.as_ref());
            }
        }
    }
    Err(NotFound.into())
}
