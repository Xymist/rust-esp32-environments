use std::f32;

use crate::bail;
use esp_idf_hal::{
    delay::Ets,
    gpio,
    i2c::{self, *},
    prelude::*,
};
use esp_idf_sys::EspError;
use log::{debug, info};

const SLAVE_ADDRESS: u8 = 0x40;
const TRIGGER_TEMP_MEASURE_HOLD: u8 = 0xE3;
const TRIGGER_HUMD_MEASURE_HOLD: u8 = 0xE5;
const TRIGGER_TEMP_MEASURE_NOHOLD: u8 = 0xF3;
const TRIGGER_HUMD_MEASURE_NOHOLD: u8 = 0xF5;
const WRITE_USER_REG: u8 = 0xE6;
const READ_USER_REG: u8 = 0xE7;
// const SOFT_RESET: u8 = 0xFE;
// const USER_REGISTER_RESOLUTION_MASK: u8 = 0x81;
// const USER_REGISTER_RESOLUTION_RH12_TEMP14: u8 = 0x00;
// const USER_REGISTER_RESOLUTION_RH8_TEMP12: u8 = 0x01;
// const USER_REGISTER_RESOLUTION_RH10_TEMP13: u8 = 0x80;
// const USER_REGISTER_RESOLUTION_RH11_TEMP11: u8 = 0x81;
const USER_REGISTER_END_OF_BATTERY: u8 = 0x40;
const USER_REGISTER_HEATER_ENABLED: u8 = 0x04;
const USER_REGISTER_DISABLE_OTP_RELOAD: u8 = 0x02;
const READ_DELAY_INTERVAL: u64 = 100; //ms
const SHIFTED_DIVISOR: u32 = 0x988000;

pub struct SHT20<I2C, SDA, SCL>
where
    I2C: I2c,
    SDA: gpio::InputPin + gpio::OutputPin,
    SCL: gpio::InputPin + gpio::OutputPin,
{
    i2c_bus: Master<I2C, SDA, SCL>,
}

impl<I2C, SDA, SCL> SHT20<I2C, SDA, SCL>
where
    I2C: I2c,
    SDA: gpio::InputPin + gpio::OutputPin,
    SCL: gpio::InputPin + gpio::OutputPin,
{
    pub fn new(i2c: I2C, sda: SDA, scl: SCL) -> Result<Self, EspError> {
        let i2c_bus = Master::new(
            i2c,
            i2c::Pins { sda, scl },
            config::MasterConfig::default().baudrate(100_000.into()),
        )?;

        Ok(SHT20 { i2c_bus })
    }

    pub fn read_value(&mut self, cmd: u8) -> anyhow::Result<u16> {
        debug!("Writing command {} to device at {}", cmd, SLAVE_ADDRESS);
        self.i2c_bus.write(SLAVE_ADDRESS, &[cmd])?;

        debug!("Waiting for measurement");

        let mut delay = Ets;
        delay.delay_ms(READ_DELAY_INTERVAL as u32);

        debug!("Reading into buffer");
        let mut buf: [u8; 3] = [0; 3];
        self.i2c_bus.read(SLAVE_ADDRESS, &mut buf)?;

        let msb: u8 = buf[0];
        let lsb: u8 = buf[1];
        let checksum: u8 = buf[2];

        let raw_value: u16 = ((msb as u16) << 8) | (lsb as u16);

        debug!("Validating checksum");
        Self::check_crc(raw_value, checksum)?;

        Ok(raw_value & 0xFFFC)
    }

    pub fn check_crc(raw_value: u16, checksum: u8) -> anyhow::Result<()> {
        let mut remainder = (raw_value as u32) << 8;
        remainder |= checksum as u32;
        let mut divisor = SHIFTED_DIVISOR;

        for i in 0..16u32 {
            if (remainder & 1u32 << (23 - i)) != 0 {
                remainder ^= divisor;
            }
            divisor >>= 1;
        }

        match remainder {
            0 => Ok(()),
            _ => bail!("Incorrect checksum when receiving bytes from sensor"),
        }
    }

    pub fn humidity(&mut self, hold: bool) -> anyhow::Result<f32> {
        let cmd = if hold {
            TRIGGER_HUMD_MEASURE_HOLD
        } else {
            TRIGGER_HUMD_MEASURE_NOHOLD
        };
        let raw_humidity = self.read_value(cmd)?;
        Ok((raw_humidity as f32 * (125.0 / 65536.0)) - 6.0)
    }

    pub fn temperature(&mut self, hold: bool) -> anyhow::Result<f32> {
        let cmd = if hold {
            TRIGGER_TEMP_MEASURE_HOLD
        } else {
            TRIGGER_TEMP_MEASURE_NOHOLD
        };
        let raw_temperature = self.read_value(cmd)?;
        Ok((raw_temperature as f32 * (175.72 / 65536.0)) - 46.85)
    }

    pub fn set_resolution(&mut self, mut resolution: u8) -> anyhow::Result<()> {
        let mut user_register = self.read_user_register()?;
        user_register &= 0b01111110;
        resolution &= 0b10000001;
        user_register |= resolution;
        self.write_user_register(user_register)?;
        Ok(())
    }

    pub fn read_user_register(&mut self) -> anyhow::Result<u8> {
        let mut buffer = [0; 1];
        self.i2c_bus
            .write_read(SLAVE_ADDRESS, &[READ_USER_REG], &mut buffer)?;
        Ok(buffer[0])
    }

    pub fn write_user_register(&mut self, val: u8) -> anyhow::Result<()> {
        self.i2c_bus.write(SLAVE_ADDRESS, &[WRITE_USER_REG, val])?;
        Ok(())
    }

    pub fn check_sht20(&mut self) -> anyhow::Result<()> {
        info!("Checking SHT20 Status");
        let reg = self.read_user_register()?;
        info!(
            "End of battery: {}",
            (reg & USER_REGISTER_END_OF_BATTERY) != 0
        );
        info!(
            "Heater enabled: {}",
            (reg & USER_REGISTER_HEATER_ENABLED) != 0
        );
        info!(
            "Disable OTP reload: {}",
            (reg & USER_REGISTER_DISABLE_OTP_RELOAD) != 0
        );
        Ok(())
    }
}
