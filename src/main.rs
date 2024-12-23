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
    let mut util_tx_payload_rx = board.util_tx_payload_rx;
    let pc_to_payload_fut = async {
        loop {
            usb_cdc_rx.wait_connection().await;
            info!("CDC RX Connected");
            let _ =
                pc_to_payload(&mut util_tx_payload_rx, &mut usb_cdc_rx).await;
            info!("CDC RX Disconnected");
        }
    };

    let mut usb_cdc_tx = board.usb_cdc_tx;
    let mut util_rx_payload_tx = board.util_rx_payload_tx;
    let payload_to_pc_fut = async {
        loop {
            usb_cdc_tx.wait_connection().await;
            info!("CDC TX Connected");
            let _ =
                payload_to_pc(&mut usb_cdc_tx, &mut util_rx_payload_tx).await;
            info!("CDC TX Disconnected");
        }
    };

    join3(usb_fut, pc_to_payload_fut, payload_to_pc_fut).await;
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

async fn pc_to_payload(
    util_tx_payload_rx: &mut BufferedUartTx<'static>,
    usb_cdc_rx: &mut cdc_acm::Receiver<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = usb_cdc_rx.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("To Payload {}: {:x}", n, data);
        match util_tx_payload_rx.write(data).await {
            Ok(_n) => {}
            Err(e) => {
                warn!("UART TX err: {:?}", e);
                return Err(Disconnected {});
            }
        }
    }
}

async fn payload_to_pc(
    usb_cdc_tx: &mut cdc_acm::Sender<
        'static,
        usb::Driver<'static, peripherals::USB>,
    >,
    util_rx_payload_tx: &mut BufferedUartRx<'static>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = match util_rx_payload_tx.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                warn!("UART RX err: {:?}", e);
                return Err(Disconnected {});
            }
        };

        let data = &buf[..n];
        info!("From Payload {}: {:x}", n, data);
        usb_cdc_tx.write_packet(&mut buf).await?;
    }
}
