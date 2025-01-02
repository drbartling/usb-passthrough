default:
    just --list

format:
    cargo fmt

build: format
    cargo build --release
    arm-none-eabi-size ./target/thumbv6m-none-eabi/release/led-toggle
    arm-none-eabi-objdump -drwCS ./target/thumbv6m-none-eabi/release/led-toggle > ./target/thumbv6m-none-eabi/release/led-toggle.asm
    arm-none-eabi-objcopy -Obinary ./target/thumbv6m-none-eabi/release/led-toggle ./target/thumbv6m-none-eabi/release/led-toggle.bin

flash: build
    pyocd load ./target/thumbv6m-none-eabi/release/led-toggle --format elf

run: build
    cargo run --release

build-release: format
    cargo build --release --no-default-features
    arm-none-eabi-size ./target/thumbv6m-none-eabi/release/led-toggle
    arm-none-eabi-objdump -drwCS ./target/thumbv6m-none-eabi/release/led-toggle > ./target/thumbv6m-none-eabi/release/led-toggle.asm
    arm-none-eabi-objcopy -Obinary ./target/thumbv6m-none-eabi/release/led-toggle ./target/thumbv6m-none-eabi/release/led-toggle.bin

flash-release: build-release
    pyocd load ./target/thumbv6m-none-eabi/release/led-toggle --format elf

clean:
    cargo clean
