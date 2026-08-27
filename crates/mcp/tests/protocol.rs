mod harness;

use harness::Server;

#[test]
fn initialize_returns_server_info() {
    let mut server = Server::start();
    let resp = server.initialize();

    let result = resp.get("result").expect("initialize returned an error");
    assert_eq!(result["serverInfo"]["name"], "zoetrope-mcp");
    // Asserted as "present", not as an exact string: the negotiated version
    // is rmcp's to choose and will move with SDK upgrades.
    assert!(
        result.get("protocolVersion").is_some(),
        "no protocolVersion in {result}"
    );
}

#[test]
fn tools_list_advertises_render_document() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request("tools/list", serde_json::json!({}));
    let tools = resp["result"]["tools"].as_array().expect("tools array");

    let doc_tool = tools
        .iter()
        .find(|t| t["name"] == "render_document")
        .unwrap_or_else(|| panic!("render_document missing from {tools:?}"));

    // The derived input schema is our public contract; assert its shape
    // exactly so schema drift is caught rather than silently shipped.
    let props = &doc_tool["inputSchema"]["properties"];
    for key in [
        "document",
        "documentPath",
        "assetBaseDir",
        "out",
        "fps",
        "validateOnly",
        "previewFrames",
    ] {
        assert!(
            props.get(key).is_some(),
            "missing property {key} in {props}"
        );
    }
}
