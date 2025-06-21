#[cfg(test)]
mod gpio_util_tests {
    use super::super::pin::GpioPin;
    use super::super::watcher::GpioWatcher;
    use std::{collections::HashMap, env};
    use tokio::sync::watch;
    use tokio::{fs, time};

    #[tokio::test]
    async fn gpio_watcher_test() {
        unsafe {
            env::set_var("GPIO_DIR", "test_assets/output/gpio");
        }

        // Remove old test outputs
        fs::remove_dir_all("test_assets/output/gpio")
            .await
            .unwrap_or_default();

        // Create a fake GPIO pins
        let gpio1 = GpioPin::new_fake_input(1).await.unwrap();

        // Create a watch channel to check the callback
        let (tx, mut rx) = watch::channel::<u8>(0);

        // Set up the pin-to-callback map
        let mut pin_map = HashMap::new();
        pin_map.insert(gpio1, tx);

        // Initialize the GPIO watcher
        let _watcher = GpioWatcher::new(pin_map).await.unwrap();

        // Check if something is sent to the rx (meaning that the callback is triggered)
        time::timeout(time::Duration::from_secs(1), rx.changed())
            .await
            .unwrap()
            .unwrap();
        let result = *rx.borrow();
        assert!(result == 0);

        // Simulate value changes in both pins
        fs::write("test_assets/output/gpio/gpio1/value", "1".as_bytes())
            .await
            .unwrap();

        // Check if something is sent to the rx (meaning that the callback is triggered)
        time::timeout(time::Duration::from_secs(1), rx.changed())
            .await
            .unwrap()
            .unwrap();
        let result = *rx.borrow();
        assert!(result == 1);
    }
}
