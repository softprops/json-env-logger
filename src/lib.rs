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

/// Yields the standard `env_logger::Builder` configured to log in JSON format
pub fn builder() -> Builder {
    let mut builder = Builder::from_default_env();
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
            write!(self.writer, ",")?;
            write_json_str(&mut self.writer, key.as_str())?;
            write!(self.writer, ":")?;
            write_json_str(&mut self.writer, &format!("{}", val))?;
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
    use std::error::Error;

    #[test]
    fn writes_records_as_json() -> Result<(), Box<dyn Error>> {
        let record = log::Record::builder()
            .args(format_args!("hello"))
            .level(log::Level::Info)
            .build();
        let mut buf = Vec::new();
        write(&mut buf, &record)?;
        let output = std::str::from_utf8(&buf)?;
        assert!(serde_json::from_str::<serde_json::Value>(&output).is_ok());
        Ok(())
    }

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

    #[test]
    fn escapes_json_strings_within_kv() -> Result<(), Box<dyn Error>> {
        use serde_json::Value;

        let record = log::Record::builder()
            .args(format_args!("msg"))
            .key_values(&(
                "challenge \"key\"",
                r#""challenge":"key",{"nested":not really json}"#,
            ))
            .level(log::Level::Info)
            .build();

        // Output the record and then deserialize to make sure it works and we
        // can locate the challenge string.
        let mut buf = Vec::new();
        write(&mut buf, &record)?;
        let value = serde_json::from_str::<Value>(&std::str::from_utf8(&buf)?)?;

        // Should be an object with a challenge key and value.
        match value.get("challenge \"key\"") {
            Some(Value::String(string)) => {
                assert_eq!(string, r#""challenge":"key",{"nested":not really json}"#);
            }
            _ => panic!("Object with challenge key expected"),
        };
        Ok(())
    }
}
