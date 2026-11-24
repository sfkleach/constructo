//! What the model works out about recursion, so that no target does it again: which fields hold their type by value on
//! a cycle, and so must be boxed in a target such as Rust, and which enums have no default value at all.
//!
//! Both are questions about a graph of the declarations, with a node for each enum and each record type without a
//! name, and an edge from A to B where A holds a B. A type holds another directly, in an enum such as `Option`, in a
//! field, or through a type argument where the declaration holds that parameter. It holds nothing in an array.

use super::model::{Body, Enum, FullName, Model, TypeRef};
use std::collections::HashMap;

/// Works out `boxed` for every field and parameter, and `has_default` for every enum.
pub(crate) fn work_out(model: &mut Model) {
    let index: HashMap<FullName, usize> = model
        .enums
        .iter()
        .enumerate()
        .map(|(at, decl)| (decl.name.clone(), at))
        .collect();
    // A field of a node holds its targets, so a target is on a cycle with the node exactly when the two are in one
    // strongly connected component.
    let graph = Graph::new(model, &index, false);
    let component = graph.components();
    let boxed = |ty: &TypeRef, node: usize| {
        let mut targets = Vec::new();
        graph.held.targets(ty, &index, model.enums.len(), &mut targets);
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
    let defaults = Graph::new(model, &index, true);
    let default_component = defaults.components();
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

    for ((decl, boxes), has_default) in model.enums.iter_mut().zip(enum_boxes).zip(has_default) {
        decl.has_default = has_default;
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
/// an enum is that of its first constructor.
fn constructors_of(decl: &Enum, first_only: bool) -> &[super::model::Constructor] {
    if first_only {
        &decl.constructors[..decl.constructors.len().min(1)]
    } else {
        &decl.constructors
    }
}

/// Which parameters each declaration holds, by its index: the enums, and then the records.
struct Held {
    enums: Vec<Vec<bool>>,
    records: Vec<Vec<bool>>,
}

impl Held {
    /// Works out which parameters each declaration holds, as a fixed point: a declaration holds a parameter if a type
    /// that it holds is the parameter, or holds it through a type argument where that declaration holds its parameter.
    fn new(model: &Model, index: &HashMap<FullName, usize>, first_only: bool) -> Held {
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
                    let holds = constructors_of(decl, first_only)
                        .iter()
                        .flat_map(|constructor| held_types(&constructor.body))
                        .any(|ty| held.holds_param(ty, &param.name, index));
                    if holds {
                        held.enums[at][p] = true;
                        changed = true;
                    }
                }
            }
            for (at, record) in model.records.iter().enumerate() {
                for (p, param) in record.params.iter().enumerate() {
                    if !held.records[at][p] && record.fields.iter().any(|f| held.holds_param(&f.ty, param, index)) {
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

    /// Whether the type holds the parameter with the name.
    fn holds_param(&self, ty: &TypeRef, name: &str, index: &HashMap<FullName, usize>) -> bool {
        let through = |args: &[TypeRef], holds: Option<&Vec<bool>>| {
            args.iter().enumerate().any(|(j, arg)| {
                holds.and_then(|holds| holds.get(j)).copied().unwrap_or(false) && self.holds_param(arg, name, index)
            })
        };
        match ty {
            TypeRef::Param(param) => param == name,
            TypeRef::Enum(decl, args) => through(args, index.get(decl).map(|&at| &self.enums[at])),
            TypeRef::Record(at, args) => through(args, self.records.get(*at)),
            TypeRef::Array(_) | TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => false,
        }
    }

    /// Adds the nodes that the type holds to `out`: an enum is node `index`, and a record is `enums` after its index.
    fn targets(&self, ty: &TypeRef, index: &HashMap<FullName, usize>, enums: usize, out: &mut Vec<usize>) {
        let (node, args, holds) = match ty {
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
                self.targets(arg, index, enums, out);
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
    fn new(model: &Model, index: &HashMap<FullName, usize>, first_only: bool) -> Graph {
        let held = Held::new(model, index, first_only);
        let enums = model.enums.len();
        let mut edges = Vec::new();
        for decl in &model.enums {
            let mut out = Vec::new();
            for constructor in constructors_of(decl, first_only) {
                for ty in held_types(&constructor.body) {
                    held.targets(ty, index, enums, &mut out);
                }
            }
            edges.push(out);
        }
        for record in &model.records {
            let mut out = Vec::new();
            for field in &record.fields {
                held.targets(&field.ty, index, enums, &mut out);
            }
            edges.push(out);
        }
        Graph { held, edges }
    }

    /// The strongly connected component of each node, by Tarjan's algorithm. It keeps its own stack, so that a long
    /// chain of declarations does not overflow the stack of the thread.
    fn components(&self) -> Vec<usize> {
        const UNSEEN: usize = usize::MAX;
        let count = self.edges.len();
        let mut order = vec![UNSEEN; count];
        let mut low = vec![0; count];
        let mut on_stack = vec![false; count];
        let mut component = vec![UNSEEN; count];
        let mut stack = Vec::new();
        let mut next_order = 0;
        let mut next_component = 0;
        for start in 0..count {
            if order[start] != UNSEEN {
                continue;
            }
            // Each entry is a node and how many of its edges have been followed.
            let mut work = vec![(start, 0)];
            order[start] = next_order;
            low[start] = next_order;
            next_order += 1;
            stack.push(start);
            on_stack[start] = true;
            while let Some(&(node, followed)) = work.last() {
                if let Some(&next) = self.edges[node].get(followed) {
                    work.last_mut().expect("the work is not empty").1 += 1;
                    if order[next] == UNSEEN {
                        order[next] = next_order;
                        low[next] = next_order;
                        next_order += 1;
                        stack.push(next);
                        on_stack[next] = true;
                        work.push((next, 0));
                    } else if on_stack[next] {
                        low[node] = low[node].min(order[next]);
                    }
                    continue;
                }
                work.pop();
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[node]);
                }
                if low[node] == order[node] {
                    loop {
                        let member = stack
                            .pop()
                            .expect("a node is on the stack until its component is found");
                        on_stack[member] = false;
                        component[member] = next_component;
                        if member == node {
                            break;
                        }
                    }
                    next_component += 1;
                }
            }
        }
        component
    }
}
