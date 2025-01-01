#![cfg_attr(not(feature = "std"), no_std)]
#![no_main]

extern crate alloc;
mod board;

#[cfg(feature = "defmt")]
use defmt::{error, info};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use board::Board;
use core::ptr::addr_of_mut;
use embassy_executor::Spawner;
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals;
use embassy_stm32::usart::{RingBufferedUartRx, UartTx};
use embassy_stm32::usb;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher, Subscriber, WaitResult};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm;
use embedded_alloc::LlffHeap as Heap;
use embedded_io_async::Write;
use heapless::Vec;

#[global_allocator]
static HEAP: Heap = Heap::empty();

type ToUsbBuf = Vec<u8, 63>;
type ToUsbChannel = PubSubChannel<ThreadModeRawMutex, ToUsbBuf, 5, 1, 1>;
type ToUsbChannelPublisher =
    Publisher<'static, ThreadModeRawMutex, ToUsbBuf, 5, 1, 1>;
type ToUsbChannelSubscriber =
    Subscriber<'static, ThreadModeRawMutex, ToUsbBuf, 5, 1, 1>;
static TO_USB: ToUsbChannel = PubSubChannel::new();

type ToUartBuf = Vec<u8, 64>;
type ToUartChannel = PubSubChannel<ThreadModeRawMutex, ToUartBuf, 5, 1, 2>;
type ToUartChannelPublisher =
    Publisher<'static, ThreadModeRawMutex, ToUartBuf, 5, 1, 2>;
type ToUartChannelSubscriber =
    Subscriber<'static, ThreadModeRawMutex, ToUartBuf, 5, 1, 2>;
static TO_UART: ToUartChannel = PubSubChannel::new();

static mut USB_RX_STATE: UsbRxState = UsbRxState::Disconnected;
#[derive(Copy, Clone, Debug, PartialEq)]
enum UsbRxState {
    Disconnected,
    Connected,
    Receiving,
}
async fn usb_rx_state_set(state: UsbRxState) {
    unsafe {
        USB_RX_STATE = state;
    }
    Timer::after(Duration::MIN).await
}

static mut USB_TX_STATE: UsbTxState = UsbTxState::Disconnected;
#[derive(Copy, Clone, Debug, PartialEq)]
enum UsbTxState {
    Disconnected,
    Connected,
    Transmitting,
}
async fn usb_tx_state_set(state: UsbTxState) {
    // These state values are small.  Reading and updating is atomic.  So I expect updating at any
    // stage to be safe.  Especially if it's meant as an observation point and rather than something
    // used to control the USB or UART directly.
    unsafe {
        USB_TX_STATE = state;
    }
    // Momentarily yield to other tasks that may want to observe the state change.
    // Not every change needs to be acted on, these are meant to allow us to see the current state
    // of the system.  It's likely that there's a better way to handle this.
    Timer::after(Duration::MIN).await
}

static mut UART_RX_STATE: UartRxState = UartRxState::Idle;
#[derive(Copy, Clone, Debug, PartialEq)]
enum UartRxState {
    Idle,
    Receiving,
}
async fn uart_rx_state_set(state: UartRxState) {
    unsafe {
        UART_RX_STATE = state;
    }
    Timer::after(Duration::MIN).await
}

static mut UART_TX_STATE: UartTxState = UartTxState::Idle;
#[derive(Copy, Clone, Debug, PartialEq)]
enum UartTxState {
    Idle,
    Transmitting,
}
async fn uart_tx_state_set(state: UartTxState) {
    unsafe {
        UART_TX_STATE = state;
    }
    Timer::after(Duration::MIN).await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 10 * 1024;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] =
            [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }
    }
    let board = Board::new();

    let uart_tx = board.uart_tx;
    let to_payload_sub = TO_UART.subscriber().unwrap();
    spawner.must_spawn(uart_sender(uart_tx, to_payload_sub));

    let usb_cdc_tx = board.usb_cdc_tx;
    let to_pc_sub = TO_USB.subscriber().unwrap();
    spawner.must_spawn(usb_sender(usb_cdc_tx, to_pc_sub));

    let usb_cdc_rx = board.usb_cdc_rx;
    let to_payload_pub = TO_UART.publisher().unwrap();
    spawner.must_spawn(usb_receiver(usb_cdc_rx, to_payload_pub));

    let uart_rx = board.uart_rx;
    let to_pc_pub = TO_USB.publisher().unwrap();
    spawner.must_spawn(uart_receiver(uart_rx, to_pc_pub));

    let led = board.led;
    spawner.must_spawn(show_status(led));

    let mut usb = board.usb;
    loop {
        usb.run().await;
    }
}

#[embassy_executor::task]
async fn uart_sender(
    mut uart_tx: UartTx<'static, Async>,
    mut to_uart_sub: ToUartChannelSubscriber,
) {
    loop {
        uart_tx_state_set(UartTxState::Idle).await;
        let buf = match to_uart_sub.next_message().await {
            WaitResult::Lagged(n) => {
                #[cfg(feature = "defmt")]
                error!("Missed {:?} bytes to send to payload", n);
                None
            }
            WaitResult::Message(buf) => Some(buf),
        };
        if let Some(buf) = buf {
            uart_tx_state_set(UartTxState::Transmitting).await;
            Timer::after(Duration::MIN).await;
            if let Err(e) = uart_tx.write_all(&buf).await {
                #[cfg(feature = "defmt")]
                error!("UART TX err: {:?}", e);
            }
        }
    }
}

#[embassy_executor::task]
async fn uart_receiver(
    mut uart_rx: RingBufferedUartRx<'static>,
    to_usb_pub: ToUsbChannelPublisher,
) {
    let mut buf = [0; 63];
    loop {
        uart_rx_state_set(UartRxState::Idle).await;
        let result = uart_rx.read(&mut buf).await;
        uart_rx_state_set(UartRxState::Receiving).await;
        match result {
            Ok(n) => {
                let data: ToUsbBuf = buf[..n].try_into().unwrap();
                #[cfg(feature = "defmt")]
                info!("From Payload {}: {:x}", n, data.as_slice());
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
        usb_tx_state_set(UsbTxState::Disconnected).await;
        cdc_tx.wait_connection().await;
        loop {
            usb_tx_state_set(UsbTxState::Connected).await;
            let buf = match to_usb_sub.next_message().await {
                WaitResult::Lagged(n) => {
                    #[cfg(feature = "defmt")]
                    error!("Missed {:?} packets to send to the payload", n);
                    None
                }
                WaitResult::Message(buf) => Some(buf),
            };
            if let Some(buf) = buf {
                usb_tx_state_set(UsbTxState::Transmitting).await;
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
        usb_rx_state_set(UsbRxState::Disconnected).await;
        cdc_rx.wait_connection().await;
        let mut buf = [0; 64];
        loop {
            usb_rx_state_set(UsbRxState::Connected).await;
            if let Ok(n) = cdc_rx.read_packet(&mut buf).await {
                usb_rx_state_set(UsbRxState::Receiving).await;
                let data: ToUartBuf = buf[..n].try_into().unwrap();
                #[cfg(feature = "defmt")]
                info!("From PC {}: {:x}", n, data.as_slice());
                to_uart_pub.publish(data).await;
            } else {
                break;
            }
        }
    }
}

#[embassy_executor::task]
async fn show_status(mut led: board::Led) {
    loop {
        Timer::after(Duration::MIN).await;
        let usb_tx_state = unsafe { USB_TX_STATE };
        let usb_rx_state = unsafe { USB_RX_STATE };
        let uart_tx_state = unsafe { UART_TX_STATE };
        let uart_rx_state = unsafe { UART_RX_STATE };

        match (usb_tx_state, usb_rx_state) {
            (UsbTxState::Disconnected, UsbRxState::Disconnected) => {
                show_uart_status(&mut led, uart_tx_state, uart_rx_state);
            }
            _ => {
                show_usb_status(&mut led, usb_tx_state, usb_rx_state).await;
            }
        }
    }
}

fn show_uart_status(
    led: &mut board::Led,
    tx_state: UartTxState,
    rx_state: UartRxState,
) {
    if tx_state == UartTxState::Idle && rx_state == UartRxState::Idle {
        led.off();
        return;
    }
    led.on();
}

async fn show_usb_status(
    led: &mut board::Led,
    tx_state: UsbTxState,
    rx_state: UsbRxState,
) {
    match (tx_state, rx_state) {
        (UsbTxState::Connected, UsbRxState::Connected) => led.on(),
        (UsbTxState::Disconnected, UsbRxState::Disconnected) => {
            led.off();
            panic!("Should never happen");
        }
        (UsbTxState::Disconnected, UsbRxState::Connected) => {
            show_error(led).await
        }
        (UsbTxState::Disconnected, UsbRxState::Receiving) => {
            show_error(led).await
        }
        (UsbTxState::Connected, UsbRxState::Disconnected) => {
            show_error(led).await
        }
        (UsbTxState::Transmitting, UsbRxState::Disconnected) => {
            show_error(led).await
        }
        (UsbTxState::Connected, UsbRxState::Receiving) => {
            show_activity(led).await
        }
        (UsbTxState::Transmitting, UsbRxState::Receiving) => {
            show_activity(led).await
        }
        (UsbTxState::Transmitting, UsbRxState::Connected) => {
            show_activity(led).await
        }
    }
}

async fn show_error(led: &mut board::Led) {
    led.on();
    for _ in 0..4 {
        Timer::after(Duration::from_millis(200)).await;
        led.off();
        Timer::after(Duration::from_millis(200)).await;
        led.on();
    }
}

async fn show_activity(led: &mut board::Led) {
    led.on();
    Timer::after(Duration::from_millis(50)).await;
    led.off();
    Timer::after(Duration::from_millis(50)).await;
    led.on();
}
