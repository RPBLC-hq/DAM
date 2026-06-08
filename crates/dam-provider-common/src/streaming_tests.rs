use super::*;
use futures_util::TryStreamExt;

async fn collect_text(stream: ProviderByteStream) -> String {
    let chunks = stream.try_collect::<Vec<_>>().await.unwrap();
    let bytes = chunks.into_iter().flatten().collect::<Vec<_>>();
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn transforms_reference_split_across_chunks() {
    let reference = "[email:1111111111111111111111]";
    let first = format!("event: delta\ndata: prefix {reference}");
    let split = first.len() - 8;
    let chunks = stream::iter([
        Ok(Bytes::from(first[..split].to_string())),
        Ok(Bytes::from(format!("{} suffix\n\n", &first[split..]))),
    ]);

    let body = collect_text(transform_streaming_body(chunks, move |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(reference, "alice@example.com"))
    }))
    .await;

    assert!(body.contains("alice@example.com"));
    assert!(!body.contains("[email:"));
}

#[tokio::test]
async fn flushes_short_final_chunks() {
    let chunks = stream::iter([Ok(Bytes::from_static(b"short body"))]);

    let body = collect_text(transform_streaming_body(chunks, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("short", "resolved"))
    }))
    .await;

    assert_eq!(body, "resolved body");
}

#[tokio::test]
async fn emits_invalid_long_prefix_without_transforming() {
    let chunks = stream::iter([
        Ok(Bytes::from(vec![0xff; STREAM_TRANSFORM_TAIL_BYTES + 8])),
        Ok(Bytes::from_static(b"done")),
    ]);

    let output = transform_streaming_body(chunks, |_| Bytes::from_static(b"transformed"))
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    assert!(output.starts_with(&[0xff; 8]));
    assert!(output.ends_with(b"transformed"));
}

#[tokio::test]
async fn resolves_reference_split_across_sse_text_delta_events() {
    let chunks = stream::iter([
        Ok(Bytes::from_static(
            br#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"[email:abc"}}

"#,
        )),
        Ok(Bytes::from_static(
            br#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"123]"}}

"#,
        )),
    ]);

    let body = collect_text(transform_event_stream_text_body(chunks, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("[email:abc123]", "banana@example.test"))
    }))
    .await;

    assert!(body.contains("banana@example.test"));
    assert!(!body.contains("[email:abc123]"));
    assert!(body.contains(r#""text":"""#));
    assert!(body.contains("event: content_block_delta"));
}

#[tokio::test]
async fn event_stream_transform_emits_before_end_of_stream() {
    let events = (0..8)
        .map(|index| {
            Ok(Bytes::from(format!(
                "event: delta\ndata: {{\"text\":\"chunk-{index}\"}}\n\n"
            )))
        })
        .collect::<Vec<_>>();

    let chunks = transform_event_stream_text_body(stream::iter(events), |chunk| chunk)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    assert!(chunks.len() > 1);
    assert!(
        String::from_utf8(chunks.concat())
            .unwrap()
            .contains("chunk-0")
    );
}

#[tokio::test]
async fn event_stream_transform_handles_crlf_event_boundaries() {
    let chunks = stream::iter([
        Ok(Bytes::from_static(
            b"event: delta\r\ndata: {\"text\":\"[email:abc\"}\r\n\r\n",
        )),
        Ok(Bytes::from_static(
            b"event: delta\r\ndata: {\"text\":\"123]\"}\r\n\r\n",
        )),
        Ok(Bytes::from_static(
            b"event: delta\r\ndata: {\"text\":\"done\"}\r\n\r\n",
        )),
        Ok(Bytes::from_static(
            b"event: delta\r\ndata: {\"text\":\"tail\"}\r\n\r\n",
        )),
        Ok(Bytes::from_static(
            b"event: delta\r\ndata: {\"text\":\"finish\"}\r\n\r\n",
        )),
    ]);

    let body = collect_text(transform_event_stream_text_body(chunks, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("[email:abc123]", "banana@example.test"))
    }))
    .await;

    assert!(body.contains("banana@example.test"));
    assert!(!body.contains("[email:abc123]"));
}
