#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
}

fn setup_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let src = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/locate"
    ));
    let dst = dir.path().join("repo");
    copy_dir_all(src, &dst).unwrap();
    Fixture {
        _dir: dir,
        root: dst,
    }
}

fn send(w: &mut impl Write, msg: &serde_json::Value) {
    let mut payload = serde_json::to_vec(msg).unwrap();
    payload.push(b'\n');
    w.write_all(&payload).unwrap();
    w.flush().unwrap();
}

fn recv(r: &mut BufReader<impl std::io::Read>) -> serde_json::Value {
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn spawn_serve(root: &std::path::Path) -> (Child, BufReader<ChildStdout>, ChildStdin) {
    let bin = env!("CARGO_BIN_EXE_argyph");
    let mut child = Command::new(bin)
        .arg("serve")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());
    let writer = child.stdin.take().unwrap();
    (child, reader, writer)
}

fn handshake(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "smart-smoke-test", "version": "1.0"}
        }
    });
    send(stdin, &init_req);
    recv(stdout);
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    send(stdin, &initialized);
}

fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": args
        }
    });
    send(stdin, &req);
    recv(stdout)
}

fn parse_tool_result(v: &serde_json::Value) -> serde_json::Value {
    let content = &v["result"]["content"];
    if let Some(arr) = content.as_array() {
        if let Some(text) = arr[0]["text"].as_str() {
            if let Ok(body) = serde_json::from_str::<serde_json::Value>(text) {
                return body;
            }
        }
    }
    serde_json::Value::Null
}

#[test]
fn locate_smart_disabled_by_default() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    handshake(&mut stdin, &mut stdout);

    let resp = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "locate_smart",
        serde_json::json!({
            "query": "anything"
        }),
    );
    let body = parse_tool_result(&resp);

    let error_obj = body["error"].as_object();
    assert!(error_obj.is_some(), "Expected error object, got: {body}");
    let code = error_obj.and_then(|e| e["code"].as_str()).unwrap_or("");
    assert!(
        code.contains("LOCATE_SMART_DISABLED") || code.contains("DISABLED"),
        "Expected LOCATE_SMART_DISABLED code, got: {code}"
    );

    child.kill().ok();
}
