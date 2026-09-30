// INPUT:  CLI hook 事件（stdin JSON）+ argv(--source / install 子命令) + 环境变量
// OUTPUT: 富化后经 Unix socket 转发给 k9-host-app；阻塞事件把决策 JSON 原样写 stdout
// POS:    Claude/Codex/Kimi/Pi/OMP 与 k9-host-app SocketServer 之间的桥接进程；
//         核心安全语义：任何失败都静默 exit 0 且无输出，CLI 回退自身审批 UI

mod bridge;
mod install;
mod socket;

use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

/// stdin 读取上限 5s：防调用方忘记关闭管道时永久阻塞（对照蓝本 alarm(5)）。
const STDIN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Parser)]
#[command(
    name = "k9-hook-bridge",
    about = "K9-Pad AI 审批面板 hook 桥接器：转发 CLI hook 事件到 k9-host-app"
)]
struct Cli {
    /// 事件来源标记，注入为 _source 字段
    #[arg(long, value_enum)]
    source: Option<Source>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Source {
    Claude,
    Codex,
    Kimi,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Codex => "codex",
            Source::Kimi => "kimi",
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// 安装 hook 到 CLI 配置文件
    Install {
        #[command(subcommand)]
        target: InstallTarget,
    },
}

#[derive(Subcommand)]
enum InstallTarget {
    /// 写入 Claude Code settings.json 的 hooks 节
    Claude {
        /// Claude 配置目录（优先级高于 $CLAUDE_CONFIG_DIR，默认 ~/.claude）
        #[arg(long)]
        config_dir: Option<PathBuf>,
    },
    /// 写入 Codex hooks.json（nested 格式）+ config.toml [features] hooks = true
    Codex,
    /// 写入 Kimi Code config.toml 的 [[hooks]] 块
    Kimi,
    /// 写入 Pi 扩展 ~/.pi/agent/extensions/k9pad.ts
    Pi,
    /// 写入 OMP 扩展 ~/.omp/agent/extensions/k9pad.ts
    Omp,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    std::process::exit(run().await);
}

async fn run() -> i32 {
    let cli = Cli::parse();

    // K9PAD_SKIP 置位：无条件直通，CLI 走自身逻辑
    if std::env::var_os("K9PAD_SKIP").is_some() {
        return 0;
    }

    match cli.command {
        Some(Commands::Install { target }) => {
            let result = match target {
                InstallTarget::Claude { config_dir } => install::install_claude(config_dir),
                InstallTarget::Codex => install::codex::install_codex(),
                InstallTarget::Kimi => install::kimi::install_kimi(),
                InstallTarget::Pi => install::pi_ext::install_pi(),
                InstallTarget::Omp => install::pi_ext::install_omp(),
            };
            match result {
                Ok(msg) => {
                    println!("{msg}");
                    0
                }
                Err(e) => {
                    eprintln!("install 失败: {e}");
                    1
                }
            }
        }
        None => {
            run_bridge(cli.source).await;
            0
        }
    }
}

/// 桥接模式：任何一步失败都静默返回（exit 0 + 无输出）。
async fn run_bridge(source: Option<Source>) {
    let Some(input) = read_stdin().await else {
        return;
    };
    if input.is_empty() {
        return;
    }

    let Ok(serde_json::Value::Object(mut json)) = serde_json::from_slice(&input) else {
        return;
    };

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    bridge::enrich_event(
        &mut json,
        source.map(Source::as_str),
        std::os::unix::process::parent_id(),
        &cwd,
    );

    // 必须有非空 session_id，否则丢弃（连兜底都无从谈起时）
    if !bridge::has_valid_session_id(&json) {
        return;
    }

    let blocking = bridge::is_blocking(&json);
    let Ok(payload) = serde_json::to_vec(&json) else {
        return;
    };

    let path = socket::socket_path();
    let Some(response) = socket::forward(&path, &payload, blocking).await else {
        return;
    };

    // 阻塞事件：server 的决策 JSON 原样写 stdout，交给 CLI 消费
    if !response.is_empty() {
        let _ = std::io::stdout().lock().write_all(&response);
    }
}

/// 5s 超时读完全部 stdin；超时/IO 错误返回 None。
/// 用 spawn_blocking + std io 读取（tokio stdin 需额外 io-std feature，这里不引入）；
/// 超时后阻塞线程随进程退出自然回收。
async fn read_stdin() -> Option<Vec<u8>> {
    let read = tokio::task::spawn_blocking(|| {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf).ok()?;
        Some(buf)
    });
    tokio::time::timeout(STDIN_TIMEOUT, read).await.ok()?.ok()?
}
