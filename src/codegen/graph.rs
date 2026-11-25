//! Graphs of the declarations of a model.

/// The strongly connected component of each node of a graph, given as the nodes that each node has an edge to, by
/// Tarjan's algorithm. It keeps its own stack, so that a long chain of declarations does not overflow the stack of the
/// thread.
pub(crate) fn components(edges: &[Vec<usize>]) -> Vec<usize> {
    const UNSEEN: usize = usize::MAX;
    let count = edges.len();
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
            if let Some(&next) = edges[node].get(followed) {
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
