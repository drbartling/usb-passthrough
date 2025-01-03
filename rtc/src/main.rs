#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::rcc::LsConfig;
use embassy_stm32::rtc::{DateTime, Rtc, RtcConfig};
use embassy_stm32::Config;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    // Use external low speed oscillator.  Skipping this will result in the RTC time freezing
    // between power cycles.
    config.rcc.ls = LsConfig::default_lse();
    let p = embassy_stm32::init(config);

    let mut rtc = Rtc::new(p.RTC, RtcConfig::default());
    let now = rtc.now().unwrap();
    let now: chrono::NaiveDateTime = now.into();
    let now = now.and_utc().timestamp();

    let build_time = chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
        chrono::NaiveTime::from_hms_opt(16, 53, 0).unwrap(),
    );
    let build_timestamp = build_time.and_utc().timestamp();

    if now < build_timestamp {
        rtc.set_datetime(build_time.into())
            .expect("datetime not set");
    }

    loop {
        let now: DateTime = rtc.now().unwrap();
        info!("{}:{}:{}", now.hour(), now.minute(), now.second());

        let now = chrono::NaiveDateTime::from(now);
        info!("{}", now.and_utc().timestamp());

        Timer::after_millis(1000).await;
    }
}
