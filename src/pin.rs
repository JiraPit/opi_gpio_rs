//
// This file provides a representation of a GPIO pin which can be either an input or an output.
// It helps to ensure that the pin is properly initialized and exported to sysfs interface.
// This module uses a combination of the `gpio` command for export operations and direct
// sysfs interface for reading, writing, and mode operations.
//

use anyhow::{Context, Result, bail};
use std::env;
use tokio::{fs, process::Command};

/// Represents a GPIO pin which can either be an input or an output but not both.
/// Creating [GpioPin] directly is not recommended, use [GpioPin::new_input] or
/// [GpioPin::new_output] instead to ensure the pin is properly initialized.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum GpioPin {
    Input { pin_number: u8, support_watch: bool },
    Output { pin_number: u8 },
}

impl GpioPin {
    /// Initialize a new input pin
    pub async fn new_input(pin_number: u8) -> Result<Self> {
        // If watch support is disabled, call export
        let output = Command::new("gpio")
            .args(["export", &pin_number.to_string(), "in"])
            .output()
            .await
            .context("Failed to export the pin with gpio command")?;
        if !output.status.success() {
            let error_message = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to export the input pin: {}", error_message);
        }

        Ok(Self::Input {
            pin_number,
            support_watch: false,
        })
    }

    /// Initialize a new output pin
    pub async fn new_output(pin_number: u8, default: u8) -> Result<Self> {
        if default != 0 && default != 1 {
            bail!("Default value must be 0 or 1, got {}", default);
        }

        // Export the pin
        let output = Command::new("gpio")
            .args(["export", &pin_number.to_string(), "out"])
            .output()
            .await
            .context("Failed to export the pin with gpio command")?;
        if !output.status.success() {
            let error_message = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to export the output pin: {}", error_message);
        }

        // Set the default value
        let gpio_dir = env::var("GPIO_DIR").context("GPIO_DIR environment variable not set")?;
        let value_path = format!("{}/gpio{}/value", gpio_dir, pin_number);
        fs::write(&value_path, default.to_string())
            .await
            .context("Failed to set the pin default value")?;

        Ok(Self::Output { pin_number })
    }

    /// Enable edge notification for the pin.
    /// After calling this, [GpioPin::support_watch] will return true.
    /// Normally, edge command will automatically turn the pin into an input pin.
    /// To avoid confusion, this function is not allowed for output pins.
    pub async fn enable_watch(&mut self) -> Result<()> {
        // Call edge command
        match self {
            Self::Input {
                pin_number,
                support_watch,
            } => {
                let output = Command::new("gpio")
                    .args(["edge", &pin_number.to_string(), "both"])
                    .output()
                    .await
                    .context("Failed to edge the pin with gpio command")?;
                if output.status.success() {
                    *support_watch = true;
                    Ok(())
                } else {
                    let error_message = String::from_utf8_lossy(&output.stderr);
                    bail!("Failed to edge the input pin: {}", error_message);
                }
            }
            Self::Output { pin_number: _ } => {
                bail!("Edge notification is not supported for output pins");
            }
        }
    }

    /// Get the path to the value of the pin.
    /// This does NOT guarantee that the pin is exported nor that the path exists.
    pub fn get_value_path(&self) -> String {
        let gpio_dir = env::var("GPIO_DIR").expect("GPIO_DIR not set");

        match self {
            Self::Input {
                pin_number,
                support_watch: _,
            } => format!("{}/gpio{}/value", gpio_dir, pin_number),
            Self::Output { pin_number } => format!("{}/gpio{}/value", gpio_dir, pin_number),
        }
    }

    /// Get the pin number of the pin.
    pub fn get_pin_number(&self) -> u8 {
        match self {
            Self::Input {
                pin_number,
                support_watch: _,
            } => *pin_number,
            Self::Output { pin_number } => *pin_number,
        }
    }

    /// Check if the pin supports watch.
    /// Calling [GpioPin::enable_watch] will enable watch support for input pins.
    /// Output pins always return false.
    pub fn support_watch(&self) -> bool {
        match self {
            Self::Input {
                pin_number: _,
                support_watch,
            } => *support_watch,
            Self::Output { pin_number: _ } => false,
        }
    }

    /// Write a value to the pin.
    pub async fn write(&self, value: u8) -> Result<()> {
        // Check if the value is valid
        if value != 0 && value != 1 {
            bail!("Value must be 0 or 1");
        }

        // Write the value to the pin using sysfs interface
        let value_path = self.get_value_path();
        fs::write(&value_path, value.to_string())
            .await
            .context("Failed to write to the pin")?;

        Ok(())
    }

    /// Read the value from the pin.
    pub async fn read(&self) -> Result<u8> {
        // Read the value from the pin using sysfs interface
        let value_path = self.get_value_path();
        let content = fs::read_to_string(&value_path)
            .await
            .context("Failed to read from the pin")?;

        let value = content
            .trim()
            .parse()
            .context("Failed to parse the value from the pin")?;
        Ok(value)
    }

    #[cfg(test)]
    /// Initialize a **FAKE** input pin.
    /// Only used for testing on devices without actual GPIO pins.
    pub async fn new_fake_input(pin_number: u8) -> Result<Self> {
        let gpio_dir = env::var("GPIO_DIR").expect("GPIO_DIR not set");

        println!("Creating fake input pin {} at {}", pin_number, gpio_dir);

        // Create a new directory and some files to simulate the pin export
        fs::create_dir_all(format!("{}/gpio{}", gpio_dir, pin_number)).await?;

        // set the pin as input
        fs::write(
            format!("{}/gpio{}/direction", gpio_dir, pin_number),
            "in".as_bytes(),
        )
        .await?;

        // set the pin as down
        fs::write(
            format!("{}/gpio{}/value", gpio_dir, pin_number),
            "0".as_bytes(),
        )
        .await?;

        Ok(Self::Input {
            pin_number,
            support_watch: true,
        })
    }
}
