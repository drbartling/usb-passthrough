# USB Passthrough Example

Example of creating a USB to UART bridge using Embassy

## About

A simple USB to UART bridge using Embassy.  Demonstrates splitting peripherals
into senders and receivers.  Using DMA to prevent buffer overruns on UART since
the STM32G0 parts can only buffer a single byte without DMA.

Not yet complete, since right now the UART is fixed at 115_200 bps, and so if
you use this with a computer to set the baudrate, it will fail.

You can build and run this on a NUCLEO-G0B1RE with a few small modifications:

- In `pyocd.yml`:
	- Switch the `target_override` to `STM32G0B1RETx`
- In `./.cargo/config.toml`:
	- Change the `chip` to STM32G0B1RETx
- In `memory.x`:
	- No changes needed

Nucleo Board setup with a USB breakout board:

- Connect d+ to CN10 pin 12 (PA12)
- Connect d- to CN10 pin 14 (PA11)
- Connect VBus to CN10 pin 8 (5V_USB_CHG)
- Connect GND to CN10 pin 20 (GND)
- For loopback, connect CN7 pin 28 to CN7 pin 30
	- This connects UART4 RX to TX


## References
- [NUCLEO-G0B1RE Schematic](https://www.st.com/resource/en/schematic_pack/mb1360-g0b1re-c02_schematic.pdf)
- [USB-A Breakout](https://www.adafruit.com/product/4448)
- [USB-C Breakout](https://www.adafruit.com/product/4090)
