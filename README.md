# s3-4-key

Firmware for the [M5Stack StickS3](https://docs.m5stack.com/en/core/StickS3) that turns it into a compact Bluetooth keyboard:

- **A, S, D, F** on four external mechanical switches (direct-plug, no breadboard)
- **Right side button** = Backspace
- **Blue front button** = reset pairing (erase the saved bond and re-enter pairing mode)
- **Left side button** = native power control (single click power-on/reset, double click off, hold for download mode)
- Screen shows battery estimate, connection status, last key pressed, and `press blue to pair`

Written in Rust (ESP-IDF + NimBLE via `esp-idf-svc` / `esp32-nimble`), with the input,
HID, battery and UI logic in a hardware-independent core crate that is covered by
host-side unit tests (`cargo test`).

## Wiring

Plug the switches directly into the StickS3 header. Each switch uses two adjacent
pin rows; the firmware holds the return pins LOW (open-drain), so every switch
reads LOW while pressed.

| Switch | Signal pin | Other lead | Note |
| --- | --- | --- | --- |
| A | `G1` | `GND` | The only true ground |
| S | `G4` | `G2` | G2 held LOW by firmware |
| D | `G6` | `G5` | G5 held LOW by firmware |
| F | `G8` | `G7` | G7 held LOW by firmware |

Do not wire anything to `5V`, `BAT`, `3V3`, `EXT_5V` or `5V_IN`. The S3 is a 3.3 V device.

Power it from USB while flashing; afterwards it runs from the built-in 250 mAh battery.

## Building and flashing

One-time toolchain setup (macOS Apple Silicon shown; see [The Rust on ESP Book](https://docs.espressif.com/projects/rust/book/) for others):

```sh
cargo install espup --locked
espup install --std --targets esp32s3   # installs the `esp` toolchain (Xtensa)
cargo install ldproxy --locked
source "$HOME/export-esp.sh"            # add to your shell profile to make permanent
rustup target add xtensa-esp32s3-espidf --toolchain esp
```

Build, flash and monitor (from the `firmware/` directory):

```sh
cd firmware
cargo +esp firmware                                        # build only (alias)
cargo +esp espflash flash --release --partition-table ../partitions.csv --monitor
```

The first build downloads and compiles ESP-IDF, which takes a while.
If the device refuses to enter flashing mode, hold the left button while pressing
it once more (download mode, green LED flashes) and retry.

To pair: enable Bluetooth on your computer, press and release the blue button,
then select **S3 4-Key** in the Bluetooth device list. After a pairing reset
(blue button), the computer may still remember the old pairing - remove/forget
**S3 4-Key** there once, then press blue again and re-pair.

## Project layout

```
src/            hardware-independent core (tests run on your computer)
  input.rs      debouncing, key suppression, pairing button edge detection
  hid.rs        HID keyboard report map + report composition
  battery.rs    voltage -> percentage estimation
  ui.rs         135x240 screen layout
  power.rs      M5PM1 PMIC register logic (LCD power, battery, charging)
firmware/       StickS3-specific glue: GPIO/SPI/I2C, display, NimBLE HID
sdkconfig.defaults   ESP-IDF configuration (NimBLE, bonding persistence)
partitions.csv       flash layout (1 MB NVS-free, 3 MB app)
```

## Behaviour notes

- Keys are debounced (12 ms) and support holding; the computer handles key repeat.
- Keys held while Bluetooth is down are never replayed into the new connection.
- Bonds persist across power cycles in NVS (`CONFIG_BT_NIMBLE_NVS_PERSIST`).
- Battery percentage is estimated from PMIC voltage telemetry, not a fuel gauge;
  values while charging can read high. Charging shows as `+`. The BLE Battery
  Service reports a fixed placeholder value; the screen shows the real estimate.
- The right-side Backspace button and the display share no state: keyboard input
  keeps working even if the screen fails to initialise.
- A panic in the input/HID core is isolated: the keyboard stops sending and the
  stack re-initialises rather than streaming garbage. Repeated fatal errors
  trigger a clean restart.
- The display can be ignored entirely; keys keep working without it.
