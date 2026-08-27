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
