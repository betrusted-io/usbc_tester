use utralib::generated::{
    utra, CSR, HW_SBLED_BASE,
};

use crate::CONFIG_CLOCK_FREQUENCY;

const LEDDCR0:  u32 = 0b1000;
const LEDDBR:   u32 = 0b1001;
const LEDDONR:  u32 = 0b1010;
const LEDDOFR:  u32 = 0b1011;
const LEDDBCRR: u32 = 0b0101;
const LEDDBCFR: u32 = 0b0110;
const LEDDPWRR_WHT: u32 = 0b0001;
const LEDDPWRG_RED: u32 = 0b0010;
const LEDDPWRB_GRN: u32 = 0b0011;

pub struct SbLed {
    csr: CSR::<u32>,
}
impl SbLed {
    /// creates the LED driver and starts the breathing on the white LED
    pub fn new() -> Self {
        let mut csr = CSR::new(HW_SBLED_BASE as *mut u32);
        // power up the LED block
        csr.wo(
            utra::sbled::CTRL,
            csr.ms(utra::sbled::CTRL_CURREN, 1) |
            csr.ms(utra::sbled::CTRL_RGBLEDEN, 1) |
            csr.ms(utra::sbled::CTRL_EXE, 1)
        );

        // setup the PWM IP
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDCR0);
        csr.wfo(utra::sbled::DAT_DAT, 0b1111_0100);

        // configure the prescaler
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDBR);
        csr.wfo(utra::sbled::DAT_DAT, (CONFIG_CLOCK_FREQUENCY / 64000) - 1);

        // on time to 0.25 second
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDONR);
        csr.wfo(utra::sbled::DAT_DAT, 0b0010_1000);

        // off time to 0.25 second
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDOFR);
        csr.wfo(utra::sbled::DAT_DAT, 0b0010_1000);

        // LED breathe ON, 0.768 ramp time
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDBCRR);
        csr.wfo(utra::sbled::DAT_DAT, 0b1010_0101);

        // LED breathe ON, 0.768 ramp time
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDBCFR);
        csr.wfo(utra::sbled::DAT_DAT, 0b1010_0101);

        // turn on the white LED (mapped to the "red" register)
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRR_WHT);
        csr.wfo(utra::sbled::DAT_DAT, 0xff);
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRG_RED);
        csr.wfo(utra::sbled::DAT_DAT, 0);
        csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRB_GRN);
        csr.wfo(utra::sbled::DAT_DAT, 0);

        SbLed {
            csr,
        }
    }

    pub fn idle(&mut self) {
        // restore the "breathe" mode function
        self.csr.wo(
            utra::sbled::CTRL,
            self.csr.ms(utra::sbled::CTRL_CURREN, 1) |
            self.csr.ms(utra::sbled::CTRL_RGBLEDEN, 1) |
            self.csr.ms(utra::sbled::CTRL_EXE, 1)
        );

        // on time to 0.25 second
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDONR);
        self.csr.wfo(utra::sbled::DAT_DAT, 0b0010_1000);

        // off time to 0.25 second
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDONR);
        self.csr.wfo(utra::sbled::DAT_DAT, 0b0010_1000);

        // LED breathe ON, 0.768 ramp time
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDBCRR);
        self.csr.wfo(utra::sbled::DAT_DAT, 0b1010_0101);

        // LED breathe ON, 0.768 ramp time
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDBCFR);
        self.csr.wfo(utra::sbled::DAT_DAT, 0b1010_0101);

        // turn on the white LED (mapped to the "red" register)
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRR_WHT);
        self.csr.wfo(utra::sbled::DAT_DAT, 0xff);
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRG_RED);
        self.csr.wfo(utra::sbled::DAT_DAT, 0);
        self.csr.wfo(utra::sbled::ADDR_ADDR, LEDDPWRB_GRN);
        self.csr.wfo(utra::sbled::DAT_DAT, 0);
    }

    pub fn pass(&mut self) {
        // directly drive the LEDs
        self.csr.wo(
            utra::sbled::CTRL,
            self.csr.ms(utra::sbled::CTRL_CURREN, 1) |
            self.csr.ms(utra::sbled::CTRL_RGBLEDEN, 1) |
            self.csr.ms(utra::sbled::CTRL_EXE, 1) |
            self.csr.ms(utra::sbled::CTRL_RRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_GRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_BRAW, 1)
        );
        // light up the green LED only
        self.csr.wo(
            utra::sbled::RAW,
            self.csr.ms(utra::sbled::RAW_R, 0) |  // white
            self.csr.ms(utra::sbled::RAW_G, 0) |  // red
            self.csr.ms(utra::sbled::RAW_B, 1)    // green
        );
    }

    pub fn fail(&mut self) {
        // directly drive the LEDs
        self.csr.wo(
            utra::sbled::CTRL,
            self.csr.ms(utra::sbled::CTRL_CURREN, 1) |
            self.csr.ms(utra::sbled::CTRL_RGBLEDEN, 1) |
            self.csr.ms(utra::sbled::CTRL_EXE, 1) |
            self.csr.ms(utra::sbled::CTRL_RRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_GRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_BRAW, 1)
        );
        // light up the green LED only
        self.csr.wo(
            utra::sbled::RAW,
            self.csr.ms(utra::sbled::RAW_R, 0) |  // white
            self.csr.ms(utra::sbled::RAW_G, 1) |  // red
            self.csr.ms(utra::sbled::RAW_B, 0)    // green
        );
    }

    pub fn run(&mut self) {
        // directly drive the LEDs
        self.csr.wo(
            utra::sbled::CTRL,
            self.csr.ms(utra::sbled::CTRL_CURREN, 1) |
            self.csr.ms(utra::sbled::CTRL_RGBLEDEN, 1) |
            self.csr.ms(utra::sbled::CTRL_EXE, 1) |
            self.csr.ms(utra::sbled::CTRL_RRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_GRAW, 1) |
            self.csr.ms(utra::sbled::CTRL_BRAW, 1)
        );
        // light up the green LED only
        self.csr.wo(
            utra::sbled::RAW,
            self.csr.ms(utra::sbled::RAW_R, 1) |  // white
            self.csr.ms(utra::sbled::RAW_G, 0) |  // red
            self.csr.ms(utra::sbled::RAW_B, 0)    // green
        );
    }
}