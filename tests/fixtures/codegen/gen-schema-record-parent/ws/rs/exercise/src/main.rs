//! Exercise record parent linkage through a generated processor: read
//! committed nested data through the generated typed reader and print
//! each record and declaration in stream order with its node and the
//! node its `parent` names, and each container's scope completion, then
//! check the stream against the documented
//! order: an item follows every item nested in it (post-order), while
//! node indexes number nodes as they open (pre-order). Every name the
//! exercise spells comes from the generated crate alone.

use std::path::PathBuf;

use tree_proc::{CheckScope, DiagnosticRecord, Extensions, Item, TypedItem, load, reader};

/// One record, declaration, or container completion as the stream
/// yielded it: its node as `(source, index)`, its parent's node (a
/// completion carries none), and how it prints.
struct Seen {
    node: (u32, u32),
    parent: Option<(u32, u32)>,
    label: String,
    completion: bool,
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [schema, data] = args.as_slice() else {
        eprintln!("record-parent-exercise: usage: record-parent-exercise <schema> <data>");
        std::process::exit(2);
    };

    let definition = load(
        &[schema.as_path()],
        &Extensions::standard(),
        CheckScope::Complete,
    )
    .unwrap_or_else(|records| reject("schema", &records));
    println!("schema ok digest={}", definition.digest());

    let reading = reader(&definition).unwrap_or_else(|records| reject("reader", &records));
    let mut seen: Vec<Seen> = Vec::new();
    let mut sink = |item: TypedItem<Item>| {
        let (node, parent, label) = match item {
            TypedItem::Record {
                node,
                parent,
                node_name,
                value,
                ..
            }
            | TypedItem::Declaration {
                node,
                parent,
                node_name,
                value,
                ..
            } => (node, parent, format!("{node_name} {}", title(&value))),
            TypedItem::ScopeComplete(completion) => {
                if let Some(node) = completion.scope {
                    seen.push(Seen {
                        node: (node.source.0, node.index),
                        parent: None,
                        label: "container".to_owned(),
                        completion: true,
                    });
                }
                return;
            }
            _ => return,
        };
        seen.push(Seen {
            node: (node.source.0, node.index),
            parent: parent.map(|parent| (parent.source.0, parent.index)),
            label,
            completion: false,
        });
    };
    match reading.read(&[data.as_path()], &mut sink) {
        Ok(failed) => println!("read failed={failed}"),
        Err(error) => {
            eprintln!("record-parent-exercise: data read failed: {error}");
            std::process::exit(1);
        }
    }

    let label_of = |node: (u32, u32)| {
        seen.iter()
            .find(|other| other.node == node)
            .map_or("missing", |other| other.label.as_str())
    };
    for item in &seen {
        if item.completion {
            println!(
                "{} at={}:{} completes",
                item.label, item.node.0, item.node.1
            );
            continue;
        }
        let parent = match item.parent {
            Some(parent) => format!("{} at={}:{}", label_of(parent), parent.0, parent.1),
            None => "root".to_owned(),
        };
        println!(
            "{} at={}:{} parent={parent}",
            item.label, item.node.0, item.node.1
        );
    }

    // Every parent is an item of the same source, yielded after its
    // child and opened before it.
    let ordered = seen.iter().enumerate().all(|(at, item)| match item.parent {
        None => true,
        Some(parent) => {
            parent.0 == item.node.0
                && parent.1 < item.node.1
                && seen[at + 1..].iter().any(|later| later.node == parent)
        }
    });
    println!("post-order parents-after-children={ordered}");
}

/// The title a section or card holds.
fn title(value: &Item) -> &str {
    match value {
        Item::Section(section) => section.title.as_str(),
        Item::Card(card) => card.title.as_str(),
    }
}

/// Report a rejected stage and fail; the pinned run never reaches here.
fn reject(stage: &str, records: &[DiagnosticRecord]) -> ! {
    let codes = records
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("record-parent-exercise: {stage} rejected the input ({codes})");
    std::process::exit(1);
}
