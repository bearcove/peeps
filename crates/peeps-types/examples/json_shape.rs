use peeps_types::{Edge, EdgeKind, GraphReply, GraphSnapshot, Node, NodeKind};

fn main() {
    let reply = GraphReply {
        r#type: "graph_reply".to_string(),
        snapshot_id: 123,
        process: "swift-vixenfs".to_string(),
        pid: 42,
        graph: Some(GraphSnapshot {
            process_name: "swift-vixenfs".to_string(),
            proc_key: "swift-vixenfs-42".to_string(),
            nodes: vec![Node {
                id: "request:abc".to_string(),
                kind: NodeKind::Request,
                label: Some("req".to_string()),
                attrs_json: "{}".to_string(),
            }],
            edges: vec![Edge {
                src: "request:abc".to_string(),
                dst: "response:def".to_string(),
                kind: EdgeKind::Needs,
                attrs_json: "{}".to_string(),
            }],
        }),
    };
    let json = facet_json::to_string(&reply);
    println!("{}", json);
    let decoded: GraphReply = facet_json::from_slice(json.as_bytes()).unwrap();
    println!("decoded kind={} edge={}", decoded.graph.unwrap().nodes[0].kind.as_str(), reply.graph.unwrap().edges[0].kind.as_str());
}
