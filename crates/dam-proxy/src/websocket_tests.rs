use super::*;

#[tokio::test]
async fn masked_frame_round_trips_to_unmasked_payload() {
    let mut raw = vec![0x81, 0x85, 1, 2, 3, 4];
    let mut payload = b"hello".to_vec();
    apply_mask(&mut payload, [1, 2, 3, 4]);
    raw.extend_from_slice(&payload);

    let frame = read_frame(&mut raw.as_slice()).await.unwrap().unwrap();

    assert!(frame.is_unfragmented_text());
    assert_eq!(frame.payload, b"hello");
}

#[tokio::test]
async fn unmasked_close_frame_uses_server_to_client_framing() {
    let mut output = Vec::new();
    write_unmasked_frame(&mut output, &WebSocketFrame::close(1008, "blocked"))
        .await
        .unwrap();

    assert_eq!(output[0], 0x80 | OPCODE_CLOSE);
    assert_eq!(output[1] & 0x80, 0);
    assert_eq!(&output[2..4], &1008_u16.to_be_bytes());
    assert!(output.ends_with(b"blocked"));
}

#[test]
fn upgrade_detection_accepts_standard_websocket_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONNECTION, "keep-alive, Upgrade".parse().unwrap());
    headers.insert(header::UPGRADE, "websocket".parse().unwrap());
    headers.insert("sec-websocket-key", "test".parse().unwrap());

    assert!(is_upgrade_request(&Method::GET, &headers));
}

#[test]
fn response_filter_removes_extension_negotiation() {
    let raw = b"HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\nupgrade: websocket\r\nsec-websocket-extensions: permessage-deflate\r\n\r\n";
    let filtered = filter_response_header_bytes(raw).unwrap();
    let text = String::from_utf8(filtered).unwrap();

    assert!(!text.contains("sec-websocket-extensions"));
    assert!(text.ends_with("\r\n\r\n"));
}
