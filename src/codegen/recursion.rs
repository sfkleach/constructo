//! What the model works out about recursion, so that no target does it again: which fields hold their type by value on
//! a cycle, and so must be boxed in a target such as Rust, and which enums have no default value at all.
//!
//! Both are questions about a graph of the declarations, with a node for each enum and each record type without a
//! name, and an edge from A to B where A holds a B. A type holds another directly, in an enum such as `Option`, in a
//! field, or through a type argument where the declaration holds that parameter. It holds nothing in an array.
//!
//! A target can be told that some types are opaque, as an adopted type is: the generator cannot see into it, and its
//! author boxes what it needs. So nothing is held through a use of an opaque enum, or by an opaque constructor.

use super::model::{Body, Enum, FullName, Model, TypeRef};
use std::collections::{HashMap, HashSet};

/// The types that a target cannot see into.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Opaque {
    /// The enums whose every use is of a type that the target cannot see into.
    pub enums: HashSet<FullName>,
    /// The constructors whose bodies the target cannot see into.
    pub constructors: HashSet<FullName>,
}

/// Works out `boxed` for every field and parameter, and `has_default` for every enum.
pub(crate) fn work_out(model: &mut Model) {
    rebox(model, &Opaque::default());
    let index = index(model);
    let defaults = Graph::new(model, &index, true, &Opaque::default());
    let default_component = super::graph::components(&defaults.edges);
    let has_default: Vec<bool> = model
        .enums
        .iter()
        .enumerate()
        .map(|(node, decl)| {
            // An enum with no constructor has no value at all, and so no default.
            !decl.constructors.is_empty()
                && !defaults.edges[node]
                    .iter()
                    .any(|&next| default_component[next] == default_component[node])
        })
        .collect();
    for (decl, has_default) in model.enums.iter_mut().zip(has_default) {
        decl.has_default = has_default;
    }
}

/// The index of each enum by its full name.
fn index(model: &Model) -> HashMap<FullName, usize> {
    model
        .enums
        .iter()
        .enumerate()
        .map(|(at, decl)| (decl.name.clone(), at))
        .collect()
}

/// Works out `boxed` for every field and parameter again, with the types that are opaque, so that a field is boxed only
/// where it is on a cycle that the target's types can see. `has_default` does not change, since the default of an
/// opaque type is still built from its arguments.
pub(crate) fn rebox(model: &mut Model, opaque: &Opaque) {
    let index = index(model);
    // A field of a node holds its targets, so a target is on a cycle with the node exactly when the two are in one
    // strongly connected component.
    let graph = Graph::new(model, &index, false, opaque);
    let component = super::graph::components(&graph.edges);
    let boxed = |ty: &TypeRef, node: usize| {
        let mut targets = Vec::new();
        graph.held.targets(ty, &index, model.enums.len(), opaque, &mut targets);
        targets.iter().any(|&target| component[target] == component[node])
    };
    // The answers are worked out before any is written, since the graph borrows the model.
    let enum_boxes: Vec<Vec<Vec<bool>>> = model
        .enums
        .iter()
        .enumerate()
        .map(|(node, decl)| {
            decl.constructors
                .iter()
                .map(|constructor| match &constructor.body {
                    Body::Tuple(parameters) => parameters.iter().map(|p| boxed(&p.ty, node)).collect(),
                    Body::Record(fields) => fields.iter().map(|f| boxed(&f.ty, node)).collect(),
                    Body::None | Body::Array(_) => Vec::new(),
                })
                .collect()
        })
        .collect();
    let record_boxes: Vec<Vec<bool>> = model
        .records
        .iter()
        .enumerate()
        .map(|(at, record)| {
            let node = model.enums.len() + at;
            record.fields.iter().map(|f| boxed(&f.ty, node)).collect()
        })
        .collect();
    for (decl, boxes) in model.enums.iter_mut().zip(enum_boxes) {
        for (constructor, boxes) in decl.constructors.iter_mut().zip(boxes) {
            match &mut constructor.body {
                Body::Tuple(parameters) => parameters.iter_mut().zip(boxes).for_each(|(p, b)| p.boxed = b),
                Body::Record(fields) => fields.iter_mut().zip(boxes).for_each(|(f, b)| f.boxed = b),
                Body::None | Body::Array(_) => {}
            }
        }
    }
    for (record, boxes) in model.records.iter_mut().zip(record_boxes) {
        record.fields.iter_mut().zip(boxes).for_each(|(f, b)| f.boxed = b);
    }
}

/// The types that the constructor holds, which are none for an array: an array holds nothing by value, and its default
/// is empty.
fn held_types(body: &Body) -> Vec<&TypeRef> {
    match body {
        Body::None | Body::Array(_) => Vec::new(),
        Body::Tuple(parameters) => parameters.iter().map(|p| &p.ty).collect(),
        Body::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
    }
}

/// The constructors of an enum that the graph looks at: every one, or for defaults only the first, since the default of
/// an enum is that of its first constructor. An opaque constructor holds nothing that the graph can see.
fn constructors_of<'m>(
    decl: &'m Enum,
    first_only: bool,
    opaque: &'m Opaque,
) -> impl Iterator<Item = &'m super::model::Constructor> {
    let constructors = if first_only {
        &decl.constructors[..decl.constructors.len().min(1)]
    } else {
        &decl.constructors[..]
    };
    constructors.iter().filter(|c| !opaque.constructors.contains(&c.name))
}

/// Which parameters each declaration holds, by its index: the enums, and then the records.
struct Held {
    enums: Vec<Vec<bool>>,
    records: Vec<Vec<bool>>,
}

impl Held {
    /// Works out which parameters each declaration holds, as a fixed point: a declaration holds a parameter if a type
    /// that it holds is the parameter, or holds it through a type argument where that declaration holds its parameter.
    fn new(model: &Model, index: &HashMap<FullName, usize>, first_only: bool, opaque: &Opaque) -> Held {
        let mut held = Held {
            enums: model.enums.iter().map(|decl| vec![false; decl.params.len()]).collect(),
            records: model
                .records
                .iter()
                .map(|record| vec![false; record.params.len()])
                .collect(),
        };
        // Each round sets at least one more, or ends, so there are at most as many rounds as parameters.
        loop {
            let mut changed = false;
            for (at, decl) in model.enums.iter().enumerate() {
                for (p, param) in decl.params.iter().enumerate() {
                    if held.enums[at][p] {
                        continue;
                    }
                    let holds = constructors_of(decl, first_only, opaque)
                        .flat_map(|constructor| held_types(&constructor.body))
                        .any(|ty| held.holds_param(ty, &param.name, index, opaque));
                    if holds {
                        held.enums[at][p] = true;
                        changed = true;
                    }
                }
            }
            for (at, record) in model.records.iter().enumerate() {
                for (p, param) in record.params.iter().enumerate() {
                    let holds = record
                        .fields
                        .iter()
                        .any(|f| held.holds_param(&f.ty, param, index, opaque));
                    if !held.records[at][p] && holds {
                        held.records[at][p] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                return held;
            }
        }
    }

    /// Whether the type holds the parameter with the name. An opaque enum holds nothing that can be seen.
    fn holds_param(&self, ty: &TypeRef, name: &str, index: &HashMap<FullName, usize>, opaque: &Opaque) -> bool {
        let through = |args: &[TypeRef], holds: Option<&Vec<bool>>| {
            args.iter().enumerate().any(|(j, arg)| {
                holds.and_then(|holds| holds.get(j)).copied().unwrap_or(false)
                    && self.holds_param(arg, name, index, opaque)
            })
        };
        match ty {
            TypeRef::Param(param) => param == name,
            TypeRef::Enum(decl, _) if opaque.enums.contains(decl) => false,
            TypeRef::Enum(decl, args) => through(args, index.get(decl).map(|&at| &self.enums[at])),
            TypeRef::Record(at, args) => through(args, self.records.get(*at)),
            TypeRef::Array(_) | TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => false,
        }
    }

    /// Adds the nodes that the type holds to `out`: an enum is node `index`, and a record is `enums` after its index.
    /// An opaque enum is not a node of the graph, and nothing is held through its arguments.
    fn targets(
        &self,
        ty: &TypeRef,
        index: &HashMap<FullName, usize>,
        enums: usize,
        opaque: &Opaque,
        out: &mut Vec<usize>,
    ) {
        let (node, args, holds) = match ty {
            TypeRef::Enum(decl, _) if opaque.enums.contains(decl) => return,
            // Every enum that a type names is in the model, since the run had no errors, so one that is not found is
            // only passed over.
            TypeRef::Enum(decl, args) => match index.get(decl) {
                Some(&at) => (at, args, &self.enums[at]),
                None => return,
            },
            TypeRef::Record(at, args) => (enums + at, args, &self.records[*at]),
            TypeRef::Array(_) | TypeRef::Param(_) | TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => {
                return
            }
        };
        if !out.contains(&node) {
            out.push(node);
        }
        for (arg, &holds) in args.iter().zip(holds) {
            if holds {
                self.targets(arg, index, enums, opaque, out);
            }
        }
    }
}

/// The graph of what each declaration holds.
struct Graph {
    held: Held,
    /// The nodes that each node holds.
    edges: Vec<Vec<usize>>,
}

impl Graph {
    /// The graph of every constructor, or with `first_only` of the first constructor of each enum, which is the graph
    /// that the default of each enum is worked out over.
    fn new(model: &Model, index: &HashMap<FullName, usize>, first_only: bool, opaque: &Opaque) -> Graph {
        let held = Held::new(model, index, first_only, opaque);
        let enums = model.enums.len();
        let mut edges = Vec::new();
        for decl in &model.enums {
            let mut out = Vec::new();
            for constructor in constructors_of(decl, first_only, opaque) {
                for ty in held_types(&constructor.body) {
                    held.targets(ty, index, enums, opaque, &mut out);
                }
            }
            edges.push(out);
        }
        for record in &model.records {
            let mut out = Vec::new();
            for field in &record.fields {
                held.targets(&field.ty, index, enums, opaque, &mut out);
            }
            edges.push(out);
        }
        Graph { held, edges }
    }
}
