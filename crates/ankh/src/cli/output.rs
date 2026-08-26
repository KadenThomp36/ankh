use ankh_core::{Error, SCHEMA_VERSION};
use comfy_table::{presets::NOTHING, Cell, CellAlignment, Table};
use serde_json::{json, Value};

use crate::Format;

pub struct Out {
    format: Format,
}

impl Out {
    pub fn new(format: Format) -> Self {
        Self { format }
    }

    pub fn format(&self) -> Format {
        self.format
    }

    fn envelope(&self, data: Value) -> Value {
        match data {
            Value::Object(mut m) => {
                m.insert("schema_version".into(), json!(SCHEMA_VERSION));
                Value::Object(m)
            }
            other => json!({ "schema_version": SCHEMA_VERSION, "data": other }),
        }
    }

    pub fn ok(&self, text: &str, data: Value) {
        match self.format {
            Format::Table => println!("{text}"),
            Format::Json | Format::Jsonl => println!("{}", self.envelope(data)),
        }
    }

    pub fn error(&self, e: &Error) {
        match self.format {
            Format::Table => eprintln!("error: {e}"),
            Format::Json | Format::Jsonl => eprintln!(
                "{}",
                json!({ "schema_version": SCHEMA_VERSION, "error": e.to_string(), "code": e.exit_code() })
            ),
        }
    }

    /// Tabular data: pretty for humans, a JSON tree for `--format json`, one
    /// object per line for `--format jsonl`.
    pub fn table(
        &self,
        headers: &[&str],
        rows: Vec<Vec<String>>,
        json: impl FnOnce() -> Value,
        jsonl: impl FnOnce() -> Vec<Value>,
    ) {
        match self.format {
            Format::Table => {
                let mut t = Table::new();
                t.load_preset(NOTHING);
                t.set_header(headers.iter().map(|h| Cell::new(h.to_uppercase())));
                for r in rows {
                    let mut cells: Vec<Cell> = r.into_iter().map(Cell::new).collect();
                    for c in cells.iter_mut().skip(1) {
                        *c = std::mem::replace(c, Cell::new("")).set_alignment(CellAlignment::Right);
                    }
                    t.add_row(cells);
                }
                println!("{t}");
            }
            Format::Json => println!("{}", self.envelope(json())),
            Format::Jsonl => {
                for v in jsonl() {
                    println!("{}", self.envelope(v));
                }
            }
        }
    }
}
