use constructo_runtime::ast::{TaggedBody, Value, ValueKind};
use constructo_runtime::codec::parse;

fn describe(value: &Value, depth: usize) {
    let pad = "  ".repeat(depth);
    match &value.kind {
        ValueKind::Nat(t) | ValueKind::Int(t) | ValueKind::Float(t) => println!("{pad}number {t}"),
        ValueKind::Str(s) => println!("{pad}string {s:?}"),
        ValueKind::Array(items) => {
            println!("{pad}array of {}", items.len());
            items.iter().for_each(|v| describe(v, depth + 1));
        }
        ValueKind::Record(members) => {
            println!("{pad}record");
            for m in members {
                println!("{pad}  {}:", m.name);
                describe(&m.value, depth + 2);
            }
        }
        ValueKind::Tagged { name, body } => {
            println!("{pad}constructor {name} at {}", value.pos);
            match body {
                None => {}
                Some(TaggedBody::Record(members)) => {
                    for m in members {
                        println!("{pad}  {}:", m.name);
                        describe(&m.value, depth + 2);
                    }
                }
                Some(TaggedBody::Tuple(args)) => args.iter().for_each(|a| describe(&a.value, depth + 1)),
                Some(TaggedBody::Array(items)) => items.iter().for_each(|v| describe(v, depth + 1)),
            }
        }
    }
}

fn main() {
    let text = "Circle { radius: 2.5 };\nGroup[Square(3), origin];\n{ name: \"x\", tags: [1, -2] };\n";
    for value in parse(text).expect("a values file") {
        describe(&value, 0);
    }
    if let Err(e) = parse("Circle { radius: 2.5 ") {
        println!("error: {e}");
    }
}
