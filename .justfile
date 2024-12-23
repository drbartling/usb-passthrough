default:
    just --list


format:
    cargo fmt

build: format
    cargo build --release

flash: build
    pyocd load ./target/thumbv6m-none-eabi/release/usb-passthrough --format elf

run: build
    cargo run --release

clean:
    rm -rf target
