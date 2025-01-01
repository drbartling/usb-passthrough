#![no_std]
#![no_main]

// Let's say we want to show system state to the user.  The system state is a small variable,
// something that is atomic to update.  It's not important that we respond to every state change,
// but rather when we want to show the state, we can get an accurate snapshot quickly.

// The simplest way to do that would be to have a mutable static.  It's safe in this context given
// that anything equal to or less than 32 bits allows for atomic read and write with the instruction
// set.  (On other platforms such as 8 or 16 bit micros, this would be unsafe).

// This however creates problems since there's no signaling of when things change, and so we must
// ensure each thread awaits at the appropriate time allowing control to handoff to another thread.

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

static mut PRESS_COUNTER: i32 = 0;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    info!("Reset");

    let led_pin = Output::new(p.PA5, Level::High, Speed::Low);
    let led = Led { led_pin };
    spawner.must_spawn(led_handler(led));

    let button = ExtiInput::new(p.PC13, p.EXTI13, Pull::None);
    spawner.must_spawn(button_handler(button));

    loop {
        Timer::after(Duration::MAX / 2).await;
    }
}

#[embassy_executor::task]
async fn led_handler(mut led: Led) {
    loop {
        Timer::after(Duration::MIN).await;
        let press_counter = unsafe { PRESS_COUNTER };
        if 0 == press_counter % 2 {
            led.off();
        } else {
            led.on();
        }
    }
}

pub struct Led {
    led_pin: Output<'static>,
}

impl Led {
    pub fn on(&mut self) {
        self.led_pin.set_high();
    }
    pub fn off(&mut self) {
        self.led_pin.set_low();
    }
}

#[embassy_executor::task]
async fn button_handler(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_falling_edge().await;
        unsafe { PRESS_COUNTER += 1 };
    }
}
