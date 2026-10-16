//! Exercise the generated schema processor's value-resolution hook from
//! the consuming side: a resolver compiled into this crate reads a
//! committed key-value file, the generated reader is built with it, and
//! the run reports what each call carried, which values reached typed
//! records, and how an unresolvable value fails.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use kdlith_model::Value;
use vault_proc::types::Service;
use vault_proc::{Item, ResolutionCall, Unresolvable, ValueResolver, load, reader_with_resolver};

/// The file a provider command would create if anything executed it.
const EXECUTION_MARKER: &str = "provider-executed";

/// Resolves a computed default by the incoming `stage` slot and the
/// default's single dependency, looked up as `<stage>/<dependency>` in
/// a committed `key=value` file. Every call is logged as it arrived.
struct FixtureFileResolver {
    values: BTreeMap<String, String>,
    calls: Mutex<Vec<String>>,
}

impl FixtureFileResolver {
    fn open(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("malformed line {line:?} in {}", path.display()))?;
            values.insert(key.to_owned(), value.to_owned());
        }
        Ok(Self {
            values,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn log(&self, call: &ResolutionCall<'_>, stage: &str) {
        let providers: Vec<String> = call
            .providers
            .iter()
            .map(|provider| {
                format!(
                    "{} ttl={} argc={} argv={:?}",
                    provider.id,
                    provider.ttl.map_or_else(|| "-".to_owned(), |ttl| ttl.to_string()),
                    provider.command.len(),
                    provider.command
                )
            })
            .collect();
        let start = call.site.span.start;
        let line = format!(
            "call source={} site={}:{} node={} field={} type={} secret={} stage={} dependencies={:?} providers={:?}",
            call.source,
            start.line,
            start.column,
            call.node,
            call.field,
            call.ty,
            call.is_secret(),
            stage,
            call.dependencies,
            providers
        );
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(line);
    }

    fn drain(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl ValueResolver for FixtureFileResolver {
    fn resolve(&self, call: &ResolutionCall<'_>) -> Result<Value, Unresolvable> {
        let stage = match call.context.get("stage") {
            Some(Value::String(stage)) => stage.as_str(),
            _ => "-",
        };
        self.log(call, stage);
        let [dependency] = call.dependencies else {
            return Err(Unresolvable::new("expected exactly one dependency"));
        };
        let key = format!("{stage}/{dependency}");
        match self.values.get(&key) {
            Some(value) => Ok(Value::String(value.clone())),
            None => Err(Unresolvable::new(format!("no fixture value for {key}"))),
        }
    }
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, values, data, invalid] = args.as_slice() else {
        eprintln!("resolver-exercise: usage: resolver-exercise <schema> <values> <data> <invalid-data>");
        std::process::exit(2);
    };
    let resolver = Arc::new(FixtureFileResolver::open(values).unwrap_or_else(|error| {
        eprintln!("resolver-exercise: {error}");
        std::process::exit(1);
    }));
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader_with_resolver(&definition, resolver.clone())
        .unwrap_or_else(|records| reject("reader", &records));

    let mut services: Vec<Service> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| {
        if let TypedItem::Record {
            value: Item::Service(service),
            ..
        } = item
        {
            services.push(service);
        }
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("resolver-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("resolver-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    let calls = resolver.drain();
    println!("read ok services={} calls={}", services.len(), calls.len());
    for call in &calls {
        println!("{call}");
    }
    for service in &services {
        println!(
            "service name={} region={} token={}",
            service.name, service.region, service.token
        );
    }

    let mut codes: Vec<String> = Vec::new();
    let mut messages: Vec<String> = Vec::new();
    let mut rejected_services = 0usize;
    let mut invalid_sink = |item: TypedItem<Item>| match item {
        TypedItem::Diagnostic(record) => {
            codes.push(record.code.clone());
            messages.extend(record.message.clone());
        }
        TypedItem::Record {
            value: Item::Service(_),
            ..
        } => rejected_services += 1,
        _ => {}
    };
    let had_errors = match reading.read(&[invalid], &mut invalid_sink) {
        Ok(had_errors) => had_errors,
        Err(error) => {
            eprintln!("resolver-exercise: invalid read failed: {error}");
            std::process::exit(1);
        }
    };
    for call in resolver.drain() {
        println!("{call}");
    }
    let declined = messages
        .iter()
        .find(|message| message.contains("could not be resolved"));
    match declined {
        Some(message) if had_errors && codes.iter().any(|code| code == "KDLITH-BND009") => {
            println!("rejected KDLITH-BND009 records={rejected_services} message={message}");
        }
        _ => {
            eprintln!(
                "resolver-exercise: the unresolvable reference was not rejected as declared \
                 (had_errors={had_errors}, codes={codes:?}, messages={messages:?})"
            );
            std::process::exit(1);
        }
    }

    if Path::new(EXECUTION_MARKER).exists() {
        eprintln!("resolver-exercise: a provider command was executed");
        std::process::exit(1);
    }
    println!("provider command not executed");
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "resolver-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
