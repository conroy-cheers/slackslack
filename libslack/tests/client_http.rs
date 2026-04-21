use libslack::{Credentials, SlackClient};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

fn spawn_json_server(
    expected_requests: usize,
    handler: impl Fn(String) -> String + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_clone = Arc::clone(&requests);
    let handler = Arc::new(handler);
    let handle = std::thread::spawn(move || {
        for _ in 0..expected_requests {
            let stream = listener.accept();
            let mut stream = match stream {
                Ok((stream, _)) => stream,
                Err(_) => break,
            };
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            requests_clone.lock().unwrap().push(request.clone());
            let body = handler(request);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });

    (format!("http://{}", addr), requests, handle)
}

#[tokio::test]
async fn auth_test_uses_configured_base_url_and_posts_token() {
    let (base_url, requests, handle) = spawn_json_server(1, |request| {
        assert!(request.starts_with("POST /auth.test HTTP/1.1"));
        assert!(request.contains("token=xoxc-test-token"));
        r#"{"ok":true,"user":"me","user_id":"U1","team":"Team","team_id":"T1"}"#.to_string()
    });

    let creds = Credentials {
        token: "xoxc-test-token".into(),
        cookie: "xoxd-cookie".into(),
    };
    let client = SlackClient::new_with_base_url(&creds, &base_url).unwrap();
    let auth = client.auth_test().await.unwrap();

    assert_eq!(auth.user, "me");
    assert_eq!(auth.team_id, "T1");
    assert_eq!(requests.lock().unwrap().len(), 1);

    drop(client);
    let _ = handle.join();
}

#[tokio::test]
async fn conversations_list_all_follows_next_cursor() {
    let seen_cursors = Arc::new(Mutex::new(Vec::new()));
    let seen_cursors_clone = Arc::clone(&seen_cursors);
    let (base_url, _requests, handle) = spawn_json_server(2, move |request| {
        assert!(request.starts_with("POST /conversations.list HTTP/1.1"));
        let body = request.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
        let cursor = body
            .split('&')
            .find_map(|part| part.strip_prefix("cursor="))
            .unwrap_or_default()
            .to_string();
        seen_cursors_clone.lock().unwrap().push(cursor.clone());

        if cursor.is_empty() {
            r#"{"ok":true,"channels":[{"id":"C1","name":"general"}],"response_metadata":{"next_cursor":"page-2"}}"#.to_string()
        } else {
            r#"{"ok":true,"channels":[{"id":"C2","name":"random"}],"response_metadata":{"next_cursor":""}}"#.to_string()
        }
    });

    let creds = Credentials {
        token: "xoxc-test-token".into(),
        cookie: "xoxd-cookie".into(),
    };
    let client = SlackClient::new_with_base_url(&creds, &base_url).unwrap();
    let channels = client.conversations_list_all().await.unwrap();

    let ids: Vec<_> = channels.into_iter().map(|c| c.id).collect();
    assert_eq!(ids, vec!["C1", "C2"]);
    assert_eq!(seen_cursors.lock().unwrap().as_slice(), &["", "page-2"]);

    drop(client);
    let _ = handle.join();
}
