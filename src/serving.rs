//! This module implements the http server support for our application.

use std::{net::ToSocketAddrs, path::PathBuf, sync::Arc, sync::RwLock};
use std::fmt::Write;

use handlebars::{Handlebars, TemplateFileError};
use hyper::server::Server;
use notify::{Error as NotifyError, Event, EventFn, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use Pencil;

fn handle_modify(paths: &[PathBuf], registry: &RwLock<Handlebars<'_>>) {
    for path in paths {
        if let Some(fname) = path.file_name() {
            let mut write_guard = registry.write().unwrap();
            let fname = fname.to_str().unwrap();
            let compile_result = write_guard.register_template_file(fname, &path);
            if let Err(TemplateFileError::TemplateError(err)) = compile_result {
                let mut msg = "Template syntax error:".to_owned();
                if let Some(line_no) = err.line_no {
                    write!(msg, " line no: {}", line_no).unwrap();
                }
                if let Some(col_no) = err.column_no {
                    write!(msg, " col no: {}", col_no).unwrap();
                }
                write!(msg, " cause: {}", err.reason).unwrap();
                warn!("{}", msg);
                write_guard.register_template_string(fname, msg).unwrap();
            } else {
                info!("Registered template {}", fname);
            }
        }
    }
}

fn watch_files(registry: Arc<RwLock<Handlebars<'static>>>) -> impl EventFn {
    move |event: Result<Event, NotifyError>| match event {
        Ok(Event { kind: EventKind::Create(_), paths, ..}) => handle_modify(&paths, &registry),
        Ok(Event { kind: EventKind::Modify(_), paths, ..}) => handle_modify(&paths, &registry),
        Err(e) => warn!("template watch error: {:?}", e),
        _ => (),
    }
}


/// Run the `Pencil` application.
pub fn run_server<A: ToSocketAddrs>(application: Pencil, addr: A, threads: usize) {

    let mut watcher: RecommendedWatcher;
    if application.template_debug {
        let registry = application.handlebars_registry.clone();
        let template_dir = application.template_folder.clone();
        watcher = Watcher::new_immediate(watch_files(registry)).unwrap();
        info!("Begin watching {}", &template_dir);
        watcher.watch(&template_dir, RecursiveMode::Recursive).unwrap();
    }

    let server = Server::http(addr).unwrap();
    let _guard = server.handle_threads(application, threads).unwrap();
}
