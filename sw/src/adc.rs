use utralib::generated::{
    utra, CSR, HW_DUT_BASE,
};

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
    utra::dut::DUT_GND_EX,
    utra::dut::DUT_GND_A1,
    utra::dut::DUT_VBUS_A9,
    utra::dut::DUT_VBUS_A4,
    utra::dut::DUT_CC1_A5,
];
pub struct Adc {
    csr: CSR::<u32>,
}

impl Adc {
    pub fn new() -> Adc {
        let csr = CSR::new(HW_DUT_BASE as *mut u32);
        Adc {
            csr,
        }
    }
    /// given an ADC channel, return the code on the channel
    pub fn read(&mut self, channel: utralib::Field) -> Option<u16> {
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
        self.csr.wo(utra::adc::CONTROL,
            self.csr.ms(utra::adc::CONTROL_CHANNEL, ch as u32) |
            self.csr.ms(utra::adc::CONTROL_GO, 1)
        );
        while self.csr.rf(utra::adc::RESULT_RUNNING) == 1 {
            // busy wait
        }
        Some(self.csr.rf(utra::adc::RESULT_DATA) as u16)
    }
}