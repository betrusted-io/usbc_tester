use utralib::generated::{
    utra, CSR, HW_DUT_BASE,
};

pub const UPPER_PINS: [utralib::Field; 4] = [
    utra::dut::DUT_GND_EX,
    utra::dut::DUT_VBUS_EX,
    utra::dut::DUT_D2_P_A6,
    utra::dut::DUT_D2_N_A7,
];
pub const LOWER_PINS: [utralib::Field; 12] = [
    utra::dut::DUT_GND_B12,
    utra::dut::DUT_VBUS_B9,
    utra::dut::DUT_CC2_B5,
    utra::dut::DUT_GND_B1,
    utra::dut::DUT_GND_A1,
    utra::dut::DUT_VBUS_A4,
    utra::dut::DUT_CC1_A5,
    utra::dut::DUT_VBUS_A9,
    utra::dut::DUT_D1_P_A6,
    utra::dut::DUT_D1_N_A7,
    utra::dut::DUT_GND_A12,
    utra::dut::DUT_VBUS_B4,
];
// the index in this map corresponds to the ADC channel for a given DUT enable
pub const CHANNEL_MAP: [utralib::Field; 16] = [
    utra::dut::DUT_D1_P_A6,
    utra::dut::DUT_D1_N_A7,
    utra::dut::DUT_GND_A12,
    utra::dut::DUT_VBUS_B4,
    utra::dut::DUT_GND_B12,
    utra::dut::DUT_VBUS_B9,
    utra::dut::DUT_CC2_B5,
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
    csr: CSR,
}

impl Adc {
    pub fn new() -> Adc {
        let mut csr = CSR::new(HW_DUT_BASE as *mut u32);
        Adc {
            csr,
        }
    }
}