//! Local HTTP fixtures: no external lyric requests or private song metadata.
use mira_core::lrclib::{LrclibClient, TrackQuery};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
fn server(
    responses: Vec<(&'static str, &'static str)>,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fixture did not receive a request: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = vec![0u8; 8192];
            let count = stream.read(&mut request).unwrap();
            requests.push(
                String::from_utf8_lossy(&request[..count])
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    (address, task)
}
#[tokio::test]
#[ignore = "local TCP fixture; run explicitly outside a network-restricted sandbox"]
async fn falls_back_to_search_and_matches_the_right_song() {
    let (base, server) = server(vec![
        ("404 Not Found", "{}"),
        (
            "200 OK",
            r#"[
      {"trackName":"Another song","artistName":"Artist","syncedLyrics":"[00:00.00]Wrong song"},
      {"trackName":"Song","artistName":"Artist","duration":300,"syncedLyrics":"[00:00.00]Wrong version"},
      {"trackName":"Song","artistName":"Artist","duration":180,"syncedLyrics":"[00:00.00]Correct lyrics"}
    ]"#,
        ),
    ]);
    let lyrics = LrclibClient::new()
        .unwrap()
        .with_base(base)
        .fetch(&TrackQuery {
            title: "Song".into(),
            artist: "Artist".into(),
            duration_secs: Some(180),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(lyrics.lines[0].text, "Correct lyrics");
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("GET /api/get?"));
    assert!(requests[1].starts_with("GET /api/search?"));
}
#[tokio::test]
#[ignore = "local TCP fixture; run explicitly outside a network-restricted sandbox"]
async fn manual_title_only_omits_empty_artist_and_shows_plain_lyrics() {
    let (base, server) = server(vec![(
        "200 OK",
        r#"[{"trackName":"Song","plainLyrics":"First line\nSecond line"}]"#,
    )]);
    let lyrics = LrclibClient::new()
        .unwrap()
        .with_base(base)
        .fetch(&TrackQuery::new("Song", ""))
        .await
        .unwrap();
    assert!(!lyrics.synced);
    assert_eq!(lyrics.lines.len(), 2);
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("GET /api/search?"));
    assert!(!requests[0].contains("artist_name"));
}
