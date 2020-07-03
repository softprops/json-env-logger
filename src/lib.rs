//! `json_env_logger` is an extension of `env_logger` crate providing JSON formatted logs.
//!
//! `env_logger` is a crate provides a way to control active log levels via a `RUST_LOG` env variable
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
use std::{io, panic, thread};

/// Register configured json env logger implementation with `log` crate.
///
/// Applications should ensure this fn gets called once and only once per application
/// lifetime
///
/// # panics
///
/// Panics of logger has already been configured
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

/// Yields the standard env_logger builder configured to log in JSON format
pub fn builder() -> Builder {
    let mut builder = Builder::from_default_env();
    builder.format(|f, record| {
        use io::Write;

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
        //map.serialize_entry("msg", &record.args())?;
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
    });

    builder
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
    use std::error::Error;
    #[test]
    fn escapes_json_strings() -> Result<(), Box<dyn Error>> {
        let mut buf = Vec::new();
        write_json_str(
            &mut buf, r#""
	"#,
        )?;
        assert_eq!("\"\\\"\\n\\t\"", std::str::from_utf8(&buf)?);
        Ok(())
    }
}
