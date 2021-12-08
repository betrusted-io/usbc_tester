#![no_main]
#![no_std]

extern crate utralib;
extern crate volatile;
extern crate xous_nommu;

use core::panic::PanicInfo;
use debug;
use debug::{log, loghex, loghexln, logln, LL};
use riscv_rt::entry;
use utralib::generated::{
    utra, CSR, HW_CRG_BASE, HW_TICKTIMER_BASE, HW_DUT_BASE,
};
use betrusted_hal::hal_time::{
    set_msleep_target_ticks, time_init, TimeMs, delay_ms,
};
use betrusted_hal::mem_locs::*;
use core::fmt::Write;

// Modules from this crate
mod spi;
mod str_buf;
mod uart;
mod screen;
mod sbled;
mod adc;

// Configure Log Level (used in macro expansions)
const LOG_LEVEL: LL = LL::Info;

// Constants
const CONFIG_CLOCK_FREQUENCY: u32 = 18_000_000;

/// Infinite loop panic handler (TODO: fix this to use less power)
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

/// handles just the watchdog for now
fn ticktimer_int_handler(_irq_no: usize) {
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
    let mut crg_csr = CSR::new(HW_CRG_BASE as *mut u32);

    // disarm the watchdog
    crg_csr.wfo(utra::crg::WATCHDOG_RESET_CODE, 0x600d);
    crg_csr.wfo(utra::crg::WATCHDOG_RESET_CODE, 0xc0de);

    set_msleep_target_ticks(50); // resetting this will also clear the alarm

    ticktimer_csr.wfo(utra::ticktimer::EV_PENDING_ALARM, 1);
}

fn stack_check() {
    // check the stack usage
    let stack: &[u32] = unsafe {
        core::slice::from_raw_parts(
            STACK_END as *const u32,
            (STACK_LEN as usize / core::mem::size_of::<u32>()) as usize,
        )
    };
    let mut unused_stack_words = 0;
    for &word in stack.iter() {
        if word != STACK_CANARY {
            break;
        }
        unused_stack_words += 4;
    }
    logln!(
        LL::Debug,
        "{} bytes used of {}",
        STACK_LEN - unused_stack_words,
        STACK_LEN
    );
}

pub const UPPER_PINS: [(utralib::Field, &'static str); 4] = [
    (utra::dut::DUT_GND_EX, "DUT_GND_EX"),
    (utra::dut::DUT_VBUS_EX, "DUT_VBUS_EX"),
    (utra::dut::DUT_D2_P_A6, "D_P: Pin B7"),
    (utra::dut::DUT_D2_N_A7, "D_N: Pin B6"),
];
pub const LOWER_PINS: [(utralib::Field, &'static str); 12] = [
    (utra::dut::DUT_GND_B12, "GND: Pin B12"),
    (utra::dut::DUT_VBUS_B9, "VBUS: Pin B9"),
    (utra::dut::DUT_CC2_B5, "CC2: Pin B5"),
    (utra::dut::DUT_GND_B1, "GND: Pin B1"),
    (utra::dut::DUT_GND_A1, "GND: Pin A1"),
    (utra::dut::DUT_VBUS_A4, "VBUS: Pin A4"),
    (utra::dut::DUT_CC1_A5, "CC1: Pin A5"),
    (utra::dut::DUT_VBUS_A9, "VBUS: Pin A9"),
    (utra::dut::DUT_D1_P_A6, "D_P: Pin A6"),
    (utra::dut::DUT_D1_N_A7, "D_N: Pin A7"),
    (utra::dut::DUT_GND_A12, "GND: Pin A12"),
    (utra::dut::DUT_VBUS_B4, "VBUS: Pin B4"),
];

#[derive(PartialEq, Eq, Clone, Copy)]
enum PinBank {
    Upper,
    Lower
}

/// anything above this number is considered to be an "open" pin
const MIN_NC_THRESH: u16 = 1000;

/// Checks a pin bank.
/// 1. checks to see if any pins are connected. If are connected, return None
/// 2. if any show some kind of connectivity, returns Some([Option<&str>; 12]), where
///    an entry is Some(&str) to represent a failing pin, or None if things pass.
fn check_pins(bank: PinBank) -> [(Option<&'static str>, u16); 12] {
    let mut adc = adc::Adc::new();
    // pick the iterator through the bank descriptor array that matches the requested bank
    let bank_iter = match bank {
        PinBank::Lower => {
            LOWER_PINS.iter()
        }
        PinBank::Upper => {
            UPPER_PINS.iter()
        }
    };
    let mut results: [(Option<&'static str>, u16); 12] = [(None, u16::MAX); 12];
    for (index, &(field, name)) in bank_iter.enumerate() {
        let reading = match adc.read(field) {
            Some(r) => r,
            None => {
                // cheeseball errors because we don't have a panic handler
                logln!(LL::Error, "ADC channel invalid");
                u16::MAX
            }
        };
        results[index] = (Some(name), reading);
    }
    results
}

/// Convenience function that just scans a bank and indicatse if an insertion was detected.
fn check_insert(bank: PinBank) -> bool {
    let result = check_pins(bank);
    for (_name, val) in result {
        if val < MIN_NC_THRESH {
            return true;
        }
    }
    false
}

fn settling_check(bank: PinBank) -> [bool; 12] {
    let result = check_pins(bank);
    let mut ret = [false; 12];
    for (index, &(_name, val)) in result.iter().enumerate() {
        if val < MIN_NC_THRESH {
            ret[index] = true;
        } else {
            ret[index] = false;
        }
    }
    ret
}
fn results_equal(a: [bool; 12], b: [bool; 12]) -> bool {
    for (&x, &y) in a.iter().zip(b.iter()) {
        if x != y {
            return false
        }
    }
    true
}

#[derive(PartialEq, Eq)]
enum TestState {
    WaitInsert,
    Measure,
    ReportResult,
}

#[entry]
fn main() -> ! {
    logln!(LL::Info, "\r\n====UP5K==00");
    let mut crg_csr = CSR::new(HW_CRG_BASE as *mut u32);
    let mut ticktimer_csr = CSR::new(HW_TICKTIMER_BASE as *mut u32);
    let mut uart_state: uart::RxState = uart::RxState::BypassOnAwaitA;

    // Initialize the no-MMU version of 'Xous' (an extremely old branch of it), which will give us
    // basic access to tasks and interrupts.
    logln!(LL::Trace, "pre-nommu");
    xous_nommu::init();

    time_init();
    logln!(LL::Debug, "time");

    let _ = xous_nommu::syscalls::sys_interrupt_claim(
        utra::ticktimer::TICKTIMER_IRQ,
        ticktimer_int_handler,
    );
    set_msleep_target_ticks(50);
    ticktimer_csr.wfo(utra::ticktimer::EV_PENDING_ALARM, 1); // clear the pending signal just in case
    ticktimer_csr.wfo(utra::ticktimer::EV_ENABLE_ALARM, 1); // enable the interrupt

    logln!(LL::Warn, "**WATCHDOG ON**");
    crg_csr.wfo(utra::crg::WATCHDOG_ENABLE, 1); // 1 = enable the watchdog reset

    // Drain the UART RX buffer
    uart::drain_rx_buf();

    let mut sbled = sbled::SbLed::new();
    sbled.idle();
    let mut screen = screen::Screen {};

    write!(screen, "#LCK").unwrap();
    write!(screen, "USB C Test Power On").unwrap();
    write!(screen, "#SYN").unwrap();

    let dut_csr = CSR::new(HW_DUT_BASE as *mut u32);
    //////////////////////// MAIN LOOP ------------------
    logln!(LL::Info, "main loop");
    loop {
        ///////////////////////////// DEBUG UART RX HANDLER BLOCK ----------
        // Uart starts in bypass mode, so this won't start returning bytes
        // until after it sees the "AT\n" wake sequence (or "AT\r")
        let mut show_help = false;
        if let Some(b) = uart::rx_byte(&mut uart_state) {
            match b {
                0x1B => {
                    // In case of ANSI escape sequences (arrow keys, etc.) turn UART bypass mode
                    // on to avoid the hassle of having to parse the escape sequences or deal
                    // with whatever unintended commands they might accidentally trigger
                    uart_state = uart::RxState::BypassOnAwaitA;
                    logln!(LL::Debug, "UartRx off");
                }
                b'h' | b'H' | b'?' => show_help = true,
                b'5' => {
                    let now = TimeMs::now();
                    loghex!(LL::Debug, "NowMs ", now.ms_high_word());
                    loghexln!(LL::Debug, " ", now.ms_low_word());
                }
                b'6' => stack_check(),
                _ => (),
            }
        } else if uart_state == uart::RxState::Waking {
            logln!(LL::Debug, "UartRx on");
            uart_state = uart::RxState::BypassOff;
            show_help = true;
        }
        if show_help {
            log!(
                LL::Debug,
                concat!(
                    "UartRx Help:\r\n",
                    " h => Help\r\n",
                    " 5 => Now ms\r\n",
                    " 6 => Peak stack usage\r\n",
                )
            );
        }
        ///////////////////////////// --------------------------------------
        ///////////////////////////// TEST LOOP ----------------------------
        if dut_csr.rf(utra::dut::RUN_RUN) == 0 { // active low switch hit
            delay_ms(10); // wait for the switch to debounce
            while dut_csr.rf(utra::dut::RUN_RUN) == 0 { // wait for the switch to rise
                delay_ms(10);
            }
            delay_ms(10); // another debounce period
            sbled.run();

            // test at least twice because we need to debounce the insertion
            let mut test_state = TestState::WaitInsert;
            let mut stabilize = 0;
            let mut last_result = [false; 12];
            let mut lower_result: [(Option<&'static str>, u16); 12] = [(None, u16::MAX); 12];
            let mut lower_finished = false;
            let mut upper_result: [(Option<&'static str>, u16); 12] = [(None, u16::MAX); 12];
            let mut upper_finished = false;
            let mut bank = PinBank::Lower;
            let mut counter = 0;
            logln!(LL::Info, "test start");
            loop {
                if dut_csr.rf(utra::dut::RUN_RUN) == 0 { // active low switch hit exits the test
                    delay_ms(10); // wait for the switch to debounce
                    while dut_csr.rf(utra::dut::RUN_RUN) == 0 { // wait for the switch to rise
                        delay_ms(10);
                    }
                    delay_ms(10);
                    sbled.idle();
                    logln!(LL::Info, "test exit");
                    break; // exit the loop
                }
                match test_state {
                    TestState::WaitInsert => {
                        counter = 0;
                        stabilize = 0;
                        if upper_finished && lower_finished {
                            logln!(LL::Info, "show result");
                            test_state = TestState::ReportResult;
                            continue;
                        }
                        write!(screen, "    *Test running*\n\r").unwrap();
                        if !upper_finished {
                            write!(screen, "INSERT UPPER\n\r").unwrap();
                        } else {
                            write!(screen, "Upper measured.\n\r").unwrap();
                        }
                        write!(screen, " \n\r").unwrap();
                        if !lower_finished {
                            write!(screen, "INSERT LOWER\n\r").unwrap();
                        } else {
                            write!(screen, "Lower measured.\n\r").unwrap();
                        }
                        write!(screen, " \n\r").unwrap();
                        write!(screen, "#SYN").unwrap();
                        if !lower_finished && check_insert(PinBank::Lower) {
                            logln!(LL::Info, "measure lower");
                            test_state = TestState::Measure;
                            bank = PinBank::Lower;
                            continue;
                        }
                        if !upper_finished && check_insert(PinBank::Upper) {
                            logln!(LL::Info, "measure upper");
                            test_state = TestState::Measure;
                            bank = PinBank::Upper;
                            continue;
                        }
                    }
                    TestState::Measure => {
                        if bank == PinBank::Lower {
                            write!(screen, "Measuring lower...\n\r").unwrap();
                        } else {
                            write!(screen, "Measuring upper...\n\r").unwrap();
                        }
                        write!(screen, " \n\r").unwrap();
                        match counter % 4 {
                            0 => write!(screen, " |  |  |  | \n\r").unwrap(),
                            1 => write!(screen, " /  /  /  / \n\r").unwrap(),
                            2 => write!(screen, " -  -  -  - \n\r").unwrap(),
                            _ => write!(screen, " \\  \\  \\  \\ \n\r").unwrap(),
                        };
                        write!(screen, " \n\r").unwrap();
                        write!(screen, " \n\r").unwrap();
                        write!(screen, "#SYN").unwrap();
                        counter += 1;
                        let new_result = settling_check(PinBank::Lower);
                        if results_equal(new_result, last_result) {
                            stabilize += 1;
                        } else {
                            stabilize = 0;
                        }
                        for (&src, dst) in new_result.iter().zip(last_result.iter_mut()) {*dst = src;}
                        if stabilize == 4 {
                            if bank == PinBank::Lower {
                                lower_result = check_pins(PinBank::Lower);
                                lower_finished = true;
                                test_state = TestState::WaitInsert;
                            } else {
                                upper_result = check_pins(PinBank::Upper);
                                upper_finished = true;
                                test_state = TestState::WaitInsert;
                            }
                        }
                    }
                    TestState::ReportResult => {
                        let mut passing = true;
                        let mut total_fail = 0;
                        for (_name, val) in lower_result {
                            if val > MIN_NC_THRESH {
                                passing = false;
                                total_fail += 1;
                            }
                        }
                        for (_name, val) in upper_result {
                            if val > MIN_NC_THRESH {
                                passing = false;
                                total_fail += 1;
                            }
                        }
                        if passing {
                            sbled.pass();
                            write!(screen, "   PASS PASS PASS\n\r").unwrap();
                            write!(screen, " \n\r").unwrap();
                            write!(screen, "Remove DUT and press\n\r").unwrap();
                            write!(screen, "start to test another.\n\r").unwrap();
                            write!(screen, "   PASS PASS PASS\n\r").unwrap();
                            write!(screen, "#SYN").unwrap();
                        } else {
                            sbled.fail();
                            write!(screen, "!!! FAIL: {} PINS !!!\n\r", total_fail).unwrap();
                            let mut lines = 0;
                            for (maybe_name, val) in lower_result {
                                if let Some(name) = maybe_name {
                                    if val >= MIN_NC_THRESH {
                                        if lines < 5 {
                                            write!(screen, " {}", name).unwrap();
                                            lines += 1;
                                        }
                                    }
                                }
                            }
                            for (maybe_name, val) in upper_result {
                                if let Some(name) = maybe_name {
                                    if val >= MIN_NC_THRESH {
                                        if lines < 5 {
                                            write!(screen, " {}", name).unwrap();
                                            lines += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
