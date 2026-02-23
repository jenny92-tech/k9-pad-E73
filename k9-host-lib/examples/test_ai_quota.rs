// INPUT:  k9-host-lib::ai_quota (credential readers + quota fetchers)
// OUTPUT: CLI smoke test — reads credentials and prints Claude/Codex quota to stdout
// POS:    Developer example — validates ai_quota module works end-to-end

//! Quick smoke test: read credentials and fetch AI quota.
use k9_host_lib::ai_quota;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== AI Quota Test ===\n");

    // --- Claude Code ---
    print!("[Claude] Reading credentials... ");
    match ai_quota::read_claude_credentials() {
        Ok(cred) => {
            println!("OK");
            println!(
                "  token:   {}...{}",
                &cred.access_token[..20],
                &cred.access_token[cred.access_token.len() - 6..]
            );
            println!("  expires: {} (valid={})", cred.expires_at, cred.is_valid());

            if cred.is_valid() {
                print!("[Claude] Fetching quota... ");
                match ai_quota::fetch_claude_quota(&cred).await {
                    Ok(info) => {
                        println!("OK");
                        println!("  utilization: {:.1}%", info.utilization_pct);
                        println!("  progress:    {}/100", info.as_progress());
                        println!("  display:     {}", info.as_display_text());
                    }
                    Err(e) => println!("FAILED: {e}"),
                }
            } else {
                println!("[Claude] Token expired, skipping quota fetch");
            }
        }
        Err(e) => println!("FAILED: {e}"),
    }

    println!();

    // --- Codex CLI ---
    print!("[Codex]  Reading credentials... ");
    match ai_quota::read_codex_credentials() {
        Ok(tokens) => {
            println!("OK");
            println!(
                "  token:      {}...",
                &tokens.access_token[..20.min(tokens.access_token.len())]
            );
            println!("  account_id: {:?}", tokens.account_id);

            print!("[Codex]  Fetching quota... ");
            match ai_quota::fetch_codex_quota(&tokens).await {
                Ok(info) => {
                    println!("OK");
                    println!("  utilization: {:.1}%", info.utilization_pct);
                    println!("  progress:    {}/100", info.as_progress());
                    println!("  display:     {}", info.as_display_text());
                }
                Err(e) => println!("FAILED: {e}"),
            }
        }
        Err(e) => println!("FAILED: {e}"),
    }

    println!("\n=== Done ===");
}
