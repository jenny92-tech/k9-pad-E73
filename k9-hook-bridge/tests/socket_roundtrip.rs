// INPUT:  编译产物 k9-hook-bridge 二进制 + 临时 UnixListener 模拟 server
// OUTPUT: 断言阻塞事件 round-trip 字节一致、非阻塞/无 server 时静默 exit 0
// POS:    socket 分帧协议（单 JSON + 半关闭）的端到端验证，覆盖 src 单测之外的进程行为

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 每个用例独立的临时 socket 路径，测试结束删除。
fn temp_sock(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("k9pad-test-{}-{name}.sock", std::process::id()))
}

/// 启动模拟 server：接受一条连接，读到 EOF（半关闭分帧），回 response 后关闭。
/// 返回 (server 线程 handle, 收到字节的回传通道)。
fn spawn_mock_server(
    path: &std::path::Path,
    response: &'static [u8],
) -> (
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<Vec<u8>>,
) {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).expect("bind temp socket");
    listener.set_nonblocking(true).expect("set nonblocking");
    let (tx, rx) = std::sync::mpsc::channel();
    let path = path.to_path_buf();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "server 等不到 bridge 连接");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("accept 失败: {e}"),
            }
        };
        // macOS 上 accept 出来的 socket 继承 listener 的 O_NONBLOCK，且该状态下
        // SO_RCVTIMEO 会报 EINVAL——保持非阻塞，手动轮询读到 EOF（带 deadline）。
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => received.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "server 读不到 EOF");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("server 读失败: {e}"),
            }
        }
        // 非阻塞事件下 bridge 发完即退出，回写可能 BrokenPipe——尽力而为即可
        let _ = stream.write_all(response);
        tx.send(received).expect("send received bytes");
        let _ = std::fs::remove_file(&path);
    });
    (handle, rx)
}

/// 跑一遍 bridge：喂 stdin，收 (exit_status, stdout)。
fn run_bridge(sock: &std::path::Path, args: &[&str], stdin_json: &str) -> (std::process::ExitStatus, Vec<u8>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_k9-hook-bridge"))
        .args(args)
        .env("K9PAD_SOCKET_PATH", sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn bridge");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(stdin_json.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait bridge");
    (output.status, output.stdout)
}

#[test]
fn blocking_permission_request_round_trip() {
    let sock = temp_sock("blocking");
    const RESPONSE: &[u8] = br#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;
    let (server, received_rx) = spawn_mock_server(&sock, RESPONSE);

    let (status, stdout) = run_bridge(
        &sock,
        &["--source", "claude"],
        r#"{"session_id":"s1","hook_event_name":"PermissionRequest","tool_name":"Bash","cwd":"/tmp"}"#,
    );

    assert!(status.success());
    assert_eq!(stdout, RESPONSE, "stdout 必须原样回传 server 响应");

    let received = received_rx.recv().expect("server 收到事件");
    let json: serde_json::Value = serde_json::from_slice(&received).expect("server 收到合法 JSON");
    assert_eq!(json["session_id"], "s1");
    assert_eq!(json["_source"], "claude");
    assert!(json["_ppid"].is_number(), "_ppid 已注入");
    assert_eq!(json["cwd"], "/tmp", "已有 cwd 不被覆盖");
    assert_eq!(json["tool_name"], "Bash", "原字段保留");
    server.join().expect("server 线程结束");
}

#[test]
fn session_id_fallback_when_missing() {
    let sock = temp_sock("fallback");
    let (server, received_rx) = spawn_mock_server(&sock, b"{}");

    let (status, _) = run_bridge(
        &sock,
        &["--source", "codex"],
        r#"{"hook_event_name":"SessionStart"}"#,
    );
    assert!(status.success());

    let received = received_rx.recv().expect("server 收到事件");
    let json: serde_json::Value = serde_json::from_slice(&received).unwrap();
    let session_id = json["session_id"].as_str().expect("兜底 session_id");
    assert!(
        session_id.starts_with("codex-ppid-"),
        "session_id 兜底格式: {session_id}"
    );
    assert!(json["cwd"].is_string(), "cwd 缺失时已兜底");
    server.join().expect("server 线程结束");
}

#[test]
fn non_blocking_event_silent_success() {
    let sock = temp_sock("nonblocking");
    let (server, received_rx) = spawn_mock_server(&sock, b"{}");

    let (status, stdout) = run_bridge(
        &sock,
        &["--source", "claude"],
        r#"{"session_id":"s1","hook_event_name":"SessionStart","cwd":"/tmp"}"#,
    );

    assert!(status.success());
    assert!(stdout.is_empty(), "非阻塞事件不写 stdout");
    // server 仍收到了事件（只是 bridge 不等响应）
    received_rx.recv().expect("server 收到事件");
    server.join().expect("server 线程结束");
}

#[test]
fn silent_exit_when_no_server() {
    let sock = temp_sock("absent");
    let _ = std::fs::remove_file(&sock);

    // 阻塞事件也必须在无 server 时静默成功退出（CLI 回退自身审批 UI）
    let (status, stdout) = run_bridge(
        &sock,
        &["--source", "claude"],
        r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"/tmp"}"#,
    );
    assert!(status.success());
    assert!(stdout.is_empty());

    let (status, stdout) = run_bridge(
        &sock,
        &["--source", "claude"],
        r#"{"session_id":"s1","hook_event_name":"SessionStart","cwd":"/tmp"}"#,
    );
    assert!(status.success());
    assert!(stdout.is_empty());
}

#[test]
fn silent_exit_on_garbage_stdin() {
    let sock = temp_sock("garbage");
    let (status, stdout) = run_bridge(&sock, &["--source", "claude"], "not json at all");
    assert!(status.success());
    assert!(stdout.is_empty());
}

#[test]
fn skip_env_short_circuits() {
    let sock = temp_sock("skip");
    let mut child = Command::new(env!("CARGO_BIN_EXE_k9-hook-bridge"))
        .args(["--source", "claude"])
        .env("K9PAD_SOCKET_PATH", &sock)
        .env("K9PAD_SKIP", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn bridge");
    // 不喂 stdin、不关管道也应立即退出
    let status = child.wait().expect("wait bridge");
    assert!(status.success());
}
