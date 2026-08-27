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

#[test]
fn resources_list_includes_the_schema_and_the_corpus() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request("resources/list", serde_json::json!({}));
    let resources = resp["result"]["resources"]
        .as_array()
        .expect("resources array");

    assert!(
        resources
            .iter()
            .any(|r| r["uri"] == "zoetrope://schema/document"),
        "schema resource missing from {resources:?}"
    );
    assert!(
        resources
            .iter()
            .filter(|r| r["uri"]
                .as_str()
                .is_some_and(|u| u.starts_with("zoetrope://corpus/")))
            .count()
            > 0,
        "no corpus resources in {resources:?}"
    );
    assert!(resources
        .iter()
        .all(|r| r["mimeType"] == "application/json"));
}

#[test]
fn reading_a_corpus_resource_returns_a_renderable_document() {
    let mut server = Server::start();
    server.initialize();

    let list = server.request("resources/list", serde_json::json!({}));
    let uri = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|r| {
            r["uri"]
                .as_str()
                .filter(|u| u.starts_with("zoetrope://corpus/"))
                .map(str::to_string)
        })
        .expect("a corpus resource");

    let resp = server.request("resources/read", serde_json::json!({ "uri": uri }));
    let text = resp["result"]["contents"][0]["text"]
        .as_str()
        .expect("text contents");

    // The examples we hand a model must actually be valid documents.
    zoetrope_core::Document::from_json(text).expect("corpus resource is a valid document");
}

#[test]
fn reading_an_unknown_uri_is_an_error() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request(
        "resources/read",
        serde_json::json!({ "uri": "zoetrope://corpus/does-not-exist" }),
    );
    assert!(
        resp.get("error").is_some(),
        "expected a JSON-RPC error, got {resp}"
    );
}
