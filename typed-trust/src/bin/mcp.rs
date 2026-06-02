//! `typed-trust-mcp`: MCP server for the typed-trust corpus.

use std::process::ExitCode;

use typed_trust::loader::AllowListPathPolicy;
use typed_trust::mcp;

fn print_usage() {
    eprintln!(
        "usage: typed-trust-mcp [--allow-manifest <path>] ...\n\n\
         Phase 3 MCP server for the typed-trust corpus. Reads\n\
         JSON-RPC 2.0 frames from stdin and writes responses to\n\
         stdout. Configure with --allow-manifest <dir-or-file>\n\
         (repeatable) to restrict which manifests can be queried.\n"
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut policy = AllowListPathPolicy::new();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--allow-manifest" => {
                let Some(p) = iter.next() else {
                    eprintln!("error: --allow-manifest requires a path");
                    print_usage();
                    return ExitCode::from(2);
                };
                if let Err(denied) = policy.allow(p) {
                    eprintln!("error: cannot register {p}: {denied}");
                    return ExitCode::from(2);
                }
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unknown argument {other}");
                print_usage();
                return ExitCode::from(2);
            }
        }
    }

    let state = mcp::build_state(policy);
    if let Err(e) = mcp::run(state).await {
        eprintln!("server error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
