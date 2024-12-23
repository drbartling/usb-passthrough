use alloc::boxed::Box;
use embassy_stm32::interrupt;
use embassy_stm32::interrupt::InterruptExt;
use embassy_stm32::rcc::Sysclk;
use embassy_stm32::usart::{BufferedUart, BufferedUartRx, BufferedUartTx};
use embassy_stm32::{bind_interrupts, usart};
use embassy_stm32::{peripherals, usb};
use embassy_usb::class::cdc_acm;
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::{Builder, UsbDevice};

bind_interrupts!(struct Irqs {
    USART3_4_5_6_LPUART1 => usart::BufferedInterruptHandler<peripherals::USART4>;
    USB_UCPD1_2 => usb::InterruptHandler<peripherals::USB>;
});

pub struct Board {
    pub usb: UsbDevice<'static, usb::Driver<'static, peripherals::USB>>,
    pub usb_cdc_tx:
        cdc_acm::Sender<'static, usb::Driver<'static, peripherals::USB>>,
    pub usb_cdc_rx:
        cdc_acm::Receiver<'static, usb::Driver<'static, peripherals::USB>>,
    pub util_tx_payload_rx: BufferedUartTx<'static>,
    pub util_rx_payload_tx: BufferedUartRx<'static>,
}

impl Board {
    pub fn new() -> Self {
        let p = {
            let mut config = embassy_stm32::Config::default();
            let pll_config = embassy_stm32::rcc::Pll {
                mul: embassy_stm32::rcc::PllMul::MUL16,
                divp: Some(embassy_stm32::rcc::PllPDiv::DIV2),
                divr: Some(embassy_stm32::rcc::PllRDiv::DIV2),
                divq: Some(embassy_stm32::rcc::PllQDiv::DIV2),
                prediv: embassy_stm32::rcc::PllPreDiv::DIV2,
                source: embassy_stm32::rcc::PllSource::HSI,
            };
            config.rcc.pll = Some(pll_config);
            config.rcc.sys = Sysclk::PLL1_R;
            embassy_stm32::init(config)
        };

        let (usb, usb_cdc_tx, usb_cdc_rx) = {
            let driver = usb::Driver::new(p.USB, Irqs, p.PA12, p.PA11);
            let config = embassy_usb::Config::new(0xc0de, 0xcafe);
            let config_descriptor = Box::leak(Box::new([0u8; 256]));
            let bos_descriptor = Box::leak(Box::new([0u8; 256]));
            let control_buf = Box::leak(Box::new([0u8; 7]));
            let state = Box::leak(Box::new(cdc_acm::State::new()));
            let mut builder = Builder::new(
                driver,
                config,
                config_descriptor,
                bos_descriptor,
                &mut [], // no msos descriptors
                control_buf,
            );
            let class = CdcAcmClass::new(&mut builder, state, 64);
            let (usb_cdc_tx, usb_cdc_rx) = class.split();
            let usb = builder.build();
            (usb, usb_cdc_tx, usb_cdc_rx)
        };

        let (util_tx_payload_rx, util_rx_payload_tx) = {
            let uart_tx_buf = Box::leak(Box::new([0u8; 256]));
            let uart_rx_buf = Box::leak(Box::new([0u8; 256]));

            let mut config = usart::Config::default();
            config.swap_rx_tx = true;
            let uart4 = {
                BufferedUart::new(
                    p.USART4,
                    Irqs,
                    p.PA1,
                    p.PA0,
                    uart_tx_buf,
                    uart_rx_buf,
                    config,
                )
                .unwrap()
            };
            uart4.split()
        };

        // May want to try adjust interrupts?
        InterruptExt::set_priority(
            embassy_stm32::interrupt::USART3_4_5_6_LPUART1,
            interrupt::Priority::P0,
        );
        InterruptExt::set_priority(
            embassy_stm32::interrupt::USB_UCPD1_2,
            interrupt::Priority::P0,
        );

        Self {
            usb,
            usb_cdc_tx,
            usb_cdc_rx,
            util_tx_payload_rx,
            util_rx_payload_tx,
        }
    }
}
