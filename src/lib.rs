//! `json_env_logger` is an extension of [`env_logger`](https://crates.io/crates/env_logger) crate providing JSON formatted logs.
//!
//! The [`env_logger`](https://crates.io/crates/env_logger) is a crate that provides a way to declare what log levels are enabled for which modules \via a `RUST_LOG` env variable. See its documentation for
//! syntax of declaring crate and module filtering options.
//!
//! ## features
//!
//! * `iso-timestamps`
//!
//! By default, a timestamp field called `ts` is emitted with the current unix epic timestamp in seconds
//! You can replace this with IOS-8601 timestamps by enabling the `iso-timestamps` feature. Note, this will add `chrono` crate
//! to your dependency tree.
//!
//! ```toml
//! [dependencies]
//! json_env_logger = { version = "0.1", features = ["iso-timestamps"] }
//! ```
//! * `backtrace`
//!
//! When registering a panic hook with `panic_hook` by default backtraces are omitted. You can
//! annotate your error with then by enabling the `backtrace` feature.
//!
//! ```toml
//! [dependencies]
//! json_env_logger = { version = "0.1", features = ["backtrace"] }
//! ```

// export to make types accessible without
// requiring adding another Cargo.toml dependency
#[doc(hidden)]
pub extern crate env_logger;

use env_logger::Builder;
use log::kv;
use std::{
    io::{self, Write},
    panic, thread,
};

/// Register configured json env logger implementation with `log` crate.
///
/// Applications should ensure this fn gets called once and only once per application
/// lifetime
///
/// # panics
///
/// Panics if logger has already been configured
pub fn init() {
    try_init().unwrap()
}

/// Register configured json env logger with `log` crate
///
/// Will yield an `log::SetLoggerError` when a logger has already
/// been configured
pub fn try_init() -> Result<(), log::SetLoggerError> {
    builder().try_init()
}

/// Register a panic hook that serializes panic information as json
/// and logs via `log::error`
pub fn panic_hook() {
    panic::set_hook(Box::new(|info| {
        let thread = thread::current();
        let thread = thread.name().unwrap_or("unnamed");

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &**s,
                None => "Box<Any>",
            },
        };

        match info.location() {
            Some(location) => {
                #[cfg(not(feature = "backtrace"))]
                {
                    kv_log_macro::error!(
                        "panicked at '{}'", msg,
                        {
                            thread: thread,
                            location: format!("{}:{}", location.file(), location.line())
                        }
                    );
                }

                #[cfg(feature = "backtrace")]
                {
                    kv_log_macro::error!(
                        "panicked at '{}'", msg,
                        {
                            thread: thread,
                            location: format!("{}:{}", location.file(), location.line()),
                            backtrace: format!("{:?}", backtrace::Backtrace::new())
                        }
                    );
                }
            }
            None => {
                #[cfg(not(feature = "backtrace"))]
                {
                    kv_log_macro::error!("panicked at '{}'", msg, { thread: thread });
                }
                #[cfg(feature = "backtrace")]
                {
                    kv_log_macro::error!(
                        "panicked at '{}'", msg,
                        {
                            thread: thread,
                            backtrace: format!("{:?}", backtrace::Backtrace::new())
                        }
                    );
                }
            }
        }
    }));
}

/// Yields the standard `env_logger::Builder` configured to log in JSON format
pub fn builder() -> Builder {
    let mut builder = Builder::from_default_env();
    builder.format(write);
    builder
}

/// Use a custom environment variable instead of RUST_LOG
pub fn builder_from_env<'a, E>(env_var_name: E) -> Builder
where
    E: Into<env_logger::Env<'a>>,
{
    let mut builder = Builder::from_env(env_var_name);
    builder.format(write);
    builder
}

fn write<F>(
    f: &mut F,
    record: &log::Record,
) -> io::Result<()>
where
    F: Write,
{
    write!(f, "{{")?;
    write!(f, "\"level\":\"{}\",", record.level())?;

    #[cfg(feature = "iso-timestamps")]
    {
        write!(
            f,
            "\"ts\":\"{}\"",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        )?;
    }
    #[cfg(not(feature = "iso-timestamps"))]
    {
        write!(
            f,
            "\"ts\":{}",
            std::time::UNIX_EPOCH.elapsed().unwrap().as_millis()
        )?;
    }
    write!(f, ",\"msg\":")?;
    write_json_str(f, &record.args().to_string())?;

    struct Visitor<'a, W: Write> {
        writer: &'a mut W,
    }

    impl<'kvs, 'a, W: Write> kv::Visitor<'kvs> for Visitor<'a, W> {
        fn visit_pair(
            &mut self,
            key: kv::Key<'kvs>,
            val: kv::Value<'kvs>,
        ) -> Result<(), kv::Error> {
            write!(self.writer, ",\"{}\":{}", key, val).unwrap();
            Ok(())
        }
    }

    let mut visitor = Visitor { writer: f };
    record.key_values().visit(&mut visitor).unwrap();
    writeln!(f, "}}")
}

// until log kv Value impl serde::Serialize
fn write_json_str<W: io::Write>(
    writer: &mut W,
    raw: &str,
) -> std::io::Result<()> {
    serde_json::to_writer(writer, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use serial_test::serial;
    use std::{
        error::Error,
        sync::{Arc, RwLock},
    };

    // enables swapping loggers after it has been initialized
    lazy_static! {
        static ref LOGGER: Arc<RwLock<env_logger::Logger>> =
            Arc::new(RwLock::new(env_logger::Logger::from_default_env()));
    }
    struct LoggerShim {}
    impl log::Log for LoggerShim {
        fn enabled(
            &self,
            metadata: &log::Metadata,
        ) -> bool {
            LOGGER.read().unwrap().enabled(metadata)
        }

        fn log(
            &self,
            record: &log::Record,
        ) {
            LOGGER.read().unwrap().log(record);
        }

        fn flush(&self) {}
    }

    fn replace_logger(logger: env_logger::Logger) {
        log::set_max_level(logger.filter());
        *LOGGER.write().unwrap() = logger;
        let _ = log::set_boxed_logger(Box::new(LoggerShim {}));
    }

    // Adapter for testing output from logger
    struct WriteAdapter {
        sender: std::sync::mpsc::Sender<u8>,
    }

    impl io::Write for WriteAdapter {
        // On write we forward each u8 of the buffer to the sender and return the length of the buffer
        fn write(
            &mut self,
            buf: &[u8],
        ) -> io::Result<usize> {
            for chr in buf {
                self.sender.send(*chr).unwrap();
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    #[serial]
    fn writes_records_as_json() -> Result<(), Box<dyn Error>> {
        let (rx, tx) = std::sync::mpsc::channel();
        let json_logger = builder()
            .filter_level(log::LevelFilter::Info)
            .target(env_logger::Target::Pipe(Box::new(WriteAdapter {
                sender: rx,
            })))
            .build();
        replace_logger(json_logger);
        log::info!("hello");
        let hello_info_log = String::from_utf8(tx.try_iter().collect::<Vec<u8>>()).unwrap();
        let hello_log_parsed: serde_json::Value = serde_json::from_str(hello_info_log.as_str())?;
        println!("Full json log: {}", hello_info_log);
        assert!(hello_log_parsed["msg"] == "hello");
        Ok(())
    }

    #[test]
    fn escapes_json_strings() -> Result<(), Box<dyn Error>> {
        let mut buf = Vec::new();
        write_json_str(
            &mut buf, r#""
	"#,
        )?;
        println!("{}", std::str::from_utf8(&buf)?);
        assert_eq!("\"\\\"\\n\\t\"", std::str::from_utf8(&buf)?);
        Ok(())
    }

    #[test]
    #[serial]
    fn use_custom_env_var() -> Result<(), Box<dyn Error>> {
        // USE FOO_LOG instead of RUST_LOG for env var. Sets level to info
        std::env::set_var("FOO_LOG", "info");
        // create rx/tx channels to captue log output
        let (rx, tx) = std::sync::mpsc::channel();
        let custom_env_logger = builder_from_env("FOO_LOG")
            .target(env_logger::Target::Pipe(Box::new(WriteAdapter {
                sender: rx,
            })))
            .build();

        replace_logger(custom_env_logger);
        // log level is info. should be parseable json
        log::info!("Hello");
        let hello_info_log = String::from_utf8(tx.try_iter().collect::<Vec<u8>>()).unwrap();
        let hello_log_parsed: serde_json::Value = serde_json::from_str(hello_info_log.as_str())?;
        println!("Parsed json message: {}", hello_log_parsed);
        assert!(hello_log_parsed["msg"] == "Hello");
        // should not print debug level logs due to FOO_LOG value
        log::debug!("Hidden");
        let hidden_debug_log = String::from_utf8(tx.try_iter().collect::<Vec<u8>>()).unwrap();
        println!("Hidden Log: {}", hidden_debug_log);
        assert!(hidden_debug_log.is_empty());
        Ok(())
    }
}
