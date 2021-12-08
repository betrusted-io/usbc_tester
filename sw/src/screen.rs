use utralib::generated::{
    utra, CSR, HW_SCREEN_BASE,
};
use betrusted_hal::hal_time::delay_ms;

/// Flow control timeout limits how long putc() waits to drain a full TX buffer
const FLOW_CONTROL_TIMEOUT_MS: usize = 5;

pub struct Screen {}
impl Screen {
    /// Write to Screen with TX buffer flow control
    pub fn putc(c: u8) {
        let mut uart_csr = CSR::new(HW_SCREEN_BASE as *mut u32);
        // Allow TX buffer to drain if it's full
        // TX buffer is currently 256 bytes (see betrusted-ec/betrusted_ec.py)
        // Baud rate is 115200 with 8N1. So...
        // Time to send one byte: 1000ms / (115200 / 10) = 0.087ms
        // Bytes sent per ms: 1ms / 0.087ms = 11.5
        for _ in 0..FLOW_CONTROL_TIMEOUT_MS {
            if uart_csr.rf(utra::uart::TXFULL_TXFULL) == 1 {
                delay_ms(1);
            } else {
                break;
            }
        }
        // Send a character
        uart_csr.wfo(utra::uart::RXTX_RXTX, c as u32);
    }
}

use core::fmt::{Error, Write};
impl Write for Screen {
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            Self::putc(c);
        }
        Ok(())
    }
}
