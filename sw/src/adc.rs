use utralib::generated::{
    utra, CSR, HW_DUT_BASE, HW_ADC_BASE,
};
use betrusted_hal::hal_time::delay_ms;

use debug;
use debug::{loghexln, LL};
use crate::LOG_LEVEL;

// the index in this map corresponds to the ADC channel for a given DUT enable
pub const CHANNEL_MAP: [utralib::Field; 16] = [
    utra::dut::DUT_D1_P_A6,
    utra::dut::DUT_D1_N_A7,
    utra::dut::DUT_GND_A12,
    utra::dut::DUT_VBUS_B4,
    utra::dut::DUT_GND_B12,
    utra::dut::DUT_CC2_B5,
    utra::dut::DUT_VBUS_B9,
    utra::dut::DUT_GND_B1,
    utra::dut::DUT_D2_N_A7, // actually B7 (on upper/reverse conn)
    utra::dut::DUT_D2_P_A6, // actually B6 (on upper/reverse conn)
    utra::dut::DUT_VBUS_EX,
    utra::dut::DUT_IBUS,
    utra::dut::DUT_GND_A1,
    utra::dut::DUT_VBUS_A9,
    utra::dut::DUT_VBUS_A4,
    utra::dut::DUT_CC1_A5,
];
pub struct Adc {
    adc: CSR::<u32>,
    dut: CSR::<u32>,
}

impl Adc {
    pub fn new() -> Adc {
        let adc = CSR::new(HW_ADC_BASE as *mut u32);
        let mut dut = CSR::new(HW_DUT_BASE as *mut u32);
        // set the mux to 0
        dut.wo(utra::dut::DUT, 0);
        Adc {
            adc,
            dut,
        }
    }
    pub fn read_inner(&mut self, ch: u32) -> u16 {
        const AVERAGING: u32 = 16;
        // first one specifies the channel
        let mut cum = 0;
        self.adc.wo(utra::adc::CONTROL,
            self.adc.ms(utra::adc::CONTROL_CHANNEL, ch as u32) |
            self.adc.ms(utra::adc::CONTROL_GO, 1)
        );
        while self.adc.rf(utra::adc::RESULT_DONE) == 0 {
            // busy wait
        }
        // now average over 16 samples
        for _ in 0..AVERAGING {
            self.adc.wo(utra::adc::CONTROL,
                self.adc.ms(utra::adc::CONTROL_CHANNEL, ch as u32) |
                self.adc.ms(utra::adc::CONTROL_GO, 1)
            );
            while self.adc.rf(utra::adc::RESULT_DONE) == 0 {
                // busy wait
            }
            cum += self.adc.rf(utra::adc::RESULT_DATA);
        }
        (cum / AVERAGING) as u16
    }
    /// given an ADC channel, return the delta of the reading versus the calibration
    pub fn read(&mut self, channel: utralib::Field) -> Option<u16> {
        // convert the GPIO field selector into an ADC channel number
        let mut ch = 16;
        for (index, &field) in CHANNEL_MAP.iter().enumerate() {
            if field == channel {
                ch = index;
                break;
            }
        }
        // the channel map field was invalid
        if ch == 16 {
            return None
        }

        // get the ibus cal value; no measurement values should be muxed at this point
        // set the mux to 0
        self.dut.wo(utra::dut::DUT, 0);
        delay_ms(2);
        let cal = self.read_inner(3 + 8);

        // mux in the DUT measurement channel
        self.dut.wfo(channel, 1);
        delay_ms(2);
        let meas = self.read_inner(ch as u32);
        // set the mux to 0
        self.dut.wo(utra::dut::DUT, 0);

        loghexln!(LL::Trace, " cal: ", cal);
        loghexln!(LL::Trace, "meas: ", meas);
        loghexln!(LL::Trace, "  ch: ", ch);

        if meas >= cal {
            Some(0)
        } else {
            Some(cal - meas)
        }
    }
}