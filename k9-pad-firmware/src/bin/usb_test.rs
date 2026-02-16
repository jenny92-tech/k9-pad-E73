// INPUT:  embassy_nrf(usb), embassy_usb(cdc_acm)
// OUTPUT: Standalone USB CDC ACM test firmware
// POS:    独立二进制，验证 USB CDC 功能（调试用）
#![no_std]
#![no_main]

// Force link nrf-mpsl for critical-section implementation
extern crate nrf_mpsl;

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::usb::Driver;
use embassy_nrf::{bind_interrupts, peripherals, usb};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    nrf_config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);
    info!("=== USB Test (HardwareVbusDetect, HFXO) ===");

    let vbus_detect = HardwareVbusDetect::new(Irqs);
    info!("HardwareVbusDetect: using POWER USBREGSTATUS for VBUS detection");

    let driver = Driver::new(p.USBD, Irqs, vbus_detect);
    info!("USB driver created");

    let mut config = embassy_usb::Config::new(0x1209, 0x0001);
    config.manufacturer = Some("K9Test");
    config.product = Some("USB Test Device");
    config.serial_number = Some("00001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );

    let _class = CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64);
    let mut usb = builder.build();

    info!("USB built, calling run() - waiting for VBUS + USBREG ready...");
    usb.run().await;
}
