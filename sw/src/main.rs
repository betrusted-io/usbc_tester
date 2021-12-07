#![no_main]
#![no_std]

// note: to get vscode to reload file, do shift-ctrl-p, 'reload window'. developer:Reload window

extern crate utralib;
extern crate volatile;
extern crate xous_nommu;

use core::panic::PanicInfo;
use debug;
use debug::{log, loghex, loghexln, logln, LL};
use riscv_rt::entry;
use utralib::generated::{
    utra, CSR, HW_CRG_BASE, HW_GIT_BASE, HW_TICKTIMER_BASE, HW_DUT_BASE,
};
use betrusted_hal::hal_time::{
    get_time_ms, get_time_ticks, set_msleep_target_ticks, time_init, TimeMs,
};
use volatile::Volatile;
use betrusted_hal::mem_locs::*;

// Modules from this crate
mod spi;
mod str_buf;
mod uart;
use str_buf::StrBuf;
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

#[entry]
fn main() -> ! {
    logln!(LL::Info, "\r\n====UP5K==0E");
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
    let mut adc = adc::Adc::new();

    write!(srceen, "#LCK");
    write!(srceen, "USB C Test Power On");
    write!(srceen, "#SYN");

    let mut dut_csr = CSR::new(HW_DUT_BASE as *mut u32);
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
            // run the test
            write!(srceen, "Test run:\n\r");
            write!(srceen, " \n\r");
            write!(srceen, "INSERT LOWER\n\r");
            write!(srceen, "INSERT UPPER\n\r");
            write!(srceen, "#SYN");
            
        }
    }
}
