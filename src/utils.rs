use std::any::Any;
use std::fmt;

use serde_json::Value;

pub(crate) fn downcast<T: 'static, S: 'static>(value: S) -> Result<T, S> {
    let mut value = Some(value);
    if let Some(value) = <dyn Any>::downcast_mut::<Option<T>>(&mut value) {
        Ok(value.take().unwrap())
    } else {
        Err(value.unwrap())
    }
}
pub(crate) fn write_string_no_escape(value: &Value, f: &mut fmt::Formatter) -> fmt::Result {
    if f.alternate() {
        write_string_pretty_no_escape(value, f, 0)
    } else {
        write_string_no_escape_oneline(value, f)
    }
}

fn write_string_no_escape_oneline(value: &Value, f: &mut fmt::Formatter) -> fmt::Result {
    match value {
        Value::Null => write!(f, "null"),
        Value::Bool(b) => write!(f, "{b}"),
        Value::Number(n) => write!(f, "{n}"),
        Value::String(s) => write!(f, "{s}"),
        Value::Array(arr) => {
            write!(f, "[")?;
            let mut first = true;
            for item in arr {
                if !first {
                    write!(f, ",")?;
                }
                first = false;
                write_string_no_escape(item, f)?;
            }
            write!(f, "]")
        }
        Value::Object(obj) => {
            write!(f, "{{")?;
            let mut first = true;
            for (key, value) in obj {
                if !first {
                    write!(f, ",")?;
                }
                first = false;
                write!(f, "{key}:")?;
                write_string_no_escape(value, f)?;
            }
            write!(f, "}}")
        }
    }
}

fn write_string_pretty_no_escape(
    value: &Value,
    f: &mut fmt::Formatter,
    indent: usize,
) -> fmt::Result {
    match value {
        Value::Null => write!(f, "null"),
        Value::Bool(b) => write!(f, "{b}"),
        Value::Number(n) => write!(f, "{n}"),
        Value::String(s) => write!(f, "{s}"),
        Value::Array(arr) => {
            if arr.is_empty() {
                return write!(f, "[]");
            }

            writeln!(f, "[")?;
            let next_indent = indent + 2;

            for (i, item) in arr.iter().enumerate() {
                write!(f, "{:indent$}", "", indent = next_indent)?;
                write_string_pretty_no_escape(item, f, next_indent)?;

                if i < arr.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }

            write!(f, "{:indent$}]", "", indent = indent)
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                return write!(f, "{{}}");
            }

            writeln!(f, "{{")?;
            let next_indent = indent + 2;

            let entries: Vec<_> = obj.iter().collect();
            for (i, (key, value)) in entries.iter().enumerate() {
                write!(f, "{:indent$}\"{key}\": ", "", indent = next_indent)?;
                write_string_pretty_no_escape(value, f, next_indent)?;

                if i < entries.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }

            write!(f, "{:indent$}}}", "", indent = indent)
        }
    }
}
