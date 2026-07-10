use std::process::ExitCode;

use clap::Parser;
use parsync::cli::Cli;

fn main() -> ExitCode {
    if std::env::args().any(|arg| arg == "--internal-remote-helper") {
        return match parsync::remote_helper::run_stdio() {
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        };
    }
    #[cfg(target_os = "linux")]
    if std::env::args().any(|arg| arg == "--internal-rdma-send") {
        return match parsync::rdma::run_send_stdio() {
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        };
    }

    let cli = Cli::parse();
    let debug = cli.debug;
    match parsync::run_sync(cli) {
        Ok(summary) => {
            if debug {
                eprintln!(
                    "completed: transferred={}, skipped={}, skipped_symlinks={}, bytes={}, delta_files={}, delta_fallbacks={}, rdma_files={}, rdma_fallbacks={}, rdma_bytes={}, bytes_saved={}, listing_ms={}, planning_ms={}, read_ms={}, write_ms={}, finalize_ms={}, metadata_ms={}, state_commit_ms={}",
                    summary.transferred_files,
                    summary.skipped_files,
                    summary.skipped_symlinks,
                    summary.transferred_bytes,
                    summary.delta_files,
                    summary.delta_fallback_files,
                    summary.rdma_files,
                    summary.rdma_fallback_files,
                    summary.rdma_bytes,
                    summary.bytes_saved,
                    summary.listing_ms,
                    summary.planning_ms,
                    summary.transfer_read_ms,
                    summary.transfer_write_ms,
                    summary.transfer_finalize_ms,
                    summary.metadata_ms,
                    summary.state_commit_ms
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
