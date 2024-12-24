default:
    just --list

format:
    cargo fmt

build: format
    cargo build --release
    arm-none-eabi-size ./target/thumbv6m-none-eabi/release/usb-passthrough

flash: build
    pyocd load ./target/thumbv6m-none-eabi/release/usb-passthrough --format elf

run: build
    cargo run --release

build-release: format
    cargo build --release --no-default-features
    arm-none-eabi-size ./target/thumbv6m-none-eabi/release/usb-passthrough

flash-release: build-release
    pyocd load ./target/thumbv6m-none-eabi/release/usb-passthrough --format elf

clean:
    cargo clean
