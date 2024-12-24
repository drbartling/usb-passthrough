#![no_std]
#![no_main]

extern crate alloc;
mod board;

#[cfg(feature = "defmt")]
use defmt::{assert_eq, info, panic, warn};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use board::Board;
use core::ptr::addr_of_mut;
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals;
use embassy_stm32::usart::{RingBufferedUartRx, UartTx};
use embassy_stm32::usb;
use embassy_usb::class::cdc_acm;
use embassy_usb::driver::EndpointError;
use embedded_alloc::LlffHeap as Heap;
use embedded_io_async::Write;

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 2048;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] =
            [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }
    }
    let board = Board::new();
    let mut usb = board.usb;
    let usb_fut = usb.run();

    let mut usb_cdc_rx = board.usb_cdc_rx;
    let mut uart_tx = board.uart_tx;
    let usb_to_uart_fut = async {
        loop {
            usb_cdc_rx.wait_connection().await;
            #[cfg(feature = "defmt")]
            info!("CDC RX Connected");
            let _ = usb_to_uart(&mut uart_tx, &mut usb_cdc_rx).await;
            #[cfg(feature = "defmt")]
            info!("CDC RX Disconnected");
        }
    };

    let mut usb_cdc_tx = board.usb_cdc_tx;
    let mut uart_rx = board.uart_rx;
    let uart_to_usb_fut = async {
        loop {
            usb_cdc_tx.wait_connection().await;
            #[cfg(feature = "defmt")]
            info!("CDC TX Connected");
            let _ = uart_to_usb(&mut usb_cdc_tx, &mut uart_rx).await;
            #[cfg(feature = "defmt")]
            info!("CDC TX Disconnected");
        }
    };

    join3(usb_fut, usb_to_uart_fut, uart_to_usb_fut).await;
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn usb_to_uart(
    uart_tx: &mut UartTx<'static, Async>,
    usb_cdc_rx: &mut cdc_acm::Receiver<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
) -> Result<(), Disconnected> {
    // Default packet size is 64
    #[cfg(feature = "defmt")]
    assert_eq!(64, usb_cdc_rx.max_packet_size());
    let mut buf = [0; 64];
    loop {
        let n = usb_cdc_rx.read_packet(&mut buf).await?;
        let data = &buf[..n];
        #[cfg(feature = "defmt")]
        info!("To UART TX {}: {:x}", n, data);
        #[allow(unused_variables)]
        if let Err(e) = uart_tx.write_all(data).await {
            #[cfg(feature = "defmt")]
            warn!("UART TX err: {:?}", e);
            return Err(Disconnected {});
        }
    }
}

async fn uart_to_usb(
    usb_cdc_tx: &mut cdc_acm::Sender<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
    uart_rx: &mut RingBufferedUartRx<'static>,
) -> Result<(), Disconnected> {
    // Default packet size is 64
    #[cfg(feature = "defmt")]
    assert_eq!(usb_cdc_tx.max_packet_size(), 64);
    // Send a max of 1 less than USB max packet size
    // If we send 64, the USB driver expects more to follow
    let mut buf = [0; 63];
    loop {
        let n = match uart_rx.read(&mut buf).await {
            Ok(n) => n,
            #[allow(unused_variables)]
            Err(e) => {
                #[cfg(feature = "defmt")]
                warn!("UART RX err: {:?}", e);
                return Err(Disconnected {});
            }
        };

        let data = &buf[..n];
        #[cfg(feature = "defmt")]
        info!("To CDC TX {}: {:x}", n, data);
        usb_cdc_tx.write_packet(data).await?;
    }
}
