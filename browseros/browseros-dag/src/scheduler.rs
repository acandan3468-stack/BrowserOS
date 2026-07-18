use browseros_types::identifiers::NodeId;

use crate::error::DagError;
use crate::graph::DagGraph;

/// Result of scheduling a DAG for execution.
///
/// Contains the topological layers for parallel execution and the
/// flattened execution order for sequential processing.
#[derive(Debug, Clone)]
pub(crate) struct ScheduleResult {
    /// Topological layers. All nodes in a layer can execute in parallel.
    /// Nodes in layer[i] must complete before any node in layer[i+1] starts.
    pub layers: Vec<Vec<NodeId>>,
    /// Flattened topological order (concatenation of all layers).
    pub execution_order: Vec<NodeId>,
}

impl ScheduleResult {
    /// Returns the number of layers in this schedule.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Returns the total number of scheduled nodes.
    pub fn node_count(&self) -> usize {
        self.execution_order.len()
    }

    /// Returns true if no nodes are scheduled.
    pub fn is_empty(&self) -> bool {
        self.execution_order.is_empty()
    }
}

/// Schedule a DAG for execution starting from the given entry nodes.
///
/// Validates that all entry nodes exist and have in-degree 0 (no incoming
/// dependencies), then computes a topological sort of the entire graph.
///
/// # Errors
/// - `DagError::NodeNotFound` if an entry node is not in the graph
/// - `DagError::InvalidEntry` if an entry node has incoming edges
/// - `DagError::CycleDetected` if the graph contains cycles
pub(crate) fn schedule(
    graph: &DagGraph,
    entry_nodes: &[NodeId],
) -> Result<ScheduleResult, DagError> {
    graph.validate_entry_nodes(entry_nodes)?;
    let layers = graph.topological_sort()?;
    let execution_order: Vec<NodeId> = layers.iter().flat_map(|l| l.iter()).cloned().collect();
    Ok(ScheduleResult {
        layers,
        execution_order,
    })
}

/// Schedule a DAG for execution starting from all root nodes.
///
/// Equivalent to calling `schedule()` with all nodes that have in-degree 0.
pub(crate) fn schedule_all(graph: &DagGraph) -> Result<ScheduleResult, DagError> {
    let roots = graph.find_roots();
    schedule(graph, &roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use browseros_types::identifiers::NodeId;

    use crate::node::DagNode;

    fn n(id: &str) -> NodeId {
        NodeId::from_string(id)
    }

    fn make_node(id: &str) -> DagNode {
        DagNode::command(n(id), id, Box::new(|| -> Result<(), DagError> { Ok(()) }))
    }

    fn make_graph(pairs: &[(&str, &str)]) -> DagGraph {
        let mut g = DagGraph::new();
        let mut ids: HashSet<&str> = HashSet::new();
        for (from, to) in pairs {
            ids.insert(from);
            ids.insert(to);
        }
        for id in &ids {
            let _ = g.register_node(make_node(id));
        }
        for (from, to) in pairs {
            let _ = g.add_edge(&n(from), &n(to));
        }
        g
    }

    fn is_valid_topo(layers: &[Vec<NodeId>], g: &DagGraph) -> bool {
        let pos: std::collections::HashMap<&NodeId, usize> = layers
            .iter()
            .enumerate()
            .flat_map(|(i, layer)| layer.iter().map(move |id| (id, i)))
            .collect();
        let adjacency = g.adjacency();
        for from in adjacency.keys() {
            if let Some(deps) = adjacency.get(from) {
                for to in deps {
                    if let (Some(&fp), Some(&tp)) = (pos.get(from), pos.get(to)) {
                        if fp >= tp {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    // ── schedule() ────────────────────────────────────────────────────

    #[test]
    fn schedule_empty_graph() {
        let g = DagGraph::new();
        let result = schedule(&g, &[]).unwrap();
        assert!(result.is_empty());
        assert_eq!(result.layer_count(), 0);
        assert_eq!(result.node_count(), 0);
    }

    #[test]
    fn schedule_linear_chain() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let result = schedule(&g, &[n("a")]).unwrap();
        assert_eq!(result.layer_count(), 3);
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.layers[0], vec![n("a")]);
        assert_eq!(result.layers[1], vec![n("b")]);
        assert_eq!(result.layers[2], vec![n("c")]);
        assert_eq!(result.execution_order, vec![n("a"), n("b"), n("c")]);
        assert!(is_valid_topo(&result.layers, &g));
    }

    #[test]
    fn schedule_diamond() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let result = schedule(&g, &[n("a")]).unwrap();
        assert_eq!(result.layer_count(), 3);
        assert_eq!(result.node_count(), 4);
        assert_eq!(result.layers[0], vec![n("a")]);
        // Layer 1 contains b and c (parallel)
        assert_eq!(result.layers[1].len(), 2);
        assert!(result.layers[1].contains(&n("b")));
        assert!(result.layers[1].contains(&n("c")));
        assert_eq!(result.layers[2], vec![n("d")]);

        // execution_order: a first, then b/c, then d
        assert_eq!(result.execution_order[0], n("a"));
        assert_eq!(*result.execution_order.last().unwrap(), n("d"));
        assert!(is_valid_topo(&result.layers, &g));
    }

    #[test]
    fn schedule_parallel_nodes() {
        let g = make_graph(&[("a", "c"), ("b", "c")]);
        let result = schedule(&g, &[n("a"), n("b")]).unwrap();
        assert_eq!(result.layer_count(), 2);
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.layers[0].len(), 2);
        assert!(result.layers[0].contains(&n("a")));
        assert!(result.layers[0].contains(&n("b")));
        assert_eq!(result.layers[1], vec![n("c")]);
        assert!(is_valid_topo(&result.layers, &g));
    }

    #[test]
    fn schedule_invalid_entry_nonexistent() {
        let g = make_graph(&[("a", "b")]);
        let err = schedule(&g, &[n("x")]).unwrap_err();
        match err {
            DagError::NodeNotFound(id) => assert_eq!(id, n("x")),
            other => panic!("expected NodeNotFound, got {other:?}"),
        }
    }

    #[test]
    fn schedule_invalid_entry_with_deps() {
        let g = make_graph(&[("a", "b")]);
        let err = schedule(&g, &[n("b")]).unwrap_err();
        match err {
            DagError::InvalidEntry(id) => assert_eq!(id, n("b")),
            other => panic!("expected InvalidEntry, got {other:?}"),
        }
    }

    #[test]
    fn schedule_disconnected_graph() {
        let g = make_graph(&[("a", "b"), ("c", "d")]);
        let result = schedule(&g, &[n("a"), n("c")]).unwrap();
        // Two independent chains: a->b and c->d
        // Layer 0: [a, c], Layer 1: [b, d]
        assert_eq!(result.layer_count(), 2);
        assert_eq!(result.node_count(), 4);
        assert_eq!(result.layers[0].len(), 2);
        assert!(result.layers[0].contains(&n("a")));
        assert!(result.layers[0].contains(&n("c")));
        assert_eq!(result.layers[1].len(), 2);
        assert!(result.layers[1].contains(&n("b")));
        assert!(result.layers[1].contains(&n("d")));
        assert!(is_valid_topo(&result.layers, &g));
    }

    // ── schedule_all() ────────────────────────────────────────────────

    #[test]
    fn schedule_all_empty_graph() {
        let g = DagGraph::new();
        let result = schedule_all(&g).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn schedule_all_finds_roots_automatically() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let result = schedule_all(&g).unwrap();
        assert_eq!(result.node_count(), 3);
        assert!(result.execution_order.contains(&n("a")));
        assert!(result.execution_order.contains(&n("b")));
        assert!(result.execution_order.contains(&n("c")));
        assert!(is_valid_topo(&result.layers, &g));
    }

    #[test]
    fn schedule_all_disconnected_subgraphs() {
        let g = make_graph(&[("a", "b"), ("c", "d"), ("x", "y")]);
        let result = schedule_all(&g).unwrap();
        assert_eq!(result.node_count(), 6);
        // Roots: a, c, x
        assert_eq!(result.layers[0].len(), 3);
        // All nodes appear
        let all: HashSet<&NodeId> = result.execution_order.iter().collect();
        for id in &[n("a"), n("b"), n("c"), n("d"), n("x"), n("y")] {
            assert!(all.contains(id), "{id:?} should be in execution_order");
        }
        assert!(is_valid_topo(&result.layers, &g));
    }

    // ── Execution order correctness ───────────────────────────────────

    #[test]
    fn execution_order_dependency_constraint() {
        // Complex graph: verify no node appears before its dependencies
        let g = make_graph(&[
            ("a", "c"),
            ("b", "c"),
            ("c", "d"),
            ("c", "e"),
            ("d", "f"),
            ("e", "f"),
        ]);
        let result = schedule_all(&g).unwrap();
        // Build position map
        let pos: std::collections::HashMap<&NodeId, usize> = result
            .execution_order
            .iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let adjacency = g.adjacency();
        for (from, to_list) in adjacency.iter() {
            for to in to_list {
                let fp = pos.get(from).expect("from should be in execution_order");
                let tp = pos.get(to).expect("to should be in execution_order");
                assert!(
                    fp < tp,
                    "{from:?} should appear before {to:?} in execution_order"
                );
            }
        }
        assert!(is_valid_topo(&result.layers, &g));
    }

    // ── Large graph ───────────────────────────────────────────────────

    #[test]
    fn schedule_large_graph() {
        let mut g = DagGraph::new();
        for i in 0..100 {
            g.register_node(make_node(&format!("n{i}"))).unwrap();
        }
        // Chain: n0 -> n1 -> ... -> n99
        for i in 0..99 {
            let _ = g.add_edge(&n(&format!("n{i}")), &n(&format!("n{}", i + 1)));
        }
        let result = schedule_all(&g).unwrap();
        assert_eq!(result.node_count(), 100);
        assert_eq!(result.layer_count(), 100);
        // Each layer has exactly one node
        for (i, layer) in result.layers.iter().enumerate() {
            assert_eq!(layer.len(), 1, "layer {i} should have exactly 1 node");
            assert_eq!(layer[0], n(&format!("n{i}")));
        }
        assert_eq!(
            result.execution_order.len(),
            100,
            "execution_order should have 100 nodes"
        );
    }
}
