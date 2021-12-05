#![no_std]

extern crate bitflags;
extern crate volatile;
extern crate utralib;
extern crate riscv;

pub mod hal_time;
pub mod mem_locs;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
