#![cfg_attr(not(feature = "std"), no_std)]
#![no_main]

mod board;

#[cfg(feature = "defmt")]
use defmt::error;
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use board::Board;
use embassy_executor::Spawner;
use embassy_stm32::peripherals;
use embassy_stm32::usart::{BufferedUartRx, BufferedUartTx};
use embassy_stm32::usb;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher, Subscriber, WaitResult};
use embassy_usb::class::cdc_acm;
use embedded_io_async::{Write, Read};
use heapless::Vec;

type ToUsbBuf = Vec<u8, 63>;
type ToUsbChannel = PubSubChannel<ThreadModeRawMutex, ToUsbBuf, 4, 1, 1>;
type ToUsbChannelPublisher =
    Publisher<'static, ThreadModeRawMutex, ToUsbBuf, 4, 1, 1>;
type ToUsbChannelSubscriber =
    Subscriber<'static, ThreadModeRawMutex, ToUsbBuf, 4, 1, 1>;
static TO_USB: ToUsbChannel = PubSubChannel::new();

type ToUartBuf = Vec<u8, 64>;
type ToUartChannel = PubSubChannel<ThreadModeRawMutex, ToUartBuf, 4, 1, 1>;
type ToUartChannelPublisher =
    Publisher<'static, ThreadModeRawMutex, ToUartBuf, 4, 1, 1>;
type ToUartChannelSubscriber =
    Subscriber<'static, ThreadModeRawMutex, ToUartBuf, 4, 1, 1>;
static TO_UART: ToUartChannel = PubSubChannel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let board = Board::new();

    let uart_tx = board.uart_tx;
    let to_uart_sub = TO_UART.subscriber().unwrap();
    spawner.must_spawn(uart_sender(uart_tx, to_uart_sub));

    let usb_cdc_tx = board.usb_cdc_tx;
    let to_pc_sub = TO_USB.subscriber().unwrap();
    spawner.must_spawn(usb_sender(usb_cdc_tx, to_pc_sub));

    let uart_rx = board.uart_rx;
    let to_pc_pub = TO_USB.publisher().unwrap();
    spawner.must_spawn(uart_receiver(uart_rx, to_pc_pub));

    let usb_cdc_rx = board.usb_cdc_rx;
    let to_uart_pub = TO_UART.publisher().unwrap();
    spawner.must_spawn(usb_receiver(usb_cdc_rx, to_uart_pub));

    let mut usb = board.usb;
    loop {
        usb.run().await;
    }
}

#[embassy_executor::task]
async fn uart_sender(
    mut uart_tx: BufferedUartTx<'static>,
    mut to_uart_sub: ToUartChannelSubscriber,
) {
    loop {
        let buf = match to_uart_sub.next_message().await {
            WaitResult::Lagged(n) => {
                #[cfg(feature = "defmt")]
                error!("Missed {:?} bytes to send to payload", n);
                None
            }
            WaitResult::Message(buf) => Some(buf),
        };
        if let Some(buf) = buf {
            if let Err(e) = uart_tx.write_all(&buf).await {
                #[cfg(feature = "defmt")]
                error!("UART TX err: {:?}", e);
            }
        }
    }
}
#[embassy_executor::task]
async fn uart_receiver(
    mut uart_rx: BufferedUartRx<'static>,
    to_usb_pub: ToUsbChannelPublisher,
) {
    let mut buf = [0; 63];
    loop {
        let result = uart_rx.read(&mut buf).await;
        match result {
            Ok(n) => {
                let data: ToUsbBuf = buf[..n].try_into().unwrap();
                to_usb_pub.publish(data).await;
            }
            Err(e) => {
                #[cfg(feature = "defmt")]
                error!("UART RX error {:?}", e);
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
    mut to_usb_sub: ToUsbChannelSubscriber,
) {
    loop {
        cdc_tx.wait_connection().await;
        loop {
            let buf = match to_usb_sub.next_message().await {
                WaitResult::Lagged(n) => {
                    #[cfg(feature = "defmt")]
                    error!("Missed {:?} packets to send to the payload", n);
                    None
                }
                WaitResult::Message(buf) => Some(buf),
            };
            if let Some(buf) = buf {
                if let Err(e) = cdc_tx.write_packet(&buf).await {
                    #[cfg(feature = "defmt")]
                    error!("CDC TX err: {:?}", e);
                    break;
                }
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
    to_uart_pub: ToUartChannelPublisher,
) {
    loop {
        cdc_rx.wait_connection().await;
        let mut buf = [0; 64];
        loop {
            if let Ok(n) = cdc_rx.read_packet(&mut buf).await {
                let data: ToUartBuf = buf[..n].try_into().unwrap();
                to_uart_pub.publish(data).await;
            } else {
                break;
            }
        }
    }
}
