use anyhow::Result;
use hyprland::event_listener::AsyncEventListener;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{error, info};

pub async fn run() -> Result<()> {
    info!("hyprmonitor daemon starting");
    reconfigure().await;

    let trigger = Arc::new(Notify::new());

    let trigger_added = trigger.clone();
    let trigger_removed = trigger.clone();

    let mut listener = AsyncEventListener::new();

    // MonitorAdded delivers MonitorAddedEventData { id, name, description }
    listener.add_monitor_added_handler(move |data| {
        let t = trigger_added.clone();
        Box::pin(async move {
            info!("monitor added: {} ({})", data.name, data.description);
            t.notify_one();
        })
    });

    // MonitorRemoved delivers a String (the monitor name)
    listener.add_monitor_removed_handler(move |name| {
        let t = trigger_removed.clone();
        Box::pin(async move {
            info!("monitor removed: {}", name);
            t.notify_one();
        })
    });

    // Spawn a task that reacts to events.
    let reactor_trigger = trigger.clone();
    tokio::spawn(async move {
        loop {
            reactor_trigger.notified().await;
            reconfigure().await;
        }
    });

    info!("subscribing to hyprland events");
    listener.start_listener_async().await?;
    Ok(())
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
    let plan = crate::algo::plan(&monitors);
    info!("applying {} monitor configs", plan.len());
    for cfg in &plan {
        if let Err(e) = crate::hypr::apply(cfg).await {
            error!("apply failed for {}: {:?}", cfg.name, e);
            crate::notify::notify_failure(&format!(
                "Failed to configure {}: {}",
                cfg.name, e
            ));
        }
    }
}
