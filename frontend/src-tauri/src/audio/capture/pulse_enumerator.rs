// Persistent, single-threaded PulseAudio enumerator (TECH-02).
//
// Instead of opening and destroying two fresh libpulse connections every
// 2 seconds for the entire duration of a meeting, this module owns exactly
// one `Mainloop` + `Context` pair on a dedicated OS thread named `pulse-enum`.
// All device enumeration requests (`list_sinks`, `list_sources`,
// `default_source_name`) are sent to that thread, executed sequentially, and
// answered back on a bounded-time channel.
//
// This is the only structurally correct way to share a libpulse mainloop:
// `libpulse_binding::mainloop::standard::Mainloop` contains an `Rc`, so it is
// `!Send` and `!Sync`. A single owner thread + message passing therefore both
// satisfies the libpulse threading contract and removes the continuous
// connect/disconnect churn that multiplied the crash surface observed in
// TECH-02.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::introspect::ServerInfo;
use libpulse_binding::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::operation::{Operation, State as OperationState};
use libpulse_binding::proplist::Proplist;
use log::{debug, info, warn};

use super::pulse_linux::{PulseSink, PulseSource};

/// Upper bound on any single mainloop pumping loop. A local introspection
/// round-trip answers in milliseconds; this is a safety ceiling against a
/// server that accepts the connection and then never answers (which would
/// otherwise wedge the enumerator thread forever), not an expected duration.
const PULSE_OP_TIMEOUT: Duration = Duration::from_secs(5);

/// Time the caller waits for a response from the enumerator thread. Slightly
/// larger than `PULSE_OP_TIMEOUT` so a slow-but-successful operation is not
/// aborted on the caller side.
const CALLER_TIMEOUT: Duration = Duration::from_secs(7);

/// Requests the enumerator thread can serve. One variant per introspection
/// call we actually need — deliberately not a generic "run this closure",
/// because closures over libpulse types are not `Send`.
enum Request {
    ListSinks(mpsc::Sender<Result<Vec<PulseSink>>>),
    ListSources(mpsc::Sender<Result<Vec<PulseSource>>>),
    DefaultSourceName(mpsc::Sender<Result<Option<String>>>),
    DefaultSinkName(mpsc::Sender<Result<Option<String>>>),
}

/// Sender to the single enumerator thread. Created lazily on first use and
/// kept for the lifetime of the process.
static ENUMERATOR: OnceLock<Mutex<mpsc::Sender<Request>>> = OnceLock::new();

/// Return a clone of the global request sender, spawning the enumerator thread
/// on first call.
fn get_sender() -> Result<mpsc::Sender<Request>> {
    // Fast path: the thread is already running.
    if let Some(lock) = ENUMERATOR.get() {
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        return Ok(guard.clone());
    }

    // Slow path: create the channel and thread, then race to publish. If another
    // thread publishes first, the thread we just spawned will simply exit when
    // its duplicate sender is dropped.
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("pulse-enum".into())
        .spawn(move || enumerator_thread(rx))
        .map_err(|e| anyhow!("Failed to spawn pulse-enum thread: {}", e))?;
    let _ = ENUMERATOR.set(Mutex::new(tx));

    let lock = ENUMERATOR
        .get()
        .expect("ENUMERATOR was just set above, or by another thread");
    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    Ok(guard.clone())
}

fn enumerator_thread(receiver: mpsc::Receiver<Request>) {
    let mut connection: Option<(Mainloop, Context)> = None;

    for request in receiver {
        // Reconnect lazily if we have no connection or if the existing one died.
        let mut needs_connect = connection.is_none();
        if let Some((_, ref context)) = connection {
            if context.get_state() != ContextState::Ready {
                warn!(
                    "pulse_enumerator: context state is {:?}, reconnecting",
                    context.get_state()
                );
                needs_connect = true;
            }
        }

        if needs_connect {
            // Explicit drop order: Context must be destroyed before Mainloop.
            // `pa_context` keeps a raw pointer into the `pa_mainloop_api` that
            // lives inside `pa_mainloop`; dropping the mainloop first would be
            // a use-after-free.
            if let Some((mainloop, context)) = connection.take() {
                drop(context);
                drop(mainloop);
            }

            match connect() {
                Ok(conn) => {
                    info!("pulse_enumerator: connected to PulseAudio/PipeWire server");
                    connection = Some(conn);
                }
                Err(e) => {
                    warn!("pulse_enumerator: failed to connect: {}", e);
                    send_error(request, e);
                    continue;
                }
            }
        }

        // `connection` is guaranteed `Some` and `Ready` here.
        let (ref mut mainloop, ref context) = connection.as_mut().expect("connection set above");

        match request {
            Request::ListSinks(reply) => {
                let result = list_sinks_impl(mainloop, context);
                let _ = reply.send(result);
            }
            Request::ListSources(reply) => {
                let result = list_sources_impl(mainloop, context);
                let _ = reply.send(result);
            }
            Request::DefaultSourceName(reply) => {
                let result = default_name_impl(mainloop, context, |info| {
                    info.default_source_name.as_deref().map(str::to_string)
                });
                let _ = reply.send(result);
            }
            Request::DefaultSinkName(reply) => {
                let result = default_name_impl(mainloop, context, |info| {
                    info.default_sink_name.as_deref().map(str::to_string)
                });
                let _ = reply.send(result);
            }
        }
    }

    // Explicit drop order on thread exit as well.
    if let Some((mainloop, context)) = connection.take() {
        drop(context);
        drop(mainloop);
    }
}

/// Helper for early connection failures where we still need to reply to the
/// caller with the error.
fn send_error(request: Request, error: anyhow::Error) {
    match request {
        Request::ListSinks(reply) => {
            let _ = reply.send(Err(error));
        }
        Request::ListSources(reply) => {
            let _ = reply.send(Err(error));
        }
        Request::DefaultSourceName(reply) | Request::DefaultSinkName(reply) => {
            let _ = reply.send(Err(error));
        }
    }
}

/// Connect a context to the default PulseAudio/PipeWire server, blocking until
/// it's ready or the safety timeout fires.
fn connect() -> Result<(Mainloop, Context)> {
    let mut mainloop =
        Mainloop::new().ok_or_else(|| anyhow!("Failed to create PulseAudio mainloop"))?;

    let proplist =
        Proplist::new().ok_or_else(|| anyhow!("Failed to create PulseAudio proplist"))?;
    let mut context = Context::new_with_proplist(&mainloop, "Meetily", &proplist)
        .ok_or_else(|| anyhow!("Failed to create PulseAudio context"))?;

    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|e| anyhow!("Failed to connect to PulseAudio/PipeWire server: {}", e))?;

    let deadline = Instant::now() + PULSE_OP_TIMEOUT;
    loop {
        if Instant::now() > deadline {
            return Err(anyhow!(
                "PulseAudio/PipeWire connection timed out after {:?}",
                PULSE_OP_TIMEOUT
            ));
        }

        // `iterate(true)` blocks until an event arrives. The deadline is only
        // checked between iterations, so a server that accepts the connection
        // and then never sends any event could still wedge us for the duration
        // of one iteration. In practice the server emits connection-state
        // events, so this simple approach avoids busy-waiting on a thread that
        // lives for the whole process lifetime.
        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                return Err(anyhow!(
                    "PulseAudio mainloop iteration failed while connecting"
                ));
            }
            IterateResult::Success(_) => {}
        }

        match context.get_state() {
            ContextState::Ready => break,
            ContextState::Failed | ContextState::Terminated => {
                return Err(anyhow!(
                    "PulseAudio/PipeWire context connection failed or was terminated"
                ));
            }
            _ => {}
        }
    }

    Ok((mainloop, context))
}

/// Pump the mainloop until `operation` finishes, with a safety timeout.
fn run_operation_to_completion<T: ?Sized>(
    mainloop: &mut Mainloop,
    operation: &Operation<T>,
) -> Result<()> {
    let deadline = Instant::now() + PULSE_OP_TIMEOUT;
    loop {
        if Instant::now() > deadline {
            return Err(anyhow!(
                "PulseAudio/PipeWire operation timed out after {:?}",
                PULSE_OP_TIMEOUT
            ));
        }

        // Same timeout caveat as `connect()`: deadline is checked between
        // blocking iterations only.
        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                return Err(anyhow!("PulseAudio mainloop iteration failed"));
            }
            IterateResult::Success(_) => {}
        }

        match operation.get_state() {
            OperationState::Done => return Ok(()),
            OperationState::Cancelled => {
                return Err(anyhow!("PulseAudio operation was cancelled"));
            }
            OperationState::Running => continue,
        }
    }
}

fn list_sinks_impl(mainloop: &mut Mainloop, context: &Context) -> Result<Vec<PulseSink>> {
    let sinks: Rc<RefCell<Vec<PulseSink>>> = Rc::new(RefCell::new(Vec::new()));
    let sinks_cb = sinks.clone();

    let operation = context.introspect().get_sink_info_list(move |result| {
        if let ListResult::Item(info) = result {
            let monitor_source_name = info.monitor_source_name.as_deref().unwrap_or_default();
            if monitor_source_name.is_empty() {
                return;
            }

            let description = info
                .description
                .as_deref()
                .unwrap_or("Unknown output")
                .to_string();

            sinks_cb.borrow_mut().push(PulseSink {
                name: info.name.as_deref().unwrap_or_default().to_string(),
                description,
                monitor_source_name: monitor_source_name.to_string(),
            });
        }
    });

    run_operation_to_completion(mainloop, &operation)?;
    drop(operation);

    let result = sinks.borrow().clone();
    Ok(result)
}

fn list_sources_impl(mainloop: &mut Mainloop, context: &Context) -> Result<Vec<PulseSource>> {
    let sources: Rc<RefCell<Vec<PulseSource>>> = Rc::new(RefCell::new(Vec::new()));
    let sources_cb = sources.clone();

    let operation = context.introspect().get_source_info_list(move |result| {
        if let ListResult::Item(info) = result {
            // Monitors of sinks are already exposed as "System Audio" devices
            // through list_sinks(); ignore them here.
            if info.monitor_of_sink.is_some() {
                return;
            }

            let source_name = info.name.as_deref().unwrap_or_default();
            if source_name.is_empty() {
                return;
            }

            let description = info
                .description
                .as_deref()
                .unwrap_or(source_name)
                .to_string();

            sources_cb.borrow_mut().push(PulseSource {
                description,
                source_name: source_name.to_string(),
            });
        }
    });

    run_operation_to_completion(mainloop, &operation)?;
    drop(operation);

    let result = sources.borrow().clone();
    Ok(result)
}

fn default_name_impl(
    mainloop: &mut Mainloop,
    context: &Context,
    pick: impl Fn(&ServerInfo) -> Option<String> + 'static,
) -> Result<Option<String>> {
    let default_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let default_name_cb = default_name.clone();

    let operation = context.introspect().get_server_info(move |info: &ServerInfo| {
        *default_name_cb.borrow_mut() = pick(info);
    });

    run_operation_to_completion(mainloop, &operation)?;
    drop(operation);

    let result = default_name.borrow().clone();
    Ok(result)
}

// ---------------------------------------------------------------------------
// Public facades — signatures intentionally identical to the previous free
// functions in `pulse_linux.rs` so no call site needs to change.
// ---------------------------------------------------------------------------

pub fn list_sinks() -> Result<Vec<PulseSink>> {
    let (tx, rx) = mpsc::channel();
    get_sender()?
        .send(Request::ListSinks(tx))
        .map_err(|e| anyhow!("PulseAudio enumerator request failed: {}", e))?;

    let result = rx
        .recv_timeout(CALLER_TIMEOUT)
        .map_err(|e| match e {
            RecvTimeoutError::Timeout => anyhow!(
                "PulseAudio enumerator response timed out after {:?}",
                CALLER_TIMEOUT
            ),
            RecvTimeoutError::Disconnected => {
                anyhow!("PulseAudio enumerator thread disconnected")
            }
        })?;

    debug!("pulse_linux::list_sinks: got {} sink(s)", result.as_ref().map(|v| v.len()).unwrap_or(0));
    result
}

pub fn list_sources() -> Result<Vec<PulseSource>> {
    let (tx, rx) = mpsc::channel();
    get_sender()?
        .send(Request::ListSources(tx))
        .map_err(|e| anyhow!("PulseAudio enumerator request failed: {}", e))?;

    let result = rx
        .recv_timeout(CALLER_TIMEOUT)
        .map_err(|e| match e {
            RecvTimeoutError::Timeout => anyhow!(
                "PulseAudio enumerator response timed out after {:?}",
                CALLER_TIMEOUT
            ),
            RecvTimeoutError::Disconnected => {
                anyhow!("PulseAudio enumerator thread disconnected")
            }
        })?;

    debug!(
        "pulse_linux::list_sources: got {} source(s)",
        result.as_ref().map(|v| v.len()).unwrap_or(0)
    );
    result
}

fn request_default_name(
    make: impl FnOnce(mpsc::Sender<Result<Option<String>>>) -> Request,
) -> Result<Option<String>> {
    let (tx, rx) = mpsc::channel();
    get_sender()?
        .send(make(tx))
        .map_err(|e| anyhow!("PulseAudio enumerator request failed: {}", e))?;

    rx.recv_timeout(CALLER_TIMEOUT)
        .map_err(|e| match e {
            RecvTimeoutError::Timeout => anyhow!(
                "PulseAudio enumerator response timed out after {:?}",
                CALLER_TIMEOUT
            ),
            RecvTimeoutError::Disconnected => {
                anyhow!("PulseAudio enumerator thread disconnected")
            }
        })
        .and_then(|r| r)
}

pub fn default_source_name() -> Result<Option<String>> {
    request_default_name(Request::DefaultSourceName)
}

pub fn default_sink_name() -> Result<Option<String>> {
    request_default_name(Request::DefaultSinkName)
}
