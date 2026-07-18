use std::collections::{HashMap, HashSet, VecDeque};

use browseros_types::identifiers::NodeId;

use crate::error::DagError;
use crate::node::DagNode;

pub(crate) struct DagGraph {
    nodes: HashMap<NodeId, DagNode>,
    edges: HashMap<NodeId, Vec<NodeId>>,
    reverse_edges: HashMap<NodeId, Vec<NodeId>>,
}

impl DagGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
    }

    pub fn register_node(&mut self, node: DagNode) -> Result<(), DagError> {
        if self.nodes.contains_key(&node.id) {
            return Err(DagError::NodeAlreadyExists(node.id));
        }
        self.edges.entry(node.id.clone()).or_default();
        self.reverse_edges.entry(node.id.clone()).or_default();
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn add_edge(&mut self, from: &NodeId, to: &NodeId) -> Result<(), DagError> {
        if !self.nodes.contains_key(from) {
            return Err(DagError::NodeNotFound(from.clone()));
        }
        if !self.nodes.contains_key(to) {
            return Err(DagError::NodeNotFound(to.clone()));
        }
        if from == to {
            return Err(DagError::SelfLoop(from.clone()));
        }
        if let Some(edges) = self.edges.get(from) {
            if edges.contains(to) {
                return Err(DagError::DuplicateEdge {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        if self.has_path(to, from) {
            return Err(DagError::CycleDetected {
                from: from.clone(),
                to: to.clone(),
            });
        }
        self.edges.entry(from.clone()).or_default().push(to.clone());
        self.reverse_edges
            .entry(to.clone())
            .or_default()
            .push(from.clone());
        Ok(())
    }

    pub fn remove_edge(&mut self, from: &NodeId, to: &NodeId) -> Result<(), DagError> {
        if let Some(edges) = self.edges.get_mut(from) {
            edges.retain(|e| e != to);
        }
        if let Some(rev) = self.reverse_edges.get_mut(to) {
            rev.retain(|e| e != from);
        }
        Ok(())
    }

    pub fn get_node(&self, id: &NodeId) -> Result<&DagNode, DagError> {
        self.nodes
            .get(id)
            .ok_or_else(|| DagError::NodeNotFound(id.clone()))
    }

    pub fn has_node(&self, id: &NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn all_nodes(&self) -> &HashMap<NodeId, DagNode> {
        &self.nodes
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }

    pub fn dependencies_of(&self, node: &NodeId) -> Vec<NodeId> {
        self.reverse_edges.get(node).cloned().unwrap_or_default()
    }

    pub fn dependents_of(&self, node: &NodeId) -> Vec<NodeId> {
        self.edges.get(node).cloned().unwrap_or_default()
    }

    /// Returns a reference to the full adjacency map.
    /// Used by the scheduler module for topological verification.
    pub(crate) fn adjacency(&self) -> &HashMap<NodeId, Vec<NodeId>> {
        &self.edges
    }

    /// Returns true if there is a directed path from `from` to `to` using BFS.
    fn has_path(&self, from: &NodeId, to: &NodeId) -> bool {
        if from == to {
            return true;
        }
        let mut visited: HashSet<&NodeId> = HashSet::new();
        let mut queue: VecDeque<&NodeId> = VecDeque::new();
        queue.push_back(from);
        visited.insert(from);
        while let Some(current) = queue.pop_front() {
            if let Some(dependents) = self.edges.get(current) {
                for next in dependents {
                    if next == to {
                        return true;
                    }
                    if visited.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
        false
    }

    /// Checks if `node` has a self-loop edge.
    fn has_self_loop(&self, node: &NodeId) -> bool {
        self.edges.get(node).is_some_and(|e| e.contains(node))
    }

    /// Tarjan's strongly connected components algorithm.
    ///
    /// Returns SCCs in reverse topological order. An SCC with more than one
    /// node indicates a cycle. A single-node SCC with a self-loop also
    /// indicates a cycle.
    ///
    /// # Complexity
    /// O(V + E) time, O(V) auxiliary space.
    pub fn find_sccs(&self) -> Vec<Vec<NodeId>> {
        let mut index_counter: u64 = 0;
        let mut stack: Vec<NodeId> = Vec::new();
        let mut on_stack: HashSet<NodeId> = HashSet::new();
        let mut indices: HashMap<NodeId, u64> = HashMap::new();
        let mut lowlink: HashMap<NodeId, u64> = HashMap::new();
        let mut sccs: Vec<Vec<NodeId>> = Vec::new();

        for node_id in self.nodes.keys() {
            if !indices.contains_key(node_id) {
                Self::tarjan_strongconnect(
                    node_id.clone(),
                    &mut index_counter,
                    &mut stack,
                    &mut on_stack,
                    &mut indices,
                    &mut lowlink,
                    &mut sccs,
                    &self.edges,
                );
            }
        }

        sccs
    }

    /// Recursive step of Tarjan's algorithm.
    #[allow(clippy::too_many_arguments)]
    fn tarjan_strongconnect(
        v: NodeId,
        index_counter: &mut u64,
        stack: &mut Vec<NodeId>,
        on_stack: &mut HashSet<NodeId>,
        indices: &mut HashMap<NodeId, u64>,
        lowlink: &mut HashMap<NodeId, u64>,
        sccs: &mut Vec<Vec<NodeId>>,
        edges: &HashMap<NodeId, Vec<NodeId>>,
    ) {
        indices.insert(v.clone(), *index_counter);
        lowlink.insert(v.clone(), *index_counter);
        *index_counter += 1;
        stack.push(v.clone());
        on_stack.insert(v.clone());

        if let Some(dependents) = edges.get(&v) {
            for w in dependents {
                if !indices.contains_key(w) {
                    Self::tarjan_strongconnect(
                        w.clone(),
                        index_counter,
                        stack,
                        on_stack,
                        indices,
                        lowlink,
                        sccs,
                        edges,
                    );
                    let v_low = lowlink[&v];
                    let w_low = lowlink[w];
                    lowlink.insert(v.clone(), v_low.min(w_low));
                } else if on_stack.contains(w) {
                    let v_low = lowlink[&v];
                    let w_idx = indices[w];
                    lowlink.insert(v.clone(), v_low.min(w_idx));
                }
            }
        }

        if lowlink[&v] == indices[&v] {
            let mut scc: Vec<NodeId> = Vec::new();
            while let Some(w) = stack.pop() {
                on_stack.remove(&w);
                let is_root = w == v;
                scc.push(w);
                if is_root {
                    break;
                }
            }
            sccs.push(scc);
        }
    }

    /// Returns true if the graph contains at least one cycle.
    pub fn has_cycle(&self) -> bool {
        let sccs = self.find_sccs();
        sccs.iter()
            .any(|scc| scc.len() > 1 || (scc.len() == 1 && self.has_self_loop(&scc[0])))
    }

    /// Finds the first edge that participates in a cycle.
    ///
    /// Returns `(from, to)` such that removing this edge would break the cycle.
    pub fn find_cycle_edge(&self) -> Option<(NodeId, NodeId)> {
        let sccs = self.find_sccs();
        for scc in &sccs {
            if scc.len() > 1 {
                let scc_set: HashSet<&NodeId> = scc.iter().collect();
                for from in scc {
                    if let Some(dependents) = self.edges.get(from) {
                        for to in dependents {
                            if to != from && scc_set.contains(to) {
                                return Some((from.clone(), to.clone()));
                            }
                        }
                    }
                }
            } else if scc.len() == 1 {
                let node = &scc[0];
                if self.has_self_loop(node) {
                    return Some((node.clone(), node.clone()));
                }
            }
        }
        None
    }

    /// Comprehensive graph validation.
    ///
    /// Checks:
    /// - All edge endpoints reference registered nodes
    /// - No self-loops
    /// - No duplicate edges
    /// - No cycles (via Tarjan's SCC)
    /// - Edge/reverse-edge consistency
    /// - Graph data structure internal consistency
    pub fn validate(&self) -> Result<(), DagError> {
        for from in self.edges.keys() {
            if !self.nodes.contains_key(from) {
                return Err(DagError::NodeNotFound(from.clone()));
            }
            if let Some(dependents) = self.edges.get(from) {
                for to in dependents {
                    if !self.nodes.contains_key(to) {
                        return Err(DagError::NodeNotFound(to.clone()));
                    }
                    if from == to {
                        return Err(DagError::SelfLoop(from.clone()));
                    }
                }
            }
        }
        for to in self.reverse_edges.keys() {
            if !self.nodes.contains_key(to) {
                return Err(DagError::NodeNotFound(to.clone()));
            }
        }
        self.check_consistency()?;
        if let Some((from, to)) = self.find_cycle_edge() {
            return Err(DagError::CycleDetected { from, to });
        }
        Ok(())
    }

    /// Kahn's topological sort producing execution layers.
    ///
    /// Returns a vector of layers, where each layer contains nodes that can
    /// execute in parallel. All nodes in layer[i] must complete before any
    /// node in layer[i+1] starts.
    ///
    /// Returns `CycleDetected` if the graph contains cycles, with the actual
    /// edge that forms the cycle reported in the error.
    pub fn topological_sort(&self) -> Result<Vec<Vec<NodeId>>, DagError> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }
        let mut in_degree: HashMap<&NodeId, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.entry(id).or_insert(0);
        }
        for to_list in self.edges.values() {
            for to in to_list {
                *in_degree.entry(to).or_insert(0) += 1;
            }
        }
        let mut queue: Vec<&NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut layers: Vec<Vec<NodeId>> = Vec::new();
        let mut visited: HashSet<NodeId> = HashSet::new();
        while !queue.is_empty() {
            let layer: Vec<NodeId> = queue.drain(..).cloned().collect();
            visited.extend(layer.iter().cloned());
            let mut next_queue: Vec<&NodeId> = Vec::new();
            for node_id in &layer {
                if let Some(dependents) = self.edges.get(node_id) {
                    for dependent in dependents {
                        if let Some(deg) = in_degree.get_mut(dependent) {
                            *deg = deg.saturating_sub(1);
                            if *deg == 0 {
                                next_queue.push(dependent);
                            }
                        }
                    }
                }
            }
            layers.push(layer);
            queue = next_queue;
        }
        if visited.len() != self.nodes.len() {
            if let Some((from, to)) = self.find_cycle_edge() {
                return Err(DagError::CycleDetected { from, to });
            }
        }
        Ok(layers)
    }

    /// Returns all root nodes (nodes with in-degree 0).
    ///
    /// Roots are candidates for entry points in a DAG execution.
    /// Empty graph returns an empty vector.
    ///
    /// # Complexity
    /// O(V + E) time, O(V) space.
    pub fn find_roots(&self) -> Vec<NodeId> {
        let mut in_degree: HashMap<&NodeId, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.entry(id).or_insert(0);
        }
        for to_list in self.edges.values() {
            for to in to_list {
                *in_degree.entry(to).or_insert(0) += 1;
            }
        }
        in_degree
            .into_iter()
            .filter(|(_, deg)| *deg == 0)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Returns all leaf nodes (nodes with out-degree 0).
    ///
    /// Leaves are candidates for exit points in a DAG execution.
    /// Empty graph returns an empty vector.
    ///
    /// # Complexity
    /// O(V) time, O(V) space.
    pub fn find_leaves(&self) -> Vec<NodeId> {
        self.nodes
            .keys()
            .filter(|id| {
                self.edges
                    .get(*id)
                    .map(|deps| deps.is_empty())
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Returns orphan nodes — registered nodes with neither incoming nor
    /// outgoing edges.
    ///
    /// Orphan nodes are structurally valid but may indicate a modeling error
    /// (unintentionally disconnected nodes). They are not reported as errors
    /// by `validate()` since a valid DAG may contain isolated nodes.
    ///
    /// # Complexity
    /// O(V) time, O(V) space.
    pub fn find_orphans(&self) -> Vec<NodeId> {
        self.nodes
            .keys()
            .filter(|id| {
                let has_outgoing = self
                    .edges
                    .get(*id)
                    .map(|deps| !deps.is_empty())
                    .unwrap_or(false);
                let has_incoming = self
                    .reverse_edges
                    .get(*id)
                    .map(|deps| !deps.is_empty())
                    .unwrap_or(false);
                !has_outgoing && !has_incoming
            })
            .cloned()
            .collect()
    }

    /// Returns all nodes that cannot be reached from any root node.
    ///
    /// Performs a BFS starting from all root nodes (in-degree 0). Any
    /// node not visited is unreachable. In a well-formed DAG, unreachable
    /// nodes indicate disconnected subgraphs.
    ///
    /// # Complexity
    /// O(V + E) time, O(V) space.
    pub fn find_unreachable(&self) -> Vec<NodeId> {
        let roots = self.find_roots();
        let mut visited: HashSet<NodeId> = HashSet::new();
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        for root in &roots {
            visited.insert(root.clone());
            queue.push_back(root.clone());
        }
        while let Some(current) = queue.pop_front() {
            if let Some(dependents) = self.edges.get(&current) {
                for next in dependents {
                    if visited.insert(next.clone()) {
                        queue.push_back(next.clone());
                    }
                }
            }
        }
        self.nodes
            .keys()
            .filter(|id| !visited.contains(id))
            .cloned()
            .collect()
    }

    /// Validates that the given nodes are valid entry points for execution.
    ///
    /// Each node must:
    /// - Exist in the graph
    /// - Have in-degree 0 (no incoming edges)
    ///
    /// # Complexity
    /// O(V + E + K) where K = entry_nodes.len().
    pub fn validate_entry_nodes(&self, entry_nodes: &[NodeId]) -> Result<(), DagError> {
        for node_id in entry_nodes {
            if !self.nodes.contains_key(node_id) {
                return Err(DagError::NodeNotFound(node_id.clone()));
            }
            let deps = self.dependencies_of(node_id);
            if !deps.is_empty() {
                return Err(DagError::InvalidEntry(node_id.clone()));
            }
        }
        Ok(())
    }

    /// Validates that the given nodes are valid exit points for execution.
    ///
    /// Each node must:
    /// - Exist in the graph
    /// - Have out-degree 0 (no outgoing edges)
    ///
    /// # Complexity
    /// O(V + E + K) where K = exit_nodes.len().
    pub fn validate_exit_nodes(&self, exit_nodes: &[NodeId]) -> Result<(), DagError> {
        for node_id in exit_nodes {
            if !self.nodes.contains_key(node_id) {
                return Err(DagError::NodeNotFound(node_id.clone()));
            }
            let deps = self.dependents_of(node_id);
            if !deps.is_empty() {
                return Err(DagError::InvalidExit(node_id.clone()));
            }
        }
        Ok(())
    }

    /// Checks internal consistency of the graph data structure.
    ///
    /// Verifies that every edge in `edges` has a matching reverse entry in
    /// `reverse_edges` and vice versa. This detects data corruption from
    /// direct field manipulation or deserialization errors.
    ///
    /// # Complexity
    /// O(V + E) time, O(1) auxiliary space.
    pub fn check_consistency(&self) -> Result<(), DagError> {
        for (from, to_list) in &self.edges {
            for to in to_list {
                let has_reverse = self
                    .reverse_edges
                    .get(to)
                    .map(|rev| rev.contains(from))
                    .unwrap_or(false);
                if !has_reverse {
                    return Err(DagError::GraphInconsistent);
                }
            }
        }
        for (to, from_list) in &self.reverse_edges {
            for from in from_list {
                let has_forward = self
                    .edges
                    .get(from)
                    .map(|fwd| fwd.contains(to))
                    .unwrap_or(false);
                if !has_forward {
                    return Err(DagError::GraphInconsistent);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Builds a graph that may contain cycles by directly inserting edges,
    /// bypassing add_edge's cycle prevention. Used to test Tarjan SCC
    /// detection, has_cycle, validate, and topological_sort on cyclic graphs.
    fn make_cyclic_graph(pairs: &[(&str, &str)]) -> DagGraph {
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
            let f = n(from);
            let t = n(to);
            g.edges.entry(f).or_default().push(t.clone());
            g.reverse_edges.entry(t).or_default().push(n(from));
        }
        g
    }

    fn is_valid_topo(layers: &[Vec<NodeId>], g: &DagGraph) -> bool {
        let pos: HashMap<&NodeId, usize> = layers
            .iter()
            .enumerate()
            .flat_map(|(i, layer)| layer.iter().map(move |id| (id, i)))
            .collect();
        for from in g.edges.keys() {
            if let Some(deps) = g.edges.get(from) {
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

    // ── Empty / Single ────────────────────────────────────────────────

    #[test]
    fn empty_graph_topological_sort_succeeds() {
        let g = DagGraph::new();
        let layers = g.topological_sort();
        assert!(layers.is_ok());
        assert!(layers.unwrap().is_empty());
    }

    #[test]
    fn empty_graph_has_no_cycle() {
        let g = DagGraph::new();
        assert!(!g.has_cycle());
        assert!(g.find_cycle_edge().is_none());
        assert!(g.validate().is_ok());
    }

    #[test]
    fn single_node_topological_sort() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let layers = g.topological_sort().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0], vec![n("a")]);
    }

    // ── Topological Order ─────────────────────────────────────────────

    #[test]
    fn linear_chain_sort() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let layers = g.topological_sort().unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![n("a")]);
        assert_eq!(layers[1], vec![n("b")]);
        assert_eq!(layers[2], vec![n("c")]);
        assert!(is_valid_topo(&layers, &g));
    }

    #[test]
    fn parallel_nodes_sort() {
        let g = make_graph(&[("a", "c"), ("b", "c")]);
        let layers = g.topological_sort().unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 2);
        assert!(layers[0].contains(&n("a")));
        assert!(layers[0].contains(&n("b")));
        assert_eq!(layers[1], vec![n("c")]);
        assert!(is_valid_topo(&layers, &g));
    }

    #[test]
    fn diamond_sort() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let layers = g.topological_sort().unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![n("a")]);
        assert!(layers[1].contains(&n("b")));
        assert!(layers[1].contains(&n("c")));
        assert_eq!(layers[2], vec![n("d")]);
        assert!(is_valid_topo(&layers, &g));
    }

    #[test]
    fn disconnected_graph_sort() {
        let g = make_graph(&[("a", "b"), ("c", "d")]);
        let layers = g.topological_sort().unwrap();
        let all: HashSet<NodeId> = layers.iter().flat_map(|l| l.iter()).cloned().collect();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&n("a")));
        assert!(all.contains(&n("b")));
        assert!(all.contains(&n("c")));
        assert!(all.contains(&n("d")));
        assert!(is_valid_topo(&layers, &g));
    }

    #[test]
    fn complex_dag_sort() {
        let g = make_graph(&[
            ("a", "b"),
            ("a", "c"),
            ("b", "d"),
            ("c", "d"),
            ("d", "e"),
            ("a", "f"),
            ("f", "g"),
            ("g", "e"),
            ("h", "i"),
        ]);
        let layers = g.topological_sort().unwrap();
        let all: HashSet<NodeId> = layers.iter().flat_map(|l| l.iter()).cloned().collect();
        assert_eq!(all.len(), 9);
        assert!(is_valid_topo(&layers, &g));
        assert!(layers.iter().any(|l| l.contains(&n("a"))));
        assert!(layers.iter().any(|l| l.contains(&n("h"))));
    }

    // ── Cycle Detection (Tarjan) ──────────────────────────────────────

    #[test]
    fn simple_cycle_detected_at_add_edge() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        let err = g.add_edge(&n("b"), &n("a")).unwrap_err();
        match err {
            DagError::CycleDetected { from, to } => {
                assert_eq!(from, n("b"));
                assert_eq!(to, n("a"));
            }
            other => panic!("expected CycleDetected, got {other:?}"),
        }
    }

    #[test]
    fn simple_cycle_graph_has_cycle() {
        let g = make_cyclic_graph(&[("a", "b"), ("b", "c"), ("c", "a")]);
        assert!(g.has_cycle());
        let _edge = g.find_cycle_edge().expect("cycle edge must exist");
        let sccs = g.find_sccs();
        let has_big_scc = sccs.iter().any(|scc| scc.len() > 1);
        assert!(has_big_scc);
    }

    #[test]
    fn self_loop_rejected_at_add_edge() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let err = g.add_edge(&n("a"), &n("a")).unwrap_err();
        match err {
            DagError::SelfLoop(node) => assert_eq!(node, n("a")),
            other => panic!("expected SelfLoop, got {other:?}"),
        }
    }

    #[test]
    fn self_loop_detected_by_validate() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.edges.entry(n("a")).or_default().push(n("a"));
        g.reverse_edges.entry(n("a")).or_default().push(n("a"));
        assert!(g.has_cycle());
        let err = g.validate().unwrap_err();
        match err {
            DagError::SelfLoop(node) => assert_eq!(node, n("a")),
            DagError::CycleDetected { .. } => { /* also acceptable */ }
            other => panic!("expected SelfLoop or CycleDetected, got {other:?}"),
        }
    }

    #[test]
    fn nested_cycle_detected() {
        let g = make_cyclic_graph(&[
            ("a", "b"),
            ("b", "c"),
            ("c", "d"),
            ("d", "e"),
            ("e", "f"),
            ("f", "c"),
        ]);
        assert!(g.has_cycle());
        let _edge = g.find_cycle_edge().expect("cycle edge must exist");
    }

    #[test]
    fn multiple_sccs_no_cycle() {
        let g = make_graph(&[("a", "b"), ("b", "c"), ("x", "y")]);
        assert!(!g.has_cycle());
        let sccs = g.find_sccs();
        assert_eq!(sccs.len(), 5);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn multiple_sccs_with_cycle() {
        let g = make_cyclic_graph(&[("a", "b"), ("b", "c"), ("c", "a"), ("x", "y")]);
        assert!(g.has_cycle());
        let sccs = g.find_sccs();
        let big_sccs: Vec<_> = sccs.iter().filter(|scc| scc.len() > 1).collect();
        assert_eq!(big_sccs.len(), 1);
        assert!(big_sccs[0].contains(&n("a")));
        assert!(big_sccs[0].contains(&n("b")));
        assert!(big_sccs[0].contains(&n("c")));
    }

    #[test]
    fn topological_sort_rejects_cycle() {
        let g = make_cyclic_graph(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let err = g.topological_sort().unwrap_err();
        match err {
            DagError::CycleDetected { from, to } => {
                assert!(from == n("a") || from == n("b") || from == n("c"));
                assert!(to == n("a") || to == n("b") || to == n("c"));
            }
            other => panic!("expected CycleDetected, got {other:?}"),
        }
    }

    #[test]
    fn topological_sort_rejects_self_loop() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.edges.entry(n("a")).or_default().push(n("a"));
        g.reverse_edges.entry(n("a")).or_default().push(n("a"));
        let result = g.topological_sort();
        assert!(result.is_err());
    }

    // ── Node / Edge Rejection ─────────────────────────────────────────

    #[test]
    fn duplicate_node_rejected() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let err = g.register_node(make_node("a")).unwrap_err();
        match err {
            DagError::NodeAlreadyExists(id) => assert_eq!(id, n("a")),
            other => panic!("expected NodeAlreadyExists, got {other:?}"),
        }
    }

    #[test]
    fn invalid_edge_from_nonexistent_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let err = g.add_edge(&n("x"), &n("a")).unwrap_err();
        match err {
            DagError::NodeNotFound(id) => assert_eq!(id, n("x")),
            other => panic!("expected NodeNotFound, got {other:?}"),
        }
    }

    #[test]
    fn invalid_edge_to_nonexistent_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let err = g.add_edge(&n("a"), &n("x")).unwrap_err();
        match err {
            DagError::NodeNotFound(id) => assert_eq!(id, n("x")),
            other => panic!("expected NodeNotFound, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_edge_rejected() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        let err = g.add_edge(&n("a"), &n("b")).unwrap_err();
        match err {
            DagError::DuplicateEdge { from, to } => {
                assert_eq!(from, n("a"));
                assert_eq!(to, n("b"));
            }
            other => panic!("expected DuplicateEdge, got {other:?}"),
        }
    }

    // ── Validate ──────────────────────────────────────────────────────

    #[test]
    fn validate_acyclic_graph_succeeds() {
        let g = make_graph(&[("a", "b"), ("b", "c"), ("a", "c")]);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn validate_cycle_graph_fails() {
        let g = make_cyclic_graph(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let err = g.validate().unwrap_err();
        match err {
            DagError::CycleDetected { .. } => {} // expected
            other => panic!("expected CycleDetected, got {other:?}"),
        }
    }

    #[test]
    fn validate_disconnected_graph_succeeds() {
        let g = make_graph(&[("a", "b"), ("c", "d"), ("e", "f")]);
        assert!(g.validate().is_ok());
    }

    // ── Tarjan SCC Correctness ────────────────────────────────────────

    #[test]
    fn tarjan_acyclic_returns_singletons() {
        let g = make_graph(&[("a", "b"), ("b", "c"), ("b", "d")]);
        let sccs = g.find_sccs();
        for scc in &sccs {
            assert_eq!(scc.len(), 1, "SCC {scc:?} is unexpectedly non-trivial");
        }
        assert_eq!(sccs.len(), 4);
    }

    #[test]
    fn tarjan_diamond_returns_singletons() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let sccs = g.find_sccs();
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn tarjan_complex_multi_scc() {
        let g = make_cyclic_graph(&[
            ("a", "b"),
            ("b", "c"),
            ("c", "a"),
            ("x", "y"),
            ("y", "z"),
            ("z", "y"),
            ("m", "n"),
        ]);
        let sccs = g.find_sccs();
        let big: Vec<_> = sccs.iter().filter(|scc| scc.len() > 1).collect();
        assert_eq!(big.len(), 2, "expected 2 non-trivial SCCs");
    }

    #[test]
    fn tarjan_no_false_positive() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d"), ("d", "e")]);
        let sccs = g.find_sccs();
        for scc in &sccs {
            assert_eq!(scc.len(), 1, "false positive cycle: {scc:?}");
        }
    }

    // ── Edge Cases ────────────────────────────────────────────────────

    #[test]
    fn has_cycle_false_on_acyclic() {
        let g = make_graph(&[("a", "b")]);
        assert!(!g.has_cycle());
    }

    #[test]
    fn find_cycle_edge_none_on_acyclic() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        assert!(g.find_cycle_edge().is_none());
    }

    #[test]
    fn remove_edge_does_not_cause_errors() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        g.remove_edge(&n("a"), &n("b")).unwrap();
        assert!(!g.has_cycle());
        let layers = g.topological_sort().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].len(), 2);
    }

    #[test]
    fn node_count_returns_correct_count() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        assert_eq!(g.node_count(), 3);
    }

    #[test]
    fn edge_count_returns_correct_count() {
        let g = make_graph(&[("a", "b"), ("b", "c"), ("a", "c")]);
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn remove_nonexistent_edge_succeeds() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        assert!(g.remove_edge(&n("a"), &n("b")).is_ok());
    }

    #[test]
    fn has_node_returns_true_for_registered() {
        let mut g = DagGraph::new();
        g.register_node(make_node("z")).unwrap();
        assert!(g.has_node(&n("z")));
        assert!(!g.has_node(&n("nonexistent")));
    }

    #[test]
    fn dependencies_of_returns_empty_for_root() {
        let g = make_graph(&[("a", "b")]);
        assert!(g.dependencies_of(&n("a")).is_empty());
    }

    #[test]
    fn dependents_of_returns_empty_for_leaf() {
        let g = make_graph(&[("a", "b")]);
        assert!(g.dependents_of(&n("b")).is_empty());
    }

    // ── Roots and Leaves ───────────────────────────────────────────────

    #[test]
    fn find_roots_empty_graph() {
        let g = DagGraph::new();
        let roots = g.find_roots();
        assert!(roots.is_empty());
    }

    #[test]
    fn find_roots_single_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let roots = g.find_roots();
        assert_eq!(roots, vec![n("a")]);
    }

    #[test]
    fn find_roots_linear_chain() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let roots = g.find_roots();
        assert_eq!(roots, vec![n("a")]);
    }

    #[test]
    fn find_roots_diamond() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let roots = g.find_roots();
        assert_eq!(roots, vec![n("a")]);
    }

    #[test]
    fn find_roots_disconnected() {
        let g = make_graph(&[("a", "b"), ("c", "d")]);
        let roots: HashSet<NodeId> = g.find_roots().into_iter().collect();
        assert_eq!(roots.len(), 2);
        assert!(roots.contains(&n("a")));
        assert!(roots.contains(&n("c")));
    }

    #[test]
    fn find_leaves_empty_graph() {
        let g = DagGraph::new();
        let leaves = g.find_leaves();
        assert!(leaves.is_empty());
    }

    #[test]
    fn find_leaves_single_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let leaves = g.find_leaves();
        assert_eq!(leaves, vec![n("a")]);
    }

    #[test]
    fn find_leaves_linear_chain() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let leaves = g.find_leaves();
        assert_eq!(leaves, vec![n("c")]);
    }

    #[test]
    fn find_leaves_diamond() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        let leaves = g.find_leaves();
        assert_eq!(leaves, vec![n("d")]);
    }

    #[test]
    fn find_leaves_disconnected() {
        let g = make_graph(&[("a", "b"), ("c", "d")]);
        let leaves: HashSet<NodeId> = g.find_leaves().into_iter().collect();
        assert_eq!(leaves.len(), 2);
        assert!(leaves.contains(&n("b")));
        assert!(leaves.contains(&n("d")));
    }

    // ── Orphans ────────────────────────────────────────────────────────

    #[test]
    fn find_orphans_empty_graph() {
        let g = DagGraph::new();
        let orphans = g.find_orphans();
        assert!(orphans.is_empty());
    }

    #[test]
    fn find_orphans_single_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let orphans = g.find_orphans();
        assert_eq!(orphans, vec![n("a")]);
    }

    #[test]
    fn find_orphans_none_in_connected_graph() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let orphans = g.find_orphans();
        assert!(orphans.is_empty());
    }

    #[test]
    fn find_orphans_some_orphans() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.register_node(make_node("c")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        // 'c' is an orphan
        let orphans = g.find_orphans();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0], n("c"));
    }

    #[test]
    fn find_orphans_all_orphans_when_no_edges() {
        let mut g = DagGraph::new();
        g.register_node(make_node("x")).unwrap();
        g.register_node(make_node("y")).unwrap();
        let orphans: HashSet<NodeId> = g.find_orphans().into_iter().collect();
        assert_eq!(orphans.len(), 2);
        assert!(orphans.contains(&n("x")));
        assert!(orphans.contains(&n("y")));
    }

    // ── Unreachable Nodes ─────────────────────────────────────────────

    #[test]
    fn find_unreachable_empty_graph() {
        let g = DagGraph::new();
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn find_unreachable_single_node() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn find_unreachable_none_in_connected() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn find_unreachable_disconnected_subgraph() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.register_node(make_node("x")).unwrap();
        g.register_node(make_node("y")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        // x and y are disconnected subgraph — they are roots but
        // find_unreachable BFS starts from ALL roots (including x, y),
        // so they ARE reachable from themselves.
        // To get truly unreachable nodes, we need a chain where a
        // non-root node has no path from any root.
        // Actually, a disconnected subgraph with a root and leaf:
        // x -> y is reachable from root x, so not unreachable.
        // Let's create a node with incoming edge but no root path:
        // register 'z' and manually add an edge from nonexistent root.
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn find_unreachable_with_orphan() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.add_edge(&n("a"), &n("b")).unwrap();
        g.register_node(make_node("orphan")).unwrap();
        // The orphan has no edges at all — it IS a root (in-degree 0),
        // so find_unreachable (starting from all roots) DOES reach it.
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn find_unreachable_node_with_only_incoming_no_root() {
        // Create a node 'target' that is NOT a root (it has an incoming
        // edge from 'mid'), but 'mid' itself is not a root (has edge
        // from 'ghost'), and 'ghost' is NOT registered. This means the
        // edge ghost -> mid must be manually inserted.
        let mut g = DagGraph::new();
        g.register_node(make_node("mid")).unwrap();
        g.register_node(make_node("target")).unwrap();
        // Manually add ghost -> mid and mid -> target
        let ghost = n("ghost");
        g.edges.entry(ghost).or_default().push(n("mid"));
        g.reverse_edges
            .entry(n("mid"))
            .or_default()
            .push(n("ghost"));
        g.edges.entry(n("mid")).or_default().push(n("target"));
        g.reverse_edges
            .entry(n("target"))
            .or_default()
            .push(n("mid"));

        // 'mid' has in-degree 1 (from ghost) but ghost is not registered
        // and has no incoming edges itself. Since ghost is a root BUT
        // ghost is NOT in self.nodes, find_roots won't see it.
        let unreachable: HashSet<NodeId> = g.find_unreachable().into_iter().collect();
        assert!(unreachable.contains(&n("mid")), "mid should be unreachable");
        assert!(
            unreachable.contains(&n("target")),
            "target should be unreachable"
        );
    }

    // ── Entry Node Validation ─────────────────────────────────────────

    #[test]
    fn validate_entry_nodes_valid() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        assert!(g.validate_entry_nodes(&[n("a")]).is_ok());
    }

    #[test]
    fn validate_entry_nodes_multiple_valid() {
        let g = make_graph(&[("a", "c"), ("b", "c")]);
        assert!(g.validate_entry_nodes(&[n("a"), n("b")]).is_ok());
    }

    #[test]
    fn validate_entry_nodes_nonexistent() {
        let g = make_graph(&[("a", "b")]);
        let err = g.validate_entry_nodes(&[n("x")]).unwrap_err();
        match err {
            DagError::NodeNotFound(id) => assert_eq!(id, n("x")),
            other => panic!("expected NodeNotFound, got {other:?}"),
        }
    }

    #[test]
    fn validate_entry_node_with_deps_fails() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let err = g.validate_entry_nodes(&[n("b")]).unwrap_err();
        match err {
            DagError::InvalidEntry(id) => assert_eq!(id, n("b")),
            other => panic!("expected InvalidEntry, got {other:?}"),
        }
    }

    #[test]
    fn validate_entry_node_leaf_fails() {
        let g = make_graph(&[("a", "b")]);
        let err = g.validate_entry_nodes(&[n("b")]).unwrap_err();
        match err {
            DagError::InvalidEntry(id) => assert_eq!(id, n("b")),
            other => panic!("expected InvalidEntry, got {other:?}"),
        }
    }

    // ── Exit Node Validation ──────────────────────────────────────────

    #[test]
    fn validate_exit_nodes_valid() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        assert!(g.validate_exit_nodes(&[n("c")]).is_ok());
    }

    #[test]
    fn validate_exit_nodes_multiple_valid() {
        let g = make_graph(&[("a", "b"), ("a", "c")]);
        assert!(g.validate_exit_nodes(&[n("b"), n("c")]).is_ok());
    }

    #[test]
    fn validate_exit_nodes_nonexistent() {
        let g = make_graph(&[("a", "b")]);
        let err = g.validate_exit_nodes(&[n("x")]).unwrap_err();
        match err {
            DagError::NodeNotFound(id) => assert_eq!(id, n("x")),
            other => panic!("expected NodeNotFound, got {other:?}"),
        }
    }

    #[test]
    fn validate_exit_node_with_deps_fails() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        let err = g.validate_exit_nodes(&[n("b")]).unwrap_err();
        match err {
            DagError::InvalidExit(id) => assert_eq!(id, n("b")),
            other => panic!("expected InvalidExit, got {other:?}"),
        }
    }

    #[test]
    fn validate_exit_node_root_fails() {
        let g = make_graph(&[("a", "b")]);
        let err = g.validate_exit_nodes(&[n("a")]).unwrap_err();
        match err {
            DagError::InvalidExit(id) => assert_eq!(id, n("a")),
            other => panic!("expected InvalidExit, got {other:?}"),
        }
    }

    // ── Graph Consistency ─────────────────────────────────────────────

    #[test]
    fn check_consistency_valid_graph() {
        let g = make_graph(&[("a", "b"), ("b", "c")]);
        assert!(g.check_consistency().is_ok());
    }

    #[test]
    fn check_consistency_empty_graph() {
        let g = DagGraph::new();
        assert!(g.check_consistency().is_ok());
    }

    #[test]
    fn check_consistency_missing_reverse_edge() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.edges.entry(n("a")).or_default().push(n("b"));
        // No reverse edge for b <- a
        assert!(g.check_consistency().is_err());
    }

    #[test]
    fn check_consistency_orphan_reverse_edge() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.reverse_edges.entry(n("b")).or_default().push(n("a"));
        // No forward edge for a -> b
        assert!(g.check_consistency().is_err());
    }

    #[test]
    fn validate_inconsistent_graph_fails() {
        let mut g = DagGraph::new();
        g.register_node(make_node("a")).unwrap();
        g.register_node(make_node("b")).unwrap();
        g.edges.entry(n("a")).or_default().push(n("b"));
        // Missing reverse edge — validate should catch this
        let err = g.validate().unwrap_err();
        match err {
            DagError::GraphInconsistent => {} // expected
            other => panic!("expected GraphInconsistent, got {other:?}"),
        }
    }

    // ── Large Graph Validation ────────────────────────────────────────

    #[test]
    fn large_graph_roots_and_leaves() {
        let mut g = DagGraph::new();
        for i in 0..10 {
            g.register_node(make_node(&format!("n{i}"))).unwrap();
        }
        // n0 -> n1 -> n2 -> n3
        for i in 0..3 {
            let _ = g.add_edge(&n(&format!("n{i}")), &n(&format!("n{}", i + 1)));
        }
        // n4 -> n5
        let _ = g.add_edge(&n("n4"), &n("n5"));
        // n6 isolated
        // n7 -> n8 -> n9
        let _ = g.add_edge(&n("n7"), &n("n8"));
        let _ = g.add_edge(&n("n8"), &n("n9"));

        let roots: HashSet<NodeId> = g.find_roots().into_iter().collect();
        assert!(roots.contains(&n("n0")));
        assert!(roots.contains(&n("n4")));
        assert!(roots.contains(&n("n6")));
        assert!(roots.contains(&n("n7")));

        let leaves: HashSet<NodeId> = g.find_leaves().into_iter().collect();
        assert!(leaves.contains(&n("n3")));
        assert!(leaves.contains(&n("n5")));
        assert!(leaves.contains(&n("n6")));
        assert!(leaves.contains(&n("n9")));

        let orphans = g.find_orphans();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0], n("n6"));

        let unreachable = g.find_unreachable();
        assert!(
            unreachable.is_empty(),
            "all nodes are roots or reachable from roots"
        );

        assert!(g.validate().is_ok());
    }

    // ── Integration: validate_entry_nodes + validate_exit_nodes ───────

    #[test]
    fn full_validation_pipeline() {
        let g = make_graph(&[("a", "b"), ("b", "c"), ("c", "d")]);
        // All validations pass
        assert!(g.validate().is_ok());
        assert!(g.validate_entry_nodes(&[n("a")]).is_ok());
        assert!(g.validate_exit_nodes(&[n("d")]).is_ok());
        // Check correctness
        let roots = g.find_roots();
        assert_eq!(roots, vec![n("a")]);
        let leaves = g.find_leaves();
        assert_eq!(leaves, vec![n("d")]);
        let orphans = g.find_orphans();
        assert!(orphans.is_empty());
        let unreachable = g.find_unreachable();
        assert!(unreachable.is_empty());
    }

    #[test]
    fn diamond_full_validation() {
        let g = make_graph(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        assert!(g.validate().is_ok());
        assert!(g.validate_entry_nodes(&[n("a")]).is_ok());
        assert!(g.validate_exit_nodes(&[n("d")]).is_ok());
        assert_eq!(g.find_roots(), vec![n("a")]);
        assert_eq!(g.find_leaves(), vec![n("d")]);
    }
}
