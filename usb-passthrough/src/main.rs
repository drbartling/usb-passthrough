#![cfg_attr(not(feature = "std"), no_std)]
#![no_main]
#[macro_use]

mod board;

#[cfg(feature = "defmt")]
use defmt::error;
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use board::Board;
use embassy_executor::Spawner;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{RingBufferedUartRx, UartTx};
use embassy_stm32::{peripherals, usb};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pipe;
use embassy_usb::class::cdc_acm;
use embedded_io_async::Write;
use static_cell::StaticCell;

macro_rules! static_mut_ref {
    ($t:ty, $i:expr) => {{
        static CELL: StaticCell<$t> = StaticCell::new();
        CELL.init($i)
    }};
}
pub(crate) use static_mut_ref;

type ToUsbPipe = pipe::Pipe<NoopRawMutex, 256>;
type ToUsbWriter = pipe::Writer<'static, NoopRawMutex, 256>;
type ToUsbReader = pipe::Reader<'static, NoopRawMutex, 256>;

type ToUartPipe = pipe::Pipe<NoopRawMutex, 256>;
type ToUartWriter = pipe::Writer<'static, NoopRawMutex, 256>;
type ToUartReader = pipe::Reader<'static, NoopRawMutex, 256>;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let board = Board::new();

    let to_uart_pipe = static_mut_ref!(ToUartPipe, ToUartPipe::new());
    let (to_uart_receiver, to_uart_sender) = to_uart_pipe.split();

    let to_usb_pipe = static_mut_ref!(ToUsbPipe, ToUsbPipe::new());
    let (to_usb_receiver, to_usb_sender) = to_usb_pipe.split();

    let uart_tx = board.uart_tx;
    spawner.must_spawn(uart_sender(uart_tx, to_uart_receiver));

    let usb_cdc_tx = board.usb_cdc_tx;
    spawner.must_spawn(usb_sender(usb_cdc_tx, to_usb_receiver));

    let uart_rx = board.uart_rx;
    spawner.must_spawn(uart_receiver(uart_rx, to_usb_sender));

    let usb_cdc_rx = board.usb_cdc_rx;
    spawner.must_spawn(usb_receiver(usb_cdc_rx, to_uart_sender));

    let mut usb = board.usb;
    loop {
        usb.run().await;
    }
}

#[embassy_executor::task]
async fn uart_sender(
    mut uart_tx: UartTx<'static, Async>,
    from_usb: ToUartReader,
) {
    let mut buf = [0; 256];
    loop {
        let n = from_usb.read(&mut buf).await;
        let data = &buf[..n];
        if let Err(e) = uart_tx.write(data).await {
            #[cfg(feature = "defmt")]
            error!("UART TX err: {:?}", e);
        }
    }
}

#[embassy_executor::task]
async fn uart_receiver(
    mut uart_rx: RingBufferedUartRx<'static>,
    mut to_usb: ToUsbWriter,
) {
    let mut buf = [0; 63];
    loop {
        let result = uart_rx.read(&mut buf).await;
        match result {
            Ok(n) => {
                let data = &buf[..n];
                to_usb.write_all(data).await.unwrap();
            }
            Err(e) => {
                #[cfg(feature = "defmt")]
                error!("UART RX error: `{:?}`", e);
            }
        }
    }
}

#[embassy_executor::task]
async fn usb_sender(
    mut cdc_tx: cdc_acm::Sender<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
    from_uart: ToUsbReader,
) {
    loop {
        cdc_tx.wait_connection().await;
        let mut buf = [0; 63];
        loop {
            let n = from_uart.read(&mut buf).await;
            let data = &buf[..n];
            if let Err(e) = cdc_tx.write_packet(data).await {
                #[cfg(feature = "defmt")]
                error!("CDC TX err: {:?}", e);
                break;
            }
        }
    }
}

#[embassy_executor::task]
async fn usb_receiver(
    mut cdc_rx: cdc_acm::Receiver<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
    mut to_uart: ToUartWriter,
) {
    loop {
        cdc_rx.wait_connection().await;
        let mut buf = [0; 64];
        loop {
            if let Ok(n) = cdc_rx.read_packet(&mut buf).await {
                let data = &buf[..n];
                to_uart.write_all(data).await.unwrap();
            } else {
                break;
            }
        }
    }
}
