//! an example demonstrating some common features for json_env_logger
//! To enable low level logging levels set an env variable RUST_LOG. i.e. RUST_LOG=TRACE

// note the use of kv_log_macro. structured fields are not quite
// backed in the log crate yet. until then kv_log_macro exposes them
// in log-compatible macros
use kv_log_macro::{debug, error, info, trace, warn};

fn main() {
    json_env_logger::init();
    trace!("I am a trace", {task_id: 567, thread_id: "12"});
    debug!("I am a debug", { foo: 1 });
    info!("I am an info");
    warn!("I am a warning");
    error!("I am an error");
}
