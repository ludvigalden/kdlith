//! Exercise the generated schema processor's field spellings: load the
//! committed schema through the generated loader, read a committed data
//! document through the generated typed reader, and print every record
//! field through its snake_case identifier — camelCase, digit-boundary,
//! acronym, kebab, already-snake, and keyword-prefixed spellings alike.

use std::path::PathBuf;

use kdlith_engine::{CheckScope, Extensions, TypedItem};
use snakefields_proc::types::Job;
use snakefields_proc::{Item, load, reader};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("schema-exercise: usage: schema-exercise <schema> <data>");
        std::process::exit(2);
    };
    let extensions = Extensions::standard();

    let definition = load(&[schema], &extensions, CheckScope::Complete)
        .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));

    let mut jobs: Vec<Job> = Vec::new();
    let mut data_sink = |item: TypedItem<Item>| {
        if let TypedItem::Record {
            value: Item::Job(job),
            ..
        } = item
        {
            jobs.push(job);
        }
    };
    match reading.read(&[data], &mut data_sink) {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("schema-exercise: the data reading reported error diagnostics");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("schema-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }
    println!("read ok jobs={}", jobs.len());
    for job in &jobs {
        println!(
            "job q_start={} q2_start={} player1_score={}",
            job.q_start, job.q2_start, job.player1_score
        );
        println!(
            "job addr2_line={} parse_json_data={} http_url={}",
            job.addr2_line, job.parse_json_data, job.http_url
        );
        println!(
            "job primary_label={} max_depth={} fn_x={}",
            job.primary_label, job.max_depth, job.fn_x
        );
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[kdlith_diagnostic::DiagnosticRecord]) -> ! {
    eprintln!(
        "schema-exercise: {stage} rejected the input ({} diagnostics)",
        records.len()
    );
    std::process::exit(1);
}
