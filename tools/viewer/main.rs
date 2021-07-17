/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use sixtyfps_interpreter::ComponentInstance;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Wake;
use std::time::Duration;
use structopt::StructOpt;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(StructOpt, Clone)]
struct Cli {
    #[structopt(short = "I", name = "include path for other .60 files", number_of_values = 1)]
    include_paths: Vec<std::path::PathBuf>,

    #[structopt(name = "path to .60 file", parse(from_os_str))]
    path: std::path::PathBuf,

    /// The style name ('native', or 'ulgy')
    #[structopt(long, name = "style name", default_value)]
    style: String,

    /// The rendering backend
    #[structopt(long, name = "backend", default_value)]
    backend: String,

    /// Automatically watch the file system, and reload when it changes
    #[structopt(long)]
    auto_reload: bool,
}

thread_local! {static CURRENT_INSTANCE: std::cell::RefCell<Option<ComponentInstance>> = Default::default();}

fn main() -> Result<()> {
    let args = Cli::from_args();

    if !args.backend.is_empty() {
        std::env::set_var("SIXTYFPS_BACKEND", &args.backend);
    }

    let fswatcher = if args.auto_reload { Some(start_fswatch_thread(args.clone())?) } else { None };
    let mut compiler = init_compiler(&args, fswatcher);

    let c = spin_on::spin_on(compiler.build_from_path(args.path));
    sixtyfps_interpreter::print_diagnostics(&compiler.diagnostics());

    let c = match c {
        Some(c) => c,
        None => std::process::exit(-1),
    };

    let component = c.create();

    if args.auto_reload {
        CURRENT_INSTANCE.with(|current| current.replace(Some(component.clone_strong())));
    }

    component.run();
    Ok(())
}

fn init_compiler(
    args: &Cli,
    fswatcher: Option<Arc<Mutex<notify::RecommendedWatcher>>>,
) -> sixtyfps_interpreter::ComponentCompiler {
    let mut compiler = sixtyfps_interpreter::ComponentCompiler::default();
    compiler.set_include_paths(args.include_paths.clone());
    if !args.style.is_empty() {
        compiler.set_style(args.style.clone());
    }
    if let Some(watcher) = fswatcher {
        notify::Watcher::watch(
            &mut *watcher.lock().unwrap(),
            &args.path,
            notify::RecursiveMode::NonRecursive,
        )
        .unwrap_or_else(|err| {
            eprintln!("Warning: error while watching {}: {:?}", args.path.display(), err)
        });
        compiler.set_file_loader(move |path| {
            notify::Watcher::watch(
                &mut *watcher.lock().unwrap(),
                &path,
                notify::RecursiveMode::NonRecursive,
            )
            .unwrap_or_else(|err| {
                eprintln!("Warning: error while watching {}: {:?}", path.display(), err)
            });
            Box::pin(async { None })
        })
    }
    compiler
}

const PENDING_EVENTS: AtomicU32 = AtomicU32::new(0);

fn start_fswatch_thread(args: Cli) -> Result<Arc<Mutex<notify::RecommendedWatcher>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let w = Arc::new(Mutex::new(notify::watcher(tx, Duration::from_millis(400))?));
    let w2 = w.clone();
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            use notify::DebouncedEvent::*;
            if matches!(event, Write(_) | Remove(_) | Create(_)) {
                if PENDING_EVENTS.load(Ordering::SeqCst) == 0 {
                    PENDING_EVENTS.fetch_add(1, Ordering::SeqCst);
                    run_in_ui_thread(Box::pin(reload(args.clone(), w2.clone())));
                }
            }
        }
    });
    Ok(w)
}

async fn reload(args: Cli, fswatcher: Arc<Mutex<notify::RecommendedWatcher>>) {
    let mut compiler = init_compiler(&args, Some(fswatcher));
    let c = compiler.build_from_path(&args.path).await;
    sixtyfps_interpreter::print_diagnostics(&compiler.diagnostics());

    if let Some(c) = c {
        CURRENT_INSTANCE.with(|current| {
            let mut current = current.borrow_mut();
            if let Some(handle) = current.take() {
                let window = handle.window();
                current.replace(c.create_with_existing_window(window));
            } else {
                let handle = c.create();
                handle.show();
                current.replace(handle);
            }
            eprintln!("Successful reload of {}", args.path.display());
        });
    }

    PENDING_EVENTS.fetch_sub(1, Ordering::SeqCst);
}

/// This type is duplicated with lsp/preview.rs
struct FutureRunner {
    fut: Mutex<Option<Pin<Box<dyn Future<Output = ()>>>>>,
}

/// Safety: the future is only going to be run in the UI thread
unsafe impl Send for FutureRunner {}
/// Safety: the future is only going to be run in the UI thread
unsafe impl Sync for FutureRunner {}

impl Wake for FutureRunner {
    fn wake(self: Arc<Self>) {
        sixtyfps_rendering_backend_default::backend().post_event(Box::new(move || {
            let waker = self.clone().into();
            let mut cx = std::task::Context::from_waker(&waker);
            let mut fut_opt = self.fut.lock().unwrap();
            if let Some(fut) = &mut *fut_opt {
                match fut.as_mut().poll(&mut cx) {
                    std::task::Poll::Ready(_) => *fut_opt = None,
                    std::task::Poll::Pending => {}
                }
            }
        }));
    }
}

fn run_in_ui_thread(fut: Pin<Box<dyn Future<Output = ()>>>) {
    Arc::new(FutureRunner { fut: Mutex::new(Some(fut)) }).wake()
}
