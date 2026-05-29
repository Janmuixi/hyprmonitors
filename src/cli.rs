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
        Command::Apply { dry_run } => apply(dry_run).await,
        Command::List => list().await,
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

async fn list() -> Result<()> {
    let monitors = crate::hypr::query_monitors().await?;
    let plan = crate::algo::plan(&monitors);

    println!("Detected monitors:");
    for m in &monitors {
        println!(
            "  {} {}x{} physical_mm={:?} modes={}",
            m.name,
            m.width_px,
            m.height_px,
            m.physical_mm,
            m.available_modes.len()
        );
    }
    println!();
    println!("Plan:");
    for cfg in &plan {
        println!("  monitor = {}", cfg);
    }
    Ok(())
}

async fn apply(dry_run: bool) -> Result<()> {
    let monitors = crate::hypr::query_monitors().await?;
    let plan = crate::algo::plan(&monitors);

    if dry_run {
        for cfg in &plan {
            println!("hyprctl keyword monitor {}", cfg);
        }
        return Ok(());
    }

    for cfg in &plan {
        tracing::info!("applying {}", cfg);
        if let Err(e) = crate::hypr::apply(cfg).await {
            tracing::error!("apply failed for {}: {:?}", cfg.name, e);
            crate::notify::notify_failure(&format!(
                "Failed to configure {}: {}",
                cfg.name, e
            ));
        }
    }
    Ok(())
}
