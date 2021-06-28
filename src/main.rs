use std::{env, sync::Arc};
use serde::Serialize;

use anyhow::*;
use log::*;

use embedded_svc::wifi::*;
use esp_idf_svc::netif::*;
use esp_idf_svc::nvs::*;
use esp_idf_svc::sysloop::*;
use esp_idf_svc::wifi::*;
use esp_idf_hal::prelude::*;
use serde_json::json;

#[derive(Debug, Serialize)]
struct TsMetric {
    metric: &'static str,
    value: f64
}

impl TsMetric {
    fn temperature(value: f64) -> Self {
        TsMetric {
            metric: "temperature",
            value
        }
    }

    fn humidity(value: f64) -> Self {
        TsMetric {
            metric: "humidity",
            value
        }
    }
}

fn main() -> Result<()> {
    env::set_var("RUST_BACKTRACE", "1");

    let peripherals = Peripherals::take().unwrap();
    let _pins = peripherals.pins;
    let _wifi = wifi()?;

    send_request(&TsMetric::humidity(100.0))?;
        // send_request(&TsMetric::temperature(40.0))?;
        // send_request(&TsMetric::temperature(20.0))?;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn send_request(body_data: &TsMetric) -> Result<()> {
    info!("Sending packet");

    let data = json!(body_data).to_string();

    minreq::post("http://victoria:4242/api/put")
        .with_param("extra_label", "source=test_bench")
        .with_header("Content-Type", "application/json")
        .with_body(data)
        .send()?;

    info!("Sent!");

    Ok(())
}

fn wifi() -> Result<EspWifi> {
    let mut wifi = EspWifi::new(
        Arc::new(EspNetif::new()?),
        Arc::new(EspSysLoop::new()?),
        Arc::new(EspDefaultNvs::new()?),
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: "VM1524709".into(),
        password: "fn6wbLxdFqmh".into(),
        ..Default::default()
    }))?;

    info!("Wifi configuration set, about to get status");

    match wifi.get_status() {
        Status(ClientStatus::Started(ClientConnectionStatus::Connected(ClientIpStatus::Done(_))), _) => Ok(wifi),
        failure => bail!("Unexpected Wifi status: {:?}", &failure)
    }

}
