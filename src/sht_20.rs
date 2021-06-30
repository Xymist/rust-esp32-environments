use std::f32;

use crate::bail;
use esp_idf_hal::{delay::Ets, gpio, i2c::{self, *}, prelude::*};
use esp_idf_sys::EspError;
use log::info;

const ERROR_I2C_TIMEOUT: u16 = 998;
const ERROR_BAD_CRC: u16 = 999;
const SLAVE_ADDRESS: u8 = 0x40;
const TRIGGER_TEMP_MEASURE_HOLD: u8 = 0xE3;
const TRIGGER_HUMD_MEASURE_HOLD: u8 = 0xE5;
const TRIGGER_TEMP_MEASURE_NOHOLD: u8 = 0xF3;
const TRIGGER_HUMD_MEASURE_NOHOLD: u8 = 0xF5;
const WRITE_USER_REG: u8 = 0xE6;
const READ_USER_REG: u8 = 0xE7;
const SOFT_RESET: u8 = 0xFE;
const USER_REGISTER_RESOLUTION_MASK: u8 = 0x81;
const USER_REGISTER_RESOLUTION_RH12_TEMP14: u8 = 0x00;
const USER_REGISTER_RESOLUTION_RH8_TEMP12: u8 = 0x01;
const USER_REGISTER_RESOLUTION_RH10_TEMP13: u8 = 0x80;
const USER_REGISTER_RESOLUTION_RH11_TEMP11: u8 = 0x81;
const USER_REGISTER_END_OF_BATTERY: u8 = 0x40;
const USER_REGISTER_HEATER_ENABLED: u8 = 0x04;
const USER_REGISTER_DISABLE_OTP_RELOAD: u8 = 0x02;
const MAX_WAIT: u64 = 100;
const DELAY_INTERVAL: u64 = 10;
const SHIFTED_DIVISOR: u32 = 0x988000;
const MAX_COUNTER: u64 = MAX_WAIT / DELAY_INTERVAL;

pub struct SHT20<I2C, SDA, SCL>
where
    I2C: I2c,
    SDA: gpio::InputPin + gpio::OutputPin,
    SCL: gpio::InputPin + gpio::OutputPin,
{
    dev: Master<I2C, SDA, SCL>,
}

impl<I2C, SDA, SCL> SHT20<I2C, SDA, SCL>
where
    I2C: I2c,
    SDA: gpio::InputPin + gpio::OutputPin,
    SCL: gpio::InputPin + gpio::OutputPin,
{
    pub fn new(i2c: I2C, sda: SDA, scl: SCL) -> Result<Self, EspError> {
        let dev = Master::new(i2c, i2c::Pins { sda, scl }, 400_000)?;

        Ok(SHT20 { dev })
    }

    pub fn read_value(&mut self, cmd: u8) -> anyhow::Result<u16> {
        info!("Writing command {} to device at {}", cmd, SLAVE_ADDRESS);
        self.dev.write(SLAVE_ADDRESS, &[cmd])?;

        let mut buf: [u8; 3] = [0; 3];
        let mut counter: u64 = 0;

        info!("Reading into buffer");
        while counter < MAX_COUNTER && buf == [0; 3] {
            Ets::delay_ms(&mut Ets, DELAY_INTERVAL as u32);
            self.dev.read(SLAVE_ADDRESS, &mut buf)?;
            counter += 1
        }
        info!("Read {} bytes", buf.len());

        if counter == MAX_COUNTER {
            bail!("Timed out reading value waiting for I2C");
        }

        let msb: u8 = buf[0];
        let lsb: u8 = buf[1];
        let checksum: u8 = buf[2];

        let raw_value: u16 = ((msb as u16) << 8) | (lsb as u16);

        info!("Validating checksum");
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

    pub fn humidity(&mut self) -> anyhow::Result<f32> {
        let raw_humidity = self.read_value(TRIGGER_HUMD_MEASURE_NOHOLD)?;
        Ok((raw_humidity as f32 * (125.0 / 65536.0)) - 6.0)
    }

    pub fn temperature(&mut self) -> anyhow::Result<f32> {
        let raw_temperature = self.read_value(TRIGGER_TEMP_MEASURE_NOHOLD)?;
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
        self.dev
            .write_read(SLAVE_ADDRESS, &[READ_USER_REG], &mut buffer)?;
        Ok(buffer[0])
    }

    pub fn write_user_register(&mut self, val: u8) -> anyhow::Result<()> {
        self.dev.write(SLAVE_ADDRESS, &[WRITE_USER_REG, val])?;
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
