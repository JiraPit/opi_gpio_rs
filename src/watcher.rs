//
// This file provides a way to watch for changes in GPIO pins' state.
// It uses the `inotify` command line tool to watch for changes in the value of the GPIO pins
// through the sysfs interface.
//

use super::pin::GpioPin;
use anyhow::{Context, Result, bail};
use inotify::{EventMask, Inotify, WatchMask};
use std::collections::HashMap;
use tokio::{fs, sync::watch, task::JoinHandle};
use tokio_stream::StreamExt;

/// Watcher for GPIO pins for detecting changes in GPIO pin's
/// value (up or down) and sending notifications through watch channels.
/// A single [GpioWatcher] can be used for multiple pins.
///
/// Dropping this will abort the watcher.
pub struct GpioWatcher {
    watcher_thread: JoinHandle<()>,
}

impl Drop for GpioWatcher {
    fn drop(&mut self) {
        self.watcher_thread.abort();
    }
}

impl GpioWatcher {
    /// Create a new [GpioWatcher] with a map of GPIO pins and watch [Sender]s
    /// to notify the caller when a change is detected.
    /// Dropping this will cancel the watcher.
    pub async fn new(pin_map: HashMap<GpioPin, watch::Sender<u8>>) -> Result<Self> {
        // Check if all pins support watch
        for (pin, _) in &pin_map {
            if !pin.support_watch() {
                bail!("Pin {} does not support watch", pin.get_pin_number());
            }
        }

        // Initialize the notifier map
        let mut notifier_map: HashMap<i32, (String, watch::Sender<u8>)> = HashMap::new();

        // Create an inotify instance and add a watch for each pin
        let inotify = Inotify::init()?;
        for (pin, notifier) in pin_map {
            // Send the initial value of the pin
            notifier
                .send(
                    pin.read()
                        .await
                        .context("Failed to read the initial value for the pin")?,
                )
                .context("Failed to notify the initial value")?;

            // Add a watch for the pin's value file
            let wd = inotify.watches().add(
                pin.get_value_path(),
                WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE,
            )?;
            notifier_map.insert(
                wd.get_watch_descriptor_id(),
                (pin.get_value_path(), notifier),
            );
        }

        // Convert inotify into a stream of events
        let mut event_stream = inotify.into_event_stream([0u8; 4048])?;

        // Spawn the watcher thread
        let watcher_thread = tokio::spawn(async move {
            // Wait for incoming events
            while let Some(Ok(event)) = event_stream.next().await {
                if event.mask.contains(EventMask::MODIFY) {
                    // Get the path and notifier for the event
                    let (value_path, notifier) =
                        match notifier_map.get(&event.wd.get_watch_descriptor_id()) {
                            Some((path, notifier)) => (path, notifier),
                            None => continue,
                        };

                    // Get the value from the file
                    let value = match fs::read_to_string(value_path).await {
                        Ok(value) => value,
                        Err(e) => {
                            log::error!("Error reading GPIO value: {}", e);
                            continue;
                        }
                    };

                    // Notify the caller with the value
                    let message = if value.trim().contains("1") { 1 } else { 0 };
                    if let Err(e) = notifier.send(message) {
                        log::warn!("Error sending message: {}", e);
                    }
                }
            }
        });

        Ok(Self { watcher_thread })
    }
}
