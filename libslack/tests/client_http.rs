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
        let body = request
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or_default()
            .to_string();
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

#[tokio::test]
async fn foundational_conversation_methods_hit_expected_routes() {
    let (base_url, requests, handle) = spawn_json_server(3, |request| {
        if request.starts_with("POST /conversations.info HTTP/1.1") {
            assert!(request.contains("channel=C123"));
            r#"{"ok":true,"channel":{"id":"C123","name":"general","is_channel":true}}"#.to_string()
        } else if request.starts_with("POST /conversations.members HTTP/1.1") {
            assert!(request.contains("channel=C123"));
            assert!(request.contains("limit=200"));
            r#"{"ok":true,"members":["U1","U2"],"response_metadata":{"next_cursor":""}}"#
                .to_string()
        } else if request.starts_with("POST /conversations.open HTTP/1.1") {
            assert!(request.contains("users=U1%2CU2") || request.contains("users=U1,U2"));
            r#"{"ok":true,"channel":{"id":"D1","is_im":true},"no_op":false,"already_open":false}"#
                .to_string()
        } else {
            panic!("unexpected request: {request}");
        }
    });

    let creds = Credentials {
        token: "xoxc-test-token".into(),
        cookie: "xoxd-cookie".into(),
    };
    let client = SlackClient::new_with_base_url(&creds, &base_url).unwrap();

    let info = client.conversations_info("C123").await.unwrap();
    let members = client
        .conversations_members("C123", None, 200)
        .await
        .unwrap();
    let open = client.conversations_open("U1,U2").await.unwrap();

    assert_eq!(info.channel.unwrap().id, "C123");
    assert_eq!(members.members, vec!["U1", "U2"]);
    assert_eq!(open.channel.unwrap().id, "D1");
    assert_eq!(requests.lock().unwrap().len(), 3);

    drop(client);
    let _ = handle.join();
}

#[tokio::test]
async fn profile_file_and_pin_methods_decode_expected_payloads() {
    let (base_url, _requests, handle) = spawn_json_server(5, |request| {
        if request.starts_with("POST /users.profile.get HTTP/1.1") {
            r#"{"ok":true,"profile":{"display_name":"alice","status_text":"In Slack","status_emoji":":speech_balloon:"}}"#.to_string()
        } else if request.starts_with("POST /team.profile.get HTTP/1.1") {
            r#"{"ok":true,"profile":{"fields":[{"id":"X1","label":"Team"}]}}"#.to_string()
        } else if request.starts_with("POST /files.info HTTP/1.1") {
            r#"{"ok":true,"file":{"id":"F1","name":"spec.pdf","title":"Spec"},"response_metadata":{"next_cursor":""}}"#.to_string()
        } else if request.starts_with("POST /files.list HTTP/1.1") {
            r#"{"ok":true,"files":[{"id":"F2","name":"image.png","mimetype":"image/png"}],"paging":{"count":1,"total":1,"page":1,"pages":1}}"#.to_string()
        } else if request.starts_with("POST /pins.list HTTP/1.1") {
            r#"{"ok":true,"items":[{"type":"message","channel":"C1","created_by":"U1","message":{"ts":"1.0","text":"pinned"}}]}"#.to_string()
        } else {
            panic!("unexpected request: {request}");
        }
    });

    let creds = Credentials {
        token: "xoxc-test-token".into(),
        cookie: "xoxd-cookie".into(),
    };
    let client = SlackClient::new_with_base_url(&creds, &base_url).unwrap();

    let profile = client.users_profile_get(Some("U1"), false).await.unwrap();
    let team_profile = client.team_profile_get().await.unwrap();
    let file_info = client.files_info("F1", None, Some(50)).await.unwrap();
    let file_list = client.files_list(None, Some(20)).await.unwrap();
    let pins = client.pins_list("C1").await.unwrap();

    assert_eq!(
        profile.profile.unwrap().display_name.as_deref(),
        Some("alice")
    );
    assert_eq!(team_profile.profile.unwrap().fields[0].id, "X1");
    assert_eq!(file_info.file.unwrap().id, "F1");
    assert_eq!(file_list.files[0].id, "F2");
    assert_eq!(pins.items[0].channel.as_deref(), Some("C1"));

    drop(client);
    let _ = handle.join();
}

#[tokio::test]
async fn users_conversations_and_search_files_decode_expected_payloads() {
    let (base_url, _requests, handle) = spawn_json_server(2, |request| {
        if request.starts_with("POST /users.conversations HTTP/1.1") {
            assert!(
                request.contains("types=public_channel%2Cprivate_channel")
                    || request.contains("types=public_channel,private_channel")
            );
            r#"{"ok":true,"channels":[{"id":"C3","name":"proj","is_channel":true}],"response_metadata":{"next_cursor":""}}"#.to_string()
        } else if request.starts_with("POST /search.files HTTP/1.1") {
            assert!(request.contains("query=report"));
            r#"{"ok":true,"query":"report","files":{"matches":[{"id":"F9","name":"report.pdf","title":"Quarterly report"}],"paging":{"count":1,"total":1,"page":1,"pages":1},"total":1}}"#.to_string()
        } else {
            panic!("unexpected request: {request}");
        }
    });

    let creds = Credentials {
        token: "xoxc-test-token".into(),
        cookie: "xoxd-cookie".into(),
    };
    let client = SlackClient::new_with_base_url(&creds, &base_url).unwrap();

    let convs = client
        .users_conversations("public_channel,private_channel", None, 200)
        .await
        .unwrap();
    let files = client.search_files("report", 1, 20).await.unwrap();

    assert_eq!(convs.channels.len(), 1);
    assert_eq!(convs.channels[0].id, "C3");
    assert_eq!(files.query.as_deref(), Some("report"));
    assert_eq!(files.files.matches[0].id, "F9");

    drop(client);
    let _ = handle.join();
}
