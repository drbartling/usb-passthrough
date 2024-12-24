#![no_std]
#![no_main]

extern crate alloc;
mod board;

#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use board::Board;
use core::ptr::addr_of_mut;
use defmt::{panic, *};
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_stm32::peripherals;
use embassy_stm32::usart::{BufferedUartRx, BufferedUartTx};
use embassy_stm32::usb;
use embassy_usb::class::cdc_acm;
use embassy_usb::driver::EndpointError;
use embedded_alloc::LlffHeap as Heap;
use embedded_io_async::{Read, Write};

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
            info!("CDC RX Connected");
            let _ = usb_to_uart(&mut uart_tx, &mut usb_cdc_rx).await;
            info!("CDC RX Disconnected");
        }
    };

    let mut usb_cdc_tx = board.usb_cdc_tx;
    let mut uart_rx = board.uart_rx;
    let uart_to_usb_fut = async {
        loop {
            usb_cdc_tx.wait_connection().await;
            info!("CDC TX Connected");
            let _ = uart_to_usb(&mut usb_cdc_tx, &mut uart_rx).await;
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
    uart_tx: &mut BufferedUartTx<'static>,
    usb_cdc_rx: &mut cdc_acm::Receiver<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = usb_cdc_rx.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("To UART TX {}: {:x}", n, data);
        if let Err(e) = uart_tx.write_all(data).await {
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
    uart_rx: &mut BufferedUartRx<'static>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 63];
    loop {
        let n = match uart_rx.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                warn!("UART RX err: {:?}", e);
                return Err(Disconnected {});
            }
        };

        let data = &buf[..n];
        info!("To CDC TX {}: {:x}", n, data);
        usb_cdc_tx.write_packet(data).await?;
    }
}
