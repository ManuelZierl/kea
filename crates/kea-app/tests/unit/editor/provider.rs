use super::*;

#[test]
fn accepts_unicode_byte_ranges_but_not_stale_or_invalid_edits() {
    let response = br#"{"protocol":1,"request_id":7,"context":"remote","candidates":[{"label":"name","replacement":"name","start":5,"end":6}]}"#;
    let text = "😀 x suffix";
    let candidates = validate_response(response, 7, "remote", text).unwrap();
    assert_eq!(candidates[0].range, 5..6);
    assert!(validate_response(response, 8, "remote", text).is_err());
    assert!(validate_response(response, 7, "local", text).is_err());
    assert!(validate_response(response, 7, "remote", "tiny").is_err());
    let invalid = String::from_utf8(response.to_vec())
        .unwrap()
        .replace("\"start\":5", "\"start\":1");
    assert!(validate_response(invalid.as_bytes(), 7, "remote", text).is_err());
}

#[test]
fn protocol_round_trip_is_separate_from_terminal_input() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        token: Some("test-token".into()),
    };
    let server = std::thread::spawn(move || {
        use std::io::BufRead;
        let (mut socket, _) = listener.accept().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(socket.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["context"], "remote");
        assert_eq!(request["token"], "test-token");
        let reply = serde_json::json!({"protocol":1,"request_id":request["request_id"],"context":"remote","candidates":[{"label":"hello","replacement":"hello","start":0,"end":2}]});
        writeln!(socket, "{reply}").unwrap();
    });
    let candidates = endpoint.complete(3, "remote", "he", 2).unwrap();
    assert_eq!(candidates[0].replacement, "hello");
    server.join().unwrap();
}

#[test]
fn endpoint_configuration_cannot_enable_remote_network_connections() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), r#"{"remote":{"address":"192.0.2.1:3000"}}"#).unwrap();
    assert!(Providers::load_file(file.path()).is_err());
}
