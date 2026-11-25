//! Rust cannot hold a type that holds itself with other arguments, as `enum Nested<T> = Flat(T), Deep(Nested<[T]>)`
//! holds a `Nested<[T]>`: the type can be declared, but its drop-check overflows as soon as a value of it is made,
//! through a `Box` or a `Vec` as much as by value. `check` accepts such a type, and the Rust target refuses it.
//!
//! A recursive type is regular when every type of its cycle is reached with one set of arguments, in terms of the
//! parameters of the first. Each strongly connected component of the graph of the types that a declaration names,
//! anywhere in its constructors, is followed from its first member with its own parameters, and a member that is
//! reached with two sets of arguments is the error.

use super::super::graph::components;
use super::super::model::{At, Body, FullName, Model, TypeRef};
use crate::diagnostics::Diagnostic;
use std::collections::{HashMap, VecDeque};

/// A node of the graph: an enum, or a record type without a name, by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Enum(usize),
    Record(usize),
}

/// The errors for the types that Rust cannot hold, one for each cycle, at the type that is reached with other
/// arguments.
pub(super) fn irregular(model: &Model, index: &HashMap<FullName, usize>) -> Vec<Diagnostic> {
    let enums = model.enums.len();
    let number = |node: Node| match node {
        Node::Enum(at) => at,
        Node::Record(at) => enums + at,
    };
    let node_of = |n: usize| {
        if n < enums {
            Node::Enum(n)
        } else {
            Node::Record(n - enums)
        }
    };
    let held = |node: Node| -> Vec<&TypeRef> {
        match node {
            Node::Enum(at) => model.enums[at]
                .constructors
                .iter()
                .flat_map(|c| match &c.body {
                    Body::None => Vec::new(),
                    Body::Tuple(parameters) => parameters.iter().map(|p| &p.ty).collect(),
                    Body::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
                    Body::Array(element) => vec![element],
                })
                .collect(),
            Node::Record(at) => model.records[at].fields.iter().map(|f| &f.ty).collect(),
        }
    };
    let count = enums + model.records.len();
    let edges: Vec<Vec<usize>> = (0..count)
        .map(|n| {
            let mut out = Vec::new();
            for ty in held(node_of(n)) {
                named(ty, index, &mut |node, _| out.push(number(node)));
            }
            out
        })
        .collect();
    let component = components(&edges);
    let params = |node: Node| -> Vec<String> {
        match node {
            Node::Enum(at) => model.enums[at].params.iter().map(|p| p.name.clone()).collect(),
            Node::Record(at) => model.records[at].params.clone(),
        }
    };
    let mut errors = Vec::new();
    let mut done = vec![false; count];
    for start in 0..count {
        // Only a component with a cycle can hold a type again, and each is followed once, from its first member.
        let cyclic = edges[start].iter().any(|&next| component[next] == component[start]);
        if done[start] || !cyclic {
            continue;
        }
        for (n, flag) in done.iter_mut().enumerate() {
            if component[n] == component[start] {
                *flag = true;
            }
        }
        let first = node_of(start);
        let mut reached: HashMap<usize, Vec<TypeRef>> = HashMap::new();
        reached.insert(start, params(first).into_iter().map(TypeRef::Param).collect());
        let mut queue = VecDeque::from([start]);
        'follow: while let Some(n) = queue.pop_front() {
            let node = node_of(n);
            let names = params(node);
            let args = reached[&n].clone();
            for ty in held(node) {
                let mut found = Vec::new();
                named(ty, index, &mut |target, list: &[TypeRef]| {
                    found.push((target, list.to_vec()))
                });
                for (target, list) in found {
                    let m = number(target);
                    if component[m] != component[start] {
                        continue;
                    }
                    let list: Vec<TypeRef> = list.iter().map(|arg| substitute(arg, &names, &args)).collect();
                    match reached.get(&m) {
                        None => {
                            reached.insert(m, list);
                            queue.push_back(m);
                        }
                        Some(before) if *before == list => {}
                        Some(_) => {
                            errors.push(error(model, target, node));
                            break 'follow;
                        }
                    }
                }
            }
        }
    }
    errors
}

/// Calls `found` with every enum and record that the type names, with its arguments, outside in.
fn named(ty: &TypeRef, index: &HashMap<FullName, usize>, found: &mut dyn FnMut(Node, &[TypeRef])) {
    match ty {
        TypeRef::Enum(name, args) => {
            if let Some(&at) = index.get(name) {
                found(Node::Enum(at), args);
            }
            args.iter().for_each(|arg| named(arg, index, found));
        }
        TypeRef::Record(at, args) => {
            found(Node::Record(*at), args);
            args.iter().for_each(|arg| named(arg, index, found));
        }
        TypeRef::Array(element) => named(element, index, found),
        TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String | TypeRef::Param(_) => {}
    }
}

/// The type with each of the parameters replaced by its argument.
fn substitute(ty: &TypeRef, params: &[String], args: &[TypeRef]) -> TypeRef {
    match ty {
        TypeRef::Param(name) => match params.iter().position(|p| p == name) {
            Some(at) => args[at].clone(),
            None => ty.clone(),
        },
        TypeRef::Array(element) => TypeRef::Array(Box::new(substitute(element, params, args))),
        TypeRef::Enum(name, list) => {
            TypeRef::Enum(name.clone(), list.iter().map(|t| substitute(t, params, args)).collect())
        }
        TypeRef::Record(at, list) => TypeRef::Record(*at, list.iter().map(|t| substitute(t, params, args)).collect()),
        TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => ty.clone(),
    }
}

/// The error for a type that is reached with other arguments, at the declaration that holds it so.
fn error(model: &Model, target: Node, holder: Node) -> Diagnostic {
    let describe = |node: Node| match node {
        Node::Enum(at) => format!("the enum {}", model.enums[at].name),
        Node::Record(at) => format!("the record type {}", model.records[at].name),
    };
    let at: At = match holder {
        Node::Enum(at) => model.enums[at].at,
        Node::Record(at) => model.records[at].at,
    };
    let message = if target == holder {
        format!(
            "Rust cannot hold {}, since it holds itself with other arguments",
            describe(target)
        )
    } else {
        format!(
            "Rust cannot hold {}, since it holds {} with other arguments than it is held with",
            describe(holder),
            describe(target)
        )
    };
    Diagnostic::new(&model.files[at.file], at.pos, message)
}
