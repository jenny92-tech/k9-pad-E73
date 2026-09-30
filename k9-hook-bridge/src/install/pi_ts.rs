// INPUT:  无（编译期常量）
// OUTPUT: PI_EXTENSION_TS — 写入 ~/.pi/agent/extensions/k9pad.ts 的 Pi 扩展源码
// POS:    Pi 扩展模板（适配自蓝本 codeisland-pi.ts）：socket 路径 /tmp/k9pad-<uid>.sock
//         （K9PAD_SOCKET_PATH 可覆盖）、bridge 路径 ~/.k9pad/k9-hook-bridge；
//         以字符串字面量内嵌，install pi 时原样落盘

/// Pi 扩展源码（version: v1）。核心逻辑保留蓝本：
/// 事件映射、危险 bash 检测、ask 工具 24h 等待、危险命令 30s 超时、
/// pendingPermissionSessions 抑制。
pub const PI_EXTENSION_TS: &str = r#"// K9-Pad pi extension
// version: v1

/**
 * @fileoverview K9-Pad Integration Extension.
 *
 * Bridges the running pi session to the K9-Pad macOS approval-panel app
 * (k9-host-app) by forwarding lifecycle events over the Unix domain socket
 * the app listens on.
 *
 * Architecture:
 *   pi (this extension)  ──→  /tmp/k9pad-{uid}.sock  ──→  k9-host-app
 *
 * The extension is a socket CLIENT — no server is started. If k9-host-app is
 * not running the socket does not exist and all send calls fail silently.
 *
 * Event mapping:
 *   session_start          →  SessionStart
 *   session_shutdown       →  SessionEnd
 *   before_agent_start     →  UserPromptSubmit
 *   tool_call              →  PreToolUse  (or PermissionRequest for dangerous bash)
 *   tool_result            →  PostToolUse
 *   agent_end              →  Stop
 *   session_before_compact →  PreCompact
 *   session_compact        →  PostCompact
 *
 * Permission handling:
 *   Dangerous bash commands (`rm -rf`, `sudo`, `chmod 777`) are intercepted and
 *   sent as a blocking PermissionRequest via the k9-hook-bridge binary. The
 *   extension waits for K9-Pad's decision and returns allow/block accordingly.
 *   This replaces the built-in permission-gate.ts when K9-Pad is active.
 *
 * Installation:
 *   Drop this file in ~/.pi/agent/extensions/ — it is auto-discovered.
 *
 * Requirements:
 *   - k9-host-app running on the same machine
 */

import { execFile, execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { connect } from "node:net";
import { homedir } from "node:os";
import { getuid } from "node:process";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// ── Socket / bridge constants ─────────────────────────────────────────────────

/** Unix socket path k9-host-app listens on (user-scoped). */
const userId = getuid?.() ?? 0;
const SOCKET_PATH =
  process.env.K9PAD_SOCKET_PATH || `/tmp/k9pad-${userId}.sock`;

/**
 * Bridge binary path. Used for blocking permission requests because Node's
 * half-close (`sock.end()`) causes the server to close before the response
 * arrives on macOS; the bridge uses POSIX `shutdown(SHUT_WR)` which works.
 */
const BRIDGE_PATH = `${homedir()}/.k9pad/k9-hook-bridge`;

/** Environment variable keys forwarded to k9-host-app for terminal detection. */
const ENV_KEYS = [
  "TERM_PROGRAM",
  "ITERM_SESSION_ID",
  "TERM_SESSION_ID",
  "TMUX",
  "TMUX_PANE",
  "KITTY_WINDOW_ID",
  "CMUX_SURFACE_ID",
  "CMUX_WORKSPACE_ID",
  "ZELLIJ_PANE_ID",
  "ZELLIJ_SESSION_NAME",
  "WEZTERM_PANE",
  "__CFBundleIdentifier",
] as const;

// ── Dangerous bash patterns (mirrors permission-gate.ts) ──────────────────────

const DANGEROUS_PATTERNS: RegExp[] = [
  /\brm\s+(-rf?|--recursive)/i,
  /\bsudo\b/i,
  /\b(chmod|chown)\b.*777/i,
];

function isDangerous(command: string): boolean {
  return DANGEROUS_PATTERNS.some((p) => p.test(command));
}

// ── Environment / TTY helpers ─────────────────────────────────────────────────

/** Collects relevant terminal environment variables. */
function collectEnv(): Record<string, string> {
  const env: Record<string, string> = {};
  for (const key of ENV_KEYS) {
    if (process.env[key]) env[key] = process.env[key]!;
  }
  return env;
}

/**
 * Walks the process tree upward to find the controlling TTY.
 * Cached at startup — pi's TTY does not change during a session.
 */
function detectTty(): string | null {
  try {
    let pid = process.pid;
    for (let i = 0; i < 8; i++) {
      const out = execFileSync("ps", ["-o", "tty=,ppid=", "-p", String(pid)], {
        timeout: 1000,
      })
        .toString()
        .trim();
      const [tty, ppidStr] = out.split(/\s+/);
      if (tty && tty !== "??" && tty !== "?") {
        return tty.startsWith("/dev/") ? tty : `/dev/${tty}`;
      }
      const ppid = parseInt(ppidStr ?? "0", 10);
      if (!ppid || ppid <= 1) break;
      pid = ppid;
    }
  } catch {}
  return null;
}

// ── Socket communication ──────────────────────────────────────────────────────

/**
 * Sends a JSON payload to the K9-Pad socket (fire-and-forget).
 * Returns `false` silently when k9-host-app is not running.
 *
 * @param payload - Event object to serialise and send.
 * @returns `true` on successful delivery, `false` otherwise.
 */
function sendToSocket(payload: object): Promise<boolean> {
  return new Promise((resolve) => {
    try {
      const sock = connect({ path: SOCKET_PATH }, () => {
        sock.write(JSON.stringify(payload));
        sock.end();
        resolve(true);
      });
      sock.on("error", () => resolve(false));
      sock.setTimeout(3_000, () => {
        sock.destroy();
        resolve(false);
      });
    } catch {
      resolve(false);
    }
  });
}

/**
 * Sends a JSON payload via the bridge binary and waits for K9-Pad's response.
 * Used exclusively for blocking permission/question requests.
 *
 * @param payload    - Blocking request object.
 * @param timeoutMs  - Maximum wait time in milliseconds (default 30 s).
 * @returns Parsed response JSON, or `null` on error / timeout.
 */
function sendAndWaitResponse(
  payload: object,
  timeoutMs = 30_000,
): Promise<Record<string, unknown> | null> {
  return new Promise((resolve) => {
    if (!existsSync(BRIDGE_PATH)) {
      resolve(null);
      return;
    }
    try {
      const child = execFile(
        BRIDGE_PATH,
        [],
        { timeout: timeoutMs, maxBuffer: 1_048_576 },
        (error, stdout) => {
          if (error) {
            resolve(null);
            return;
          }
          try {
            resolve(JSON.parse(stdout));
          } catch {
            resolve(null);
          }
        },
      );
      child.stdin!.write(JSON.stringify(payload));
      child.stdin!.end();
    } catch {
      resolve(null);
    }
  });
}

// ── Event builders ────────────────────────────────────────────────────────────

/**
 * Builds the base fields required on every K9-Pad event payload.
 *
 * @param sessionId - Pi session UUID (prefixed with `"pi-"`).
 * @param cwd       - Current working directory.
 * @param extra     - Event-specific fields merged into the base.
 * @returns Complete event payload ready for `sendToSocket`.
 */
function base(
  sessionId: string,
  cwd: string,
  extra: Record<string, unknown>,
  tty: string | null,
): Record<string, unknown> {
  return {
    session_id: `pi-${sessionId}`,
    _source: "pi",
    _ppid: process.pid,
    _env: collectEnv(),
    _tty: tty,
    _server_port: 0,
    cwd,
    ...extra,
  };
}

/** Capitalises the first character of a tool name for display. */
function displayToolName(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** Extracts plain text from the last assistant message in an event.messages array. */
function extractLastAssistantText(
  messages: readonly unknown[],
): string {
  const assistants = messages.filter(
    (m): m is AssistantMessage =>
      !!m &&
      typeof m === "object" &&
      (m as { role?: string }).role === "assistant",
  );
  const last = assistants.at(-1);
  if (!last) return "";
  const content = last.content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((c): c is { type: "text"; text: string } => c?.type === "text")
    .map((c) => c.text)
    .join("")
    .trim();
}

// ── Extension ─────────────────────────────────────────────────────────────────

export default function k9padExtension(pi: ExtensionAPI) {
  /** TTY path detected once at startup. */
  const tty = detectTty();

  /**
   * Session IDs for which a blocking PermissionRequest is currently in flight.
   * Non-lifecycle events for these sessions are suppressed to prevent K9-Pad's
   * "answered externally" heuristic from auto-denying while the card is visible.
   */
  const pendingPermissionSessions = new Set<string>();
  /** Sessions for which K9-Pad has already received SessionStart. */
  const startedSessions = new Set<string>();

  async function ensureSessionStarted(sessionId: string, cwd: string): Promise<void> {
    const sid = `pi-${sessionId}`;
    if (startedSessions.has(sid)) return;

    const sessionName = pi.getSessionName();
    await sendToSocket(
      base(sessionId, cwd, {
        hook_event_name: "SessionStart",
        ...(sessionName ? { session_title: sessionName } : {}),
      }, tty),
    );
    startedSessions.add(sid);
  }


  /**
   * Forwards an `ask` tool call to K9-Pad as an AskUserQuestion and waits
   * for the user's on-island answer.
   *
   * @returns A block result carrying the answers when the user answered in
   *          K9-Pad, or `null` when the question should fall through to
   *          OMP's own TUI dialog (skipped, denied, or K9-Pad not running).
   */
  async function forwardAskToK9Pad(
    event: { input: Record<string, unknown>; toolCallId: string },
    ctx: { cwd: string },
    sessionId: string,
    sid: string,
    tty: string | null,
  ): Promise<{ block: true; reason: string } | null> {
    const rawQuestions = Array.isArray(event.input.questions)
      ? (event.input.questions as Array<Record<string, unknown>>)
      : [];
    if (rawQuestions.length === 0) return null;

    // Map OMP's ask schema → Claude-style AskUserQuestion input.
    const questions = rawQuestions.map((q) => {
      const options = Array.isArray(q.options)
        ? (q.options as Array<Record<string, unknown>>)
            .map((o) => ({
              label: typeof o.label === "string" ? o.label : "",
              ...(typeof o.description === "string"
                ? { description: o.description }
                : {}),
            }))
            .filter((o) => o.label.length > 0)
        : [];
      return {
        question: typeof q.question === "string" ? q.question : "Question",
        ...(typeof q.id === "string" && q.id ? { header: q.id } : {}),
        multiSelect: q.multi === true,
        options,
      };
    });

    // K9-Pad keys answers by question text, deduping repeats with `_2`,
    // `_3`… suffixes — reproduce that here so we can translate back to ids.
    const usedKeys = new Set<string>();
    const answerKeys = questions.map(({ question }) => {
      let key = question;
      if (usedKeys.has(key)) {
        let suffix = 2;
        while (usedKeys.has(`${question}_${suffix}`)) suffix += 1;
        key = `${question}_${suffix}`;
      }
      usedKeys.add(key);
      return key;
    });

    pendingPermissionSessions.add(sid);
    let response: Record<string, unknown> | null = null;
    try {
      response = await sendAndWaitResponse(
        base(sessionId, ctx.cwd, {
          hook_event_name: "PermissionRequest",
          tool_name: "AskUserQuestion",
          tool_input: { questions },
          _pi_tool_call_id: event.toolCallId,
        }, tty),
        86_400_000, // waiting on a human — same 24h budget as PermissionRequest hooks
      );
    } finally {
      pendingPermissionSessions.delete(sid);
    }

    const decision = (
      response?.hookSpecificOutput as Record<string, unknown> | undefined
    )?.decision as Record<string, unknown> | undefined;
    if (decision?.behavior !== "allow") return null;

    const updatedInput = decision.updatedInput as
      | Record<string, unknown>
      | undefined;
    const answers = (updatedInput?.answers ?? {}) as Record<string, unknown>;

    const lines = rawQuestions.map((q, i) => {
      const id = typeof q.id === "string" && q.id ? q.id : `q${i + 1}`;
      const value = answers[answerKeys[i]];
      const text = Array.isArray(value)
        ? value.map(String).join(", ")
        : typeof value === "string"
          ? value
          : "";
      return `${id}: ${text || "(no answer)"}`;
    });

    return {
      block: true,
      reason:
        "The user already answered these questions through the K9-Pad desktop app. " +
        "Their answers:\n" +
        lines.join("\n") +
        "\nDo not ask again — proceed using these answers.",
    };
  }

  // ── Session lifecycle ──────────────────────────────────────────────────────

  pi.on("session_start", async (_event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    await ensureSessionStarted(sessionId, ctx.cwd);
  });

  pi.on("session_shutdown", async (_event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    await sendToSocket(
      base(sessionId, ctx.cwd, { hook_event_name: "SessionEnd" }, tty),
    );
    startedSessions.delete(`pi-${sessionId}`);
  });

  // ── Agent lifecycle ────────────────────────────────────────────────────────

  pi.on("before_agent_start", async (event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    const sid = `pi-${sessionId}`;
    await ensureSessionStarted(sessionId, ctx.cwd);

    if (pendingPermissionSessions.has(sid)) return;

    const prompt = event.prompt ?? "";
    await sendToSocket(
      base(sessionId, ctx.cwd, {
        hook_event_name: "UserPromptSubmit",
        prompt,
      }, tty),
    );
  });

  pi.on("agent_end", async (event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    const sid = `pi-${sessionId}`;
    await ensureSessionStarted(sessionId, ctx.cwd);

    if (pendingPermissionSessions.has(sid)) return;

    const lastAssistantMessage = extractLastAssistantText(event.messages);
    const sessionName = pi.getSessionName();

    await sendToSocket(
      base(sessionId, ctx.cwd, {
        hook_event_name: "Stop",
        last_assistant_message: lastAssistantMessage || undefined,
        ...(sessionName ? { session_title: sessionName } : {}),
      }, tty),
    );
  });

  // ── Tool calls ─────────────────────────────────────────────────────────────

  pi.on("tool_call", async (event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    const sid = `pi-${sessionId}`;
    await ensureSessionStarted(sessionId, ctx.cwd);
    const toolName = displayToolName(event.toolName);

    // Build a tool_input object appropriate for the tool type.
    const toolInput: Record<string, unknown> = { ...event.input };
    if (event.toolName === "bash") {
      const command = event.input.command as string | undefined;
      if (command) toolInput.patterns = [command];
    }
    if (event.toolName === "edit" || event.toolName === "write") {
      const path = event.input.path as string | undefined;
      if (path) toolInput.file_path = path;
    }

    // `ask` tool → mirror the question into K9-Pad's question UI.
    // tool_call fires BEFORE the TUI dialog opens and OMP awaits this handler,
    // so we can hold the tool, let the user answer on the island (or watch/
    // phone), and feed the answers back by blocking the tool with a result
    // message. Skip/deny or an unreachable K9-Pad falls through to OMP's
    // own TUI dialog — graceful degradation, never a lost question.
    if (event.toolName === "ask") {
      const answered = await forwardAskToK9Pad(event, ctx, sessionId, sid, tty);
      if (answered) return answered;
      return undefined;
    }

    // Dangerous bash → send blocking PermissionRequest via bridge.
    if (
      event.toolName === "bash" &&
      typeof event.input.command === "string" &&
      isDangerous(event.input.command)
    ) {
      pendingPermissionSessions.add(sid);

      const payload = base(sessionId, ctx.cwd, {
        hook_event_name: "PermissionRequest",
        tool_name: toolName,
        tool_input: toolInput,
        _pi_tool_call_id: event.toolCallId,
      }, tty);

      let response: Record<string, unknown> | null = null;
      try {
        response = await sendAndWaitResponse(payload);
      } finally {
        pendingPermissionSessions.delete(sid);
      }

      const behavior = (
        response?.hookSpecificOutput as Record<string, unknown> | undefined
      )?.decision as Record<string, unknown> | undefined;

      if (behavior?.behavior === "deny") {
        return { block: true, reason: "Blocked by K9-Pad" };
      }

      // Approved — fall through to normal PreToolUse event below.
    }

    // Non-blocking PreToolUse for all other tool calls.
    if (!pendingPermissionSessions.has(sid)) {
      await sendToSocket(
        base(sessionId, ctx.cwd, {
          hook_event_name: "PreToolUse",
          tool_name: toolName,
          tool_input: toolInput,
        }, tty),
      );
    }

    return undefined;
  });

  pi.on("tool_result", async (_event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    const sid = `pi-${sessionId}`;
    await ensureSessionStarted(sessionId, ctx.cwd);

    if (pendingPermissionSessions.has(sid)) return;

    await sendToSocket(
      base(sessionId, ctx.cwd, { hook_event_name: "PostToolUse" }, tty),
    );
  });

  // ── Compaction ─────────────────────────────────────────────────────────────

  pi.on("session_before_compact", async (_event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    await ensureSessionStarted(sessionId, ctx.cwd);
    await sendToSocket(
      base(sessionId, ctx.cwd, { hook_event_name: "PreCompact" }, tty),
    );
  });

  pi.on("session_compact", async (_event, ctx) => {
    const sessionId = ctx.sessionManager.getSessionId();
    await ensureSessionStarted(sessionId, ctx.cwd);
    await sendToSocket(
      base(sessionId, ctx.cwd, { hook_event_name: "PostCompact" }, tty),
    );
  });
}
"#;
