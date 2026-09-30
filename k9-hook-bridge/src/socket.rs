// INPUT:  富化后的 JSON 字节 + socket 路径 + 阻塞性标记
// OUTPUT: Unix socket 转发（写 JSON → shutdown(Write)）；阻塞事件返回 server 响应字节
// POS:    与 k9-host-app SocketServer 的传输层；分帧协议 = 单 JSON + 半关闭，任何失败返回 None

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// 阻塞事件连接超时 3s，非阻塞 1s（对照蓝本 connectSocket 的 timeoutMs）。
const CONNECT_TIMEOUT_BLOCKING: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT_NORMAL: Duration = Duration::from_secs(1);

/// 默认 socket 路径 `/tmp/k9pad-<uid>.sock`，`K9PAD_SOCKET_PATH` 环境变量覆盖。
pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("K9PAD_SOCKET_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    // SAFETY: getuid() 无参数、无内存访问，POSIX 保证不会失败。
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/k9pad-{uid}.sock"))
}

/// 把 payload 写入 socket 并半关闭。
/// 返回：阻塞事件为 server 响应字节（读到 EOF）；非阻塞事件或任何失败为 None。
pub async fn forward(path: &std::path::Path, payload: &[u8], blocking: bool) -> Option<Vec<u8>> {
    let connect_timeout = if blocking {
        CONNECT_TIMEOUT_BLOCKING
    } else {
        CONNECT_TIMEOUT_NORMAL
    };
    let stream = tokio::time::timeout(connect_timeout, UnixStream::connect(path))
        .await
        .ok()?
        .ok()?;
    let (mut reader, mut writer) = stream.into_split();

    writer.write_all(payload).await.ok()?;
    // 半关闭表结束：server 读到 EOF 才开始处理
    writer.shutdown().await.ok()?;

    if !blocking {
        return None;
    }

    // 阻塞事件：读响应到 EOF，无超时限制（App 侧权限请求不设超时，等用户）
    let mut response = Vec::new();
    reader.read_to_end(&mut response).await.ok()?;
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_env_override() {
        std::env::set_var("K9PAD_SOCKET_PATH", "/tmp/k9pad-test-override.sock");
        assert_eq!(
            socket_path(),
            PathBuf::from("/tmp/k9pad-test-override.sock")
        );
        std::env::remove_var("K9PAD_SOCKET_PATH");
    }

    #[test]
    fn socket_path_default_contains_uid() {
        std::env::remove_var("K9PAD_SOCKET_PATH");
        let path = socket_path().to_string_lossy().to_string();
        assert!(path.starts_with("/tmp/k9pad-"), "path: {path}");
        assert!(path.ends_with(".sock"), "path: {path}");
    }
}
