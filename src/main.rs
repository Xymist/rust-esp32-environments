use esp_idf_sys::EspError;
use serde::Serialize;
use serde_json::json;
use std::{env, sync::Arc};

use anyhow::*;
use log::*;

use embedded_svc::wifi::*;
use esp_idf_hal::delay;
use esp_idf_hal::gpio::{self, Unknown};
use esp_idf_hal::prelude::*;
use esp_idf_svc::netif::*;
use esp_idf_svc::nvs::*;
use esp_idf_svc::sysloop::*;
use esp_idf_svc::wifi::*;
use esp_idf_hal::i2c;

#[derive(Debug, Serialize)]
struct TsMetric {
    metric: &'static str,
    value: f64,
}

impl TsMetric {
    fn temperature(value: f64) -> Self {
        TsMetric {
            metric: "temperature",
            value,
        }
    }

    fn humidity(value: f64) -> Self {
        TsMetric {
            metric: "humidity",
            value,
        }
    }
}

fn main() -> Result<()> {
    env::set_var("RUST_BACKTRACE", "1");

    let _wifi = wifi()?;

    // This is more complexity than I'd like but if these aren't
    // threaded then this overflows the stack. It's not clear how
    // to adjust the allowed stack size.
    let env_worker = std::thread::spawn(move || take_readings());
    let _ = env_worker.join();

    info!("Going to loop...");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

#[derive(Debug, Default)]
struct Readings {
    low_temperature: Option<f32>,
    high_temperature: Option<f32>,
    humidity: Option<f32>
}

impl Readings {
    fn temperature(&mut self, reading: f32) {
        match (self.low_temperature, self.high_temperature) {
            (Some(_), Some(_)) => return,
            (Some(t), None) => {
                if t >= reading {
                    self.high_temperature = Some(t);
                    self.low_temperature = Some(reading);
                } else {
                    self.high_temperature = Some(reading)
                }
            },
            (None, Some(u)) => {
                if u <= reading {
                    self.low_temperature = Some(u);
                    self.high_temperature = Some(reading);
                } else {
                    self.low_temperature = Some(reading);
                }
            },
            (None, None) => self.low_temperature = Some(reading)
        }
    }

    fn humidity(&mut self, reading: f32) {
        if self.humidity.is_none() {
            self.humidity = Some(reading)
        }
    }
}

fn take_readings() -> Result<()> {
    let peripherals = Peripherals::take().unwrap();
    let pins = peripherals.pins;
    let t_pin: gpio::Gpio18<Unknown> = pins.gpio18;
    let i2c = i2c::Slave::new(
        peripherals.i2c0,
        i2c::Pins {
            sda: pins.gpio22,
            scl: pins.gpio21
        },
        0x76,
        false,
        1024,
        1024

    )?;

    let mut temp_sensors = vec![];

    let mut delay = delay::Ets;
    let mut temp_probes = match one_wire_bus::OneWire::new(t_pin.into_input_output_od()?) {
        Ok(onewire) => onewire,
        Err(e) => {
            warn!("Failed to set up OneWire: {:?}", e);
            return Ok(())
        }
    };

    let mut search_state = None;

    loop {
        if let Ok(Some((device_address, state))) = temp_probes.device_search(search_state.as_ref(), false, &mut delay) {
            search_state = Some(state);
            if device_address.family_code() != ds18b20::FAMILY_CODE {
                continue;
            }
            match ds18b20::Ds18b20::new::<EspError>(device_address) {
                Ok(sensor) => temp_sensors.push(sensor),
                Err(e) => {
                    warn!("Failed to create sensor from device: {:?}", e);
                    continue;
                }
            };
        } else {
            break;
        }
    }

    loop {
        match ds18b20::start_simultaneous_temp_measurement(&mut temp_probes, &mut delay) {
            Ok(()) => (),
            Err(e) => {
                warn!("Failed to start temperature measurement: {:?}", e);
                continue;
            }
        };

        ds18b20::Resolution::Bits12.delay_for_measurement_time(&mut delay);

        let mut res = Readings::default();

        for sensor in &temp_sensors {
            match sensor.read_data(&mut temp_probes, &mut delay){
                Ok(sensor_data) => {
                    res.temperature(sensor_data.temperature);
                },
                Err(e) => {
                    warn!("Failed to read from sensor {:?}: {:?}", sensor.address(), e);
                    continue;
                }
            };

        }

        if let Some(t) = res.low_temperature {
            info!("Low temp is {:?}", t);
            send_request(&TsMetric::temperature(t.into()), "cold_end")?
        };

        if let Some(t) = res.high_temperature {
            info!("High temp is {:?}", t);
            send_request(&TsMetric::temperature(t.into()), "hot_end")?
        };

        if let Some(h) = res.humidity {
            info!("Humidity is {:?}", h);
            send_request(&TsMetric::humidity(h.into()), "central")?
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn send_request(body_data: &TsMetric, source: &str) -> Result<()> {
    let data = json!(body_data).to_string();

    minreq::post("http://victoria:4242/api/put")
        .with_param("extra_label", &format!("source={}", source))
        .with_header("Content-Type", "application/json")
        .with_body(data)
        .send()?;

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
        Status(
            ClientStatus::Started(ClientConnectionStatus::Connected(ClientIpStatus::Done(_))),
            _,
        ) => Ok(wifi),
        failure => bail!("Unexpected Wifi status: {:?}", &failure),
    }
}
