use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "hyprmonitor", version, about = "Auto-configure Hyprland monitors")]
pub struct Cli {
    #[arg(short, long, global = true, help = "Enable debug logging")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run as a daemon, reconfiguring on monitor hotplug events.
    Daemon,
    /// One-shot: detect monitors, apply best config, exit.
    Apply {
        #[arg(long, help = "Print hyprctl commands instead of running them")]
        dry_run: bool,
    },
    /// Print detected monitors and the plan without applying.
    List,
}

pub async fn run(cli: Cli) -> Result<()> {
    init_tracing(cli.verbose);
    match cli.command {
        Command::Daemon => {
            info!("daemon mode not yet implemented");
            Ok(())
        }
        Command::Apply { dry_run } => {
            info!("apply (dry_run={}) not yet implemented", dry_run);
            Ok(())
        }
        Command::List => {
            info!("list not yet implemented");
            Ok(())
        }
    }
}

fn init_tracing(verbose: bool) {
    let level = if verbose { "debug" } else { "info" };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("hyprmonitor={}", level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
