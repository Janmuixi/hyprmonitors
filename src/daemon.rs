use anyhow::Result;
use hyprland::event_listener::AsyncEventListener;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{error, info};
use hyprmonitor::model::{Monitor, MonitorConfig};

pub async fn run() -> Result<()> {
    info!("hyprmonitor daemon starting");
    reconfigure().await;

    let trigger = Arc::new(Notify::new());

    // Reactor task — debounced reconfigure on each notification.
    let reactor_trigger = trigger.clone();
    tokio::spawn(async move {
        loop {
            reactor_trigger.notified().await;
            loop {
                let sleep = tokio::time::sleep(std::time::Duration::from_millis(200));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    _ = reactor_trigger.notified() => continue,
                }
            }
            reconfigure().await;
        }
    });

    // Listener task — reconnect with exponential backoff.
    let mut backoff_secs: u64 = 1;
    loop {
        let trigger_added = trigger.clone();
        let trigger_removed = trigger.clone();

        let mut listener = AsyncEventListener::new();
        listener.add_monitor_added_handler(move |data| {
            let t = trigger_added.clone();
            Box::pin(async move {
                info!("monitor added: {}", data.name);
                t.notify_one();
            })
        });
        listener.add_monitor_removed_handler(move |name| {
            let t = trigger_removed.clone();
            Box::pin(async move {
                info!("monitor removed: {}", name);
                t.notify_one();
            })
        });

        info!("subscribing to hyprland events");
        match listener.start_listener_async().await {
            Ok(()) => {
                info!("event listener exited cleanly; reconnecting");
                backoff_secs = 1;
            }
            Err(e) => {
                error!(
                    "event listener error: {:?}; retrying in {}s",
                    e, backoff_secs
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(30);
                continue;
            }
        }

        // Listener exited cleanly. Try to reconnect immediately, and run
        // a reconfigure pass since Hyprland may have just restarted.
        reconfigure().await;
    }
}

async fn reconfigure() {
    let monitors = match crate::hypr::query_monitors().await {
        Ok(m) => m,
        Err(e) => {
            error!("failed to query monitors: {:?}", e);
            crate::notify::notify_failure(&format!("Failed to query monitors: {}", e));
            return;
        }
    };
    let mut plan = hyprmonitor::algo::plan(&monitors);
    let cfg = hyprmonitor::config::load_or_default(&hyprmonitor::config::default_path());
    hyprmonitor::config::merge_into_plan(&mut plan, &monitors, &cfg);
    info!("applying {} monitor configs (batched)", plan.len());
    if let Err(e) = crate::hypr::apply_batch(&plan).await {
        error!("apply failed: {:?}", e);
        crate::notify::notify_failure(&format!("Failed to configure monitors: {}", e));
    }
    verify_applied(&plan).await;
}

async fn verify_applied(plan: &[MonitorConfig]) {
    let after = match crate::hypr::query_monitors().await {
        Ok(m) => m,
        Err(e) => {
            error!("verify: failed to re-query monitors: {:?}", e);
            return;
        }
    };
    for cfg in plan {
        let Some(actual) = after.iter().find(|m| m.name == cfg.name) else {
            crate::notify::notify_failure(&format!(
                "{} disappeared after apply",
                cfg.name
            ));
            continue;
        };
        if !mode_matches(actual, cfg) {
            crate::notify::notify_failure(&format!(
                "{}: requested {}x{}@{} but got {}x{}",
                cfg.name,
                cfg.mode.width,
                cfg.mode.height,
                cfg.mode.refresh_hz,
                actual.width_px,
                actual.height_px,
            ));
        }
    }
}

fn mode_matches(actual: &Monitor, cfg: &MonitorConfig) -> bool {
    // Our Monitor type only carries current resolution, not refresh rate.
    // Width/height mismatch is what we can detect reliably.
    actual.width_px == cfg.mode.width && actual.height_px == cfg.mode.height
}

