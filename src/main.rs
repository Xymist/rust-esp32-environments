use std::thread;
use std::{env, sync::Arc, time::*};

use anyhow::*;
use log::*;

use embedded_svc::anyerror::*;
use embedded_svc::httpd::registry::*;
use embedded_svc::httpd::*;
use embedded_svc::ping::Ping;
use embedded_svc::wifi::*;

use esp_idf_svc::httpd as idf;
use esp_idf_svc::netif::*;
use esp_idf_svc::nvs::*;
use esp_idf_svc::ping;
use esp_idf_svc::sysloop::*;
use esp_idf_svc::wifi::*;

use esp_idf_hal::delay;
use esp_idf_hal::gpio;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi;

fn main() -> Result<()> {
    env::set_var("RUST_BACKTRACE", "1");

    let peripherals = Peripherals::take().unwrap();
    let pins = peripherals.pins;

    threads_playground();

    let _wifi = wifi()?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn wifi() -> Result<EspWifi> {
    let mut wifi = EspWifi::new(
        Arc::new(EspNetif::new()?),
        Arc::new(EspSysLoop::new()?),
        Arc::new(EspDefaultNvs::new()?),
    )?;

    info!("Wifi created");

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: "VM1524709".into(),
        password: "fn6wbLxdFqmh".into(),
        ..Default::default()
    }))?;

    info!("Wifi configuration set, about to get status");

    let status = wifi.get_status();

    if let Status(
        ClientStatus::Started(ClientConnectionStatus::Connected(ClientIpStatus::Done(ip_settings))),
        _,
    ) = status
    {
        info!("Wifi connected, about to do some pings");

        let ping_summary = ping::EspPing::default()
            .ping_summary(ip_settings.subnet.gateway, &Default::default())?;
        if ping_summary.transmitted != ping_summary.received {
            bail!(
                "Pinging gateway {} resulted in timeouts",
                ip_settings.subnet.gateway
            );
        }

        info!("Pinging done");
    } else {
        bail!("Unexpected Wifi status: {:?}", &status);
    }

    Ok(wifi)
}
