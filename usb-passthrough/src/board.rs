#[cfg(feature = "defmt")]
use defmt::assert_eq;

use embassy_stm32::mode::Async;
use embassy_stm32::rcc::Sysclk;
use embassy_stm32::usart::{RingBufferedUartRx, Uart, UartTx};
use embassy_stm32::{bind_interrupts, usart};
use embassy_stm32::{peripherals, usb};
use embassy_usb::class::cdc_acm;
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::{Builder, UsbDevice};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USART3_4_5_6_LPUART1 => usart::InterruptHandler<peripherals::USART4>;
    USB_UCPD1_2 => usb::InterruptHandler<peripherals::USB>;
});

pub struct Board {
    pub usb: UsbDevice<'static, usb::Driver<'static, peripherals::USB>>,
    pub usb_cdc_tx:
        cdc_acm::Sender<'static, usb::Driver<'static, peripherals::USB>>,
    pub usb_cdc_rx:
        cdc_acm::Receiver<'static, usb::Driver<'static, peripherals::USB>>,
    pub uart_tx: UartTx<'static, Async>,
    pub uart_rx: RingBufferedUartRx<'static>,
}

impl Board {
    pub fn new() -> Self {
        let p = {
            let mut config = embassy_stm32::Config::default();
            let pll_config = embassy_stm32::rcc::Pll {
                source: embassy_stm32::rcc::PllSource::HSI, // HSI (16MHz)
                prediv: embassy_stm32::rcc::PllPreDiv::DIV1, // 16 MHz
                mul: embassy_stm32::rcc::PllMul::MUL8,      // 128 Mhz
                divr: Some(embassy_stm32::rcc::PllRDiv::DIV2), // 64 MHz
                divq: Some(embassy_stm32::rcc::PllQDiv::DIV2), // 64 MHz
                divp: Some(embassy_stm32::rcc::PllPDiv::DIV2), // 64 MHz
            };
            config.rcc.pll = Some(pll_config);
            config.rcc.sys = Sysclk::PLL1_R;
            embassy_stm32::init(config)
        };

        let (usb, usb_cdc_tx, usb_cdc_rx) = {
            let driver = usb::Driver::new(p.USB, Irqs, p.PA12, p.PA11);

            // Generic VID and PID for development
            let config = embassy_usb::Config::new(0xc0de, 0xcafe);
            let max_packet_size = config.max_packet_size_0 as u16;
            #[cfg(feature = "defmt")]
            assert_eq!(max_packet_size, 64);

            static CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
            let config_descriptor = CONFIG_BUF.init([0u8; 256]);
            static BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
            let bos_descriptor = BOS_BUF.init([0u8; 256]);
            static CONTROL_BUF: StaticCell<[u8; 7]> = StaticCell::new();
            let control_buf = CONTROL_BUF.init([0u8; 7]);
            static CDC_STATE: StaticCell<cdc_acm::State> = StaticCell::new();
            let state = CDC_STATE.init(cdc_acm::State::new());
            let mut builder = Builder::new(
                driver,
                config,
                config_descriptor,
                bos_descriptor,
                &mut [], // no msos descriptors
                control_buf,
            );
            let class = CdcAcmClass::new(&mut builder, state, max_packet_size);
            let (usb_cdc_tx, usb_cdc_rx) = class.split();
            let usb = builder.build();
            (usb, usb_cdc_tx, usb_cdc_rx)
        };

        let (uart_tx, uart_rx) = {
            let mut config = usart::Config::default();
            config.baudrate = 115200;
            config.swap_rx_tx = true;
            let uart4 = {
                Uart::new(
                    p.USART4, p.PA1, p.PA0, Irqs, p.DMA1_CH2, p.DMA1_CH3,
                    config,
                )
                .unwrap()
            };
            let (uart_tx, uart_rx) = uart4.split();
            static RX_BUFF: StaticCell<[u8; 256]> = StaticCell::new();
            let rx_buf = RX_BUFF.init([0u8; 256]);
            let uart_rx = uart_rx.into_ring_buffered(rx_buf);
            (uart_tx, uart_rx)
        };

        Self {
            usb,
            usb_cdc_tx,
            usb_cdc_rx,
            uart_tx,
            uart_rx,
        }
    }
}
