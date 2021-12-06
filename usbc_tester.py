#!/usr/bin/env python3
# This variable defines all the external programs that this module
# relies on.  lxbuildenv reads this variable in order to ensure
# the build will finish without exiting due to missing third-party
# programs.
LX_DEPENDENCIES = ["riscv", "icestorm", "yosys"]

# Import lxbuildenv to integrate the deps/ directory
import lxbuildenv
import argparse
import os
import subprocess

from migen import *
from migen import Module, Signal, Instance, ClockDomain, If
from migen.genlib.resetsync import AsyncResetSynchronizer
from migen.fhdl.specials import TSTriple
from migen.fhdl.bitcontainer import bits_for
from migen.fhdl.structure import ClockSignal, ResetSignal, Replicate, Cat

from litex.build.lattice.platform import LatticePlatform
from litex.build.sim.platform import SimPlatform
from litex.build.generic_platform import Pins, IOStandard, Misc, Subsignal
from litex.soc.cores import ram
from litex.soc.integration.soc_core import SoCCore
from litex.soc.integration.builder import Builder
from litex.soc.integration.doc import AutoDoc, ModuleDoc
from litex.soc.interconnect import wishbone
from litex.soc.interconnect.csr import *
from litex.soc.interconnect.csr_eventmanager import *
from litex.soc.cores.uart import UARTWishboneBridge
from litex.soc.cores import uart


import litex.soc.doc as lxsocdoc

# Ish. It's actually slightly smaller, but this is divisible by 4096 (erase sector size).
GATEWARE_SIZE = 0x1a000

# 1 MB (8 Mb)
SPI_FLASH_SIZE = 1 * 1024 * 1024

io = [
    ("serial", 0,
     Subsignal("rx", Pins("20")),
     Subsignal("tx", Pins("19")),
     IOStandard("LVCMOS33")
     ),

    ("spiflash", 0,
     Subsignal("cs_n", Pins("16"), IOStandard("LVCMOS33")),
     Subsignal("clk", Pins("15"), IOStandard("LVCMOS33")),
     Subsignal("cipo", Pins("17"), IOStandard("LVCMOS33")),
     Subsignal("copi", Pins("14"), IOStandard("LVCMOS33")),
     Subsignal("wp", Pins("18"), IOStandard("LVCMOS33")),
     Subsignal("hold", Pins("13"), IOStandard("LVCMOS33")),
     ),
    ("spiflash4x", 0,
     Subsignal("cs_n", Pins("16"), IOStandard("LVCMOS33")),
     Subsignal("clk", Pins("15"), IOStandard("LVCMOS33")),
     Subsignal("dq", Pins("14 17 18 13"), IOStandard("LVCMOS33")),
     ),

    ("clk12", 0, Pins("35"), IOStandard("LVCMOS33")),

    ("led", 0,
         Subsignal("rgb0", Pins("39"), IOStandard("LVCMOS33")),
         Subsignal("rgb1", Pins("40"), IOStandard("LVCMOS33")),
         Subsignal("rgb2", Pins("41"), IOStandard("LVCMOS33")),
     ),

    ("adc", 0,
     Subsignal("sclk", Pins("38"), IOStandard("LVCMOS33")),
     Subsignal("cipo", Pins("42"), IOStandard("LVCMOS33")),
     Subsignal("copi", Pins("43"), IOStandard("LVCMOS33")),
     Subsignal("cs0_n", Pins("44"), IOStandard("LVCMOS33")),
     Subsignal("cs1_n", Pins("37"), IOStandard("LVCMOS33")),
     ),

    ("screen", 0, Pins("27"), IOStandard("LVCMOS33")),

    ("run", 0, Pins("28"), IOStandard("LVCMOS33")),

    ("dut", 0,
     Subsignal("d2_p_a6", Pins("32"), IOStandard("LVCMOS33")),
     Subsignal("d2_n_a7", Pins("36"), IOStandard("LVCMOS33")),
     Subsignal("vbus_ex", Pins("34"), IOStandard("LVCMOS33")),
     Subsignal("gnd_ex",  Pins("31"), IOStandard("LVCMOS33")),
     Subsignal("gnd_b12", Pins("47"), IOStandard("LVCMOS33")),
     Subsignal("vbus_b9", Pins("10"), IOStandard("LVCMOS33")),
     Subsignal("cc2_b5",  Pins("9"), IOStandard("LVCMOS33")),
     Subsignal("gnd_b1",  Pins("48"), IOStandard("LVCMOS33")),
     Subsignal("gnd_a1",  Pins("2"), IOStandard("LVCMOS33")),
     Subsignal("vbus_a4", Pins("6"), IOStandard("LVCMOS33")),
     Subsignal("cc1_a5",  Pins("4"), IOStandard("LVCMOS33")),
     Subsignal("vbus_a9", Pins("3"), IOStandard("LVCMOS33")),
     Subsignal("d1_p_a6", Pins("45"), IOStandard("LVCMOS33")),
     Subsignal("d1_n_a7", Pins("12"), IOStandard("LVCMOS33")),
     Subsignal("gnd_a12", Pins("11"), IOStandard("LVCMOS33")),
     Subsignal("vbus_b4", Pins("46"), IOStandard("LVCMOS33")),
     ),
]

clk_options = {
    "stable" : 18e6,
    "faster" : 19.5e6,
    "overclock" : 21e6,
}
clk_freq=clk_options["stable"]


# Dut -------------------------------------------------------------------------------------------

class Dut(Module, AutoDoc, AutoCSR):
    def __init__(self, pads, run_pad):
        self.intro = ModuleDoc("""DUT drivers""")
        sut_names = [
            "d2_p_a6",
            "d2_n_a7",
            "vbus_ex",
            "gnd_ex",
            "gnd_b12",
            "vbus_b9",
            "cc2_b5",
            "gnd_b1",
            "gnd_a1",
            "vbus_a4",
            "cc1_a5",
            "vbus_a9",
            "d1_p_a6",
            "d1_n_a7",
            "gnd_a12",
            "vbus_b4",
        ]
        sut_fields = []
        for sut in sut_names:
            sut_fields.append(
                CSRField(sut, size=1, description="Set to connect {} to test source".format(sut), reset=0)
            )
        self.dut = CSRStorage(len(sut_names), reset="0", name="dut",
            description="Bits that drive the DUT signal switches", fields=sut_fields,
        )
        for sut in sut_names:
            self.comb += getattr(pads, sut).eq(getattr(self.dut.fields, sut))

        self.run = CSRStatus(1, name="run", description="Pulled low when the `run` switch is depressed")
        self.comb += self.run.status.eq(run_pad)

# ADC -------------------------------------------------------------------------------------------
class Adc(Module, AutoDoc, AutoCSR):
    def __init__(self, pads):
        self.intro = ModuleDoc("""ADC interface""")
        self.ctrl = CSRStorage(name="control", fields=[
            CSRField("channel", size=4, description="Select the ADC channel"),
            CSRField("go", size=1, description="Start the conversion", pulse=True),
        ])
        self.result = CSRStatus(name="result", fields=[
            CSRField("data", size=10, description="Result of last conversion"),
            CSRField("running", size=1, description="Conversion is running"),
        ])
        fsm = FSM(reset_state="IDLE")
        self.submodules += fsm
        sclk = Signal()
        cipo = Signal()
        copi = Signal()
        cs_n = Signal(2)
        self.comb += [
            pads.sclk.eq(sclk),
            pads.copi.eq(copi),
            cipo.eq(pads.cipo),
            pads.cs0_n.eq(cs_n[0]),
            pads.cs1_n.eq(cs_n[1])
        ]
        cycle = Signal(4)
        run = Signal()
        self.comb += self.result.fields.running.eq(run | self.ctrl.fields.go)
        fsm.act("IDLE",
            NextValue(sclk, 1),
            NextValue(copi, 0),
            NextValue(cycle, 0),
            If(self.ctrl.fields.go,
                NextValue(run, 1),
                NextState("PHASE0"),
                If(self.ctrl.fields.channel[3] == 0,
                    NextValue(cs_n, 0b10)
                ).Else(
                    NextValue(cs_n, 0b01)
                )
            ).Else(
                NextValue(run, 0),
                NextValue(cs_n, 0b11),
            )
        )
        fsm.act("PHASE0",
            NextValue(sclk, 0),
            NextState("PHASE1")
        ),
        fsm.act("PHASE1",
            If(cycle == 2,
                NextValue(copi, self.ctrl.fields.channel[2])
            ).Elif(cycle == 3,
                NextValue(copi, self.ctrl.fields.channel[1])
            ).Else(
                NextValue(copi, self.ctrl.fields.channel[0])
            ),
            NextState("PHASE2")
        ),
        fsm.act("PHASE2",
            NextValue(sclk, 1),
            If((cycle >= 4) & (cycle <= 13),
                NextValue(self.result.fields.data, Cat(cipo, self.result.fields.data[:-1]))
            ),
            NextState("PHASE3"),
        ),
        fsm.act("PHASE3",
            If(cycle < 0xf,
                NextValue(cycle, cycle + 1),
                NextState("PHASE0")
               ).Else(
                NextState("IDLE")
            ),
        )


class BetrustedPlatform(LatticePlatform):
    def __init__(self, io, toolchain="icestorm"):
        LatticePlatform.__init__(self, "ice40-up5k-sg48", io, toolchain="icestorm")

    def create_programmer(self):
        raise ValueError("programming is not supported")

    class CRG(Module, AutoCSR, AutoDoc):
        def __init__(self, platform):
            clk12_raw = platform.request("clk12")
            clk_sys = Signal()

            self.clock_domains.cd_sys = ClockDomain()
            self.comb += self.cd_sys.clk.eq(clk_sys)

            platform.add_period_constraint(clk12_raw, 1e9/12e6)  # this is fixed and comes from external crystal

            # POR reset logic- POR generated from sys clk, POR logic feeds sys clk
            # reset. Just need a pulse one cycle wide to get things working right.
            # ^^^ this line is a lie and full of sadness. I have found devices that do not reliably reset with
            # one pulse. Extending the pulse to 2 wide seems to fix the issue.
            # update: found a device that needs 3-wide pulse to reset. Extending to 4 "just in case".
            self.clock_domains.cd_por = ClockDomain()
            reset_cascade = Signal(reset=1)
            reset_cascade2 = Signal(reset=1)
            reset_cascade3 = Signal(reset=1)
            reset_cascade4 = Signal(reset=1)
            reset_initiator = Signal()
            self.sync.por += [
                reset_cascade.eq(reset_initiator),
                reset_cascade2.eq(reset_cascade),
                reset_cascade3.eq(reset_cascade2),
                reset_cascade4.eq(reset_cascade3),
            ]
            self.comb += [
                self.cd_por.clk.eq(self.cd_sys.clk),
                self.cd_sys.rst.eq(reset_cascade4),
            ]

            # generate a >1us-wide pulse at ~1Hz based on sysclk. From legacy code, WDT depends on it.
            extcomm = Signal()
            extcomm_div = Signal(24, reset=int(12e6)) # datasheet range is 0.5Hz - 5Hz, so actual speed is 1.5Hz
            self.sync += [
                If(extcomm_div == 0,
                   extcomm_div.eq(int(12e6))
                ).Else(
                   extcomm_div.eq(extcomm_div - 1)
                ),

                If(extcomm_div < 13,
                   extcomm.eq(1)
                ).Else(
                   extcomm.eq(0)
                )
            ]

            ### WATCHDOG RESET, uses the extcomm_div divider to save on gates
            self.watchdog = CSRStorage(17, fields=[
                CSRField("reset_code", size=16, description="Write `600d` then `c0de` in sequence to this register to reset the watchdog timer"),
                CSRField("enable", description="Enable the watchdog timer. Cannot be disabled once enabled, except with a reset. Notably, a watchdog reset will disable the watchdog.", reset=0),
            ])
            wdog_enabled=Signal(reset=0)
            self.sync += [
                If(self.watchdog.fields.enable,
                    wdog_enabled.eq(1)
                ).Else(
                    wdog_enabled.eq(wdog_enabled)
                )
            ]
            wdog_cycle_r = Signal()
            wdog_cycle = Signal()
            self.sync += wdog_cycle_r.eq(extcomm)
            self.comb += wdog_cycle.eq(extcomm & ~wdog_cycle_r)
            wdog = FSM(reset_state="IDLE")
            self.submodules += wdog
            wdog.act("IDLE",
                If(wdog_enabled,
                    NextState("WAIT_ARM")
                )
            )
            wdog.act("WAIT_ARM",
                # sync up to the watchdog cycle so we give ourselves a full cycle to disarm the watchdog
                If(wdog_cycle,
                    NextState("ARMED")
                )
            )
            wdog.act("ARMED",
                If(wdog_cycle,
                    self.cd_sys.rst.eq(1),
                ),
                If(self.watchdog.re,
                    If(self.watchdog.fields.reset_code == 0x600d,
                        NextState("DISARM1")
                    )
                )
            )
            wdog.act("DISARM1",
                If(wdog_cycle,
                    self.cd_sys.rst.eq(1),
                ),
                If(self.watchdog.re,
                    If(self.watchdog.fields.reset_code == 0xc0de,
                       NextState("DISARMED")
                    ).Else(
                       NextState("ARMED")
                    )
                )
            )
            wdog.act("DISARMED",
                If(wdog_cycle,
                    NextState("ARMED")
                )
            )

            if clk_freq == 21e6:
                divf=55
            elif clk_freq == 19.5e6:
                divf=51
            elif clk_freq == 18e6:
                divf=47
            else:
                print("no sysclk frequency->PLL mapping, aborting")
                exit(0)

            self.specials += Instance(
                "SB_PLL40_PAD",
                # Parameters
                p_DIVR = 0,
                p_DIVF = divf,
                p_DIVQ = 5,
                p_FILTER_RANGE = 1,
                p_FEEDBACK_PATH = "SIMPLE",
                p_DELAY_ADJUSTMENT_MODE_FEEDBACK = "FIXED",
                p_FDA_FEEDBACK = 0,
                p_DELAY_ADJUSTMENT_MODE_RELATIVE = "FIXED",
                p_FDA_RELATIVE = 0,
                p_SHIFTREG_DIV_MODE = 1,
                p_PLLOUT_SELECT = "GENCLK",
                p_ENABLE_ICEGATE = 0,
                # IO
                i_PACKAGEPIN = clk12_raw,
                o_PLLOUTGLOBAL = clk_sys,   # from PLL
                i_BYPASS = 0,
                i_RESETB = 1,
            )
            # global buffer for input SPI clock
            self.clock_domains.cd_sclk = ClockDomain()
            clk_sclk = Signal()
            self.comb += self.cd_sclk.clk.eq(clk_sclk)

            # Add a period constraint for each clock wire.
            # NextPNR picks the clock domain's name randomly from one of the wires
            # that it finds in the domain.  Migen passes the information on timing
            # to NextPNR in a file called `top_pre_pack.py`.  In order to ensure
            # it chooses the timing for this net, annotate period constraints for
            # all wires.
            platform.add_period_constraint(clk_sclk, 1e9/20e6)
            platform.add_period_constraint(clk_sys, 1e9/clk_freq)
            platform.add_period_constraint(self.cd_por.clk, 1e9/clk_freq)


class PicoRVSpi(Module, AutoCSR, AutoDoc):
    def __init__(self, platform, pads, size=2*1024*1024):
        self.intro = ModuleDoc("See https://github.com/cliffordwolf/picorv32/tree/master/picosoc#spi-flash-controller-config-register; used with modifications")
        self.size = size

        self.bus = bus = wishbone.Interface()

        self.reset = Signal()

        cfg = Signal(32)
        cfg_we = Signal(4)
        cfg_out = Signal(32)

        # Add pulse the cfg_we line after reset
        reset_counter = Signal(2, reset=3)
        ic_reset = Signal(reset=1)
        self.sync += \
            If(reset_counter != 0,
                reset_counter.eq(reset_counter - 1)
            ).Else(
                ic_reset.eq(0)
            )

        self.rdata = CSRStatus(fields=[
            CSRField("data", size=4, description="Data bits from SPI [3:hold, 2:wp, 1:cipo, 0:copi]"),
        ])
        self.mode = CSRStorage(fields=[
            CSRField("bitbang", size=1, description="Turn on bitbang mode", reset = 0),
            CSRField("csn", size=1, description="Chip select (set to `0` to select the chip)"),
        ])
        self.wdata = CSRStorage(description="Writes to this field automatically pulse CLK", fields=[
            CSRField("data", size=4, description="Data bits to SPI [3:hold, 2:wp, 1:cipo, 0:copi]"),
            CSRField("oe", size=4, description="Output enable for data pins"),
        ])
        bb_clk = Signal()
        self.sync += [
            bb_clk.eq(self.wdata.re), # auto-clock the SPI chip whenever wdata is written. Delay a cycle for setup/hold.
        ]

        copi_pad = TSTriple()
        cipo_pad = TSTriple()
        cs_n_pad = TSTriple()
        clk_pad  = TSTriple()
        wp_pad   = TSTriple()
        hold_pad = TSTriple()
        self.specials += copi_pad.get_tristate(pads.copi)
        self.specials += cipo_pad.get_tristate(pads.cipo)
        self.specials += cs_n_pad.get_tristate(pads.cs_n)
        self.specials += clk_pad.get_tristate(pads.clk)
        self.specials += wp_pad.get_tristate(pads.wp)
        self.specials += hold_pad.get_tristate(pads.hold)

        reset = Signal()
        self.comb += [
            reset.eq(ResetSignal() | self.reset),
            cs_n_pad.oe.eq(~reset),
            clk_pad.oe.eq(~reset),
        ]

        flash_addr = Signal(24)
        # size/4 because data bus is 32 bits wide, -1 for base 0
        mem_bits = bits_for(int(size/4)-1)
        pad = Signal(2)
        self.comb += flash_addr.eq(Cat(pad, bus.adr[0:mem_bits-1]))

        read_active = Signal()
        spi_ready = Signal()
        self.sync += [
            If(bus.stb & bus.cyc & ~read_active,
                read_active.eq(1),
                bus.ack.eq(0),
            )
            .Elif(read_active & spi_ready,
                read_active.eq(0),
                bus.ack.eq(1),
            )
            .Else(
                bus.ack.eq(0),
                read_active.eq(0),
            )
        ]

        o_rdata = Signal(32)
        self.comb += bus.dat_r.eq(o_rdata)

        self.specials += Instance("spimemio",
            o_flash_io0_oe = copi_pad.oe,
            o_flash_io1_oe = cipo_pad.oe,
            o_flash_io2_oe = wp_pad.oe,
            o_flash_io3_oe = hold_pad.oe,

            o_flash_io0_do = copi_pad.o,
            o_flash_io1_do = cipo_pad.o,
            o_flash_io2_do = wp_pad.o,
            o_flash_io3_do = hold_pad.o,
            o_flash_csb    = cs_n_pad.o,
            o_flash_clk    = clk_pad.o,

            i_flash_io0_di = copi_pad.i,
            i_flash_io1_di = cipo_pad.i,
            i_flash_io2_di = wp_pad.i,
            i_flash_io3_di = hold_pad.i,

            i_resetn = ~reset,
            i_clk = ClockSignal(),

            i_valid = bus.stb & bus.cyc,
            o_ready = spi_ready,
            i_addr  = flash_addr,
            o_rdata = o_rdata,

            i_bb_oe = self.wdata.fields.oe,
            i_bb_wd = self.wdata.fields.data,
            i_bb_clk = bb_clk,
            i_bb_csb = self.mode.fields.csn,
            o_bb_rd = self.rdata.fields.data,
            i_config_update = self.mode.re | ic_reset,
            i_memio_enable = ~self.mode.fields.bitbang,
        )
        platform.add_source("rtl/spimemio.v")

# fork the ticktimer, as we don't need the power management extensions from Xous
class TickTimer(Module, AutoCSR, AutoDoc):
    """Millisecond timer"""

    def __init__(self, clkspertick, clkfreq, bits=64):
        self.clkspertick = int(clkfreq / clkspertick)

        self.intro = ModuleDoc("""TickTimer: A practical systick timer.

        TIMER0 in the system gives a high-resolution, sysclk-speed timer which overflows
        very quickly and requires OS overhead to convert it into a practically usable time source
        which counts off in systicks, instead of sysclks.

        The hardware parameter to the block is the divisor of sysclk, and sysclk. So if
        the divisor is 1000, then the increment for a tick is 1ms. If the divisor is 2000,
        the increment for a tick is 0.5ms. 
        """)

        resolution_in_ms = 1000 * (self.clkspertick / clkfreq)
        self.note = ModuleDoc(title="Configuration",
            body="This timer was configured with {} bits, which rolls over in {:.2f} years, with each bit giving {}ms resolution".format(
                bits, (2 ** bits / (60 * 60 * 24 * 365)) * (self.clkspertick / clkfreq), resolution_in_ms))

        prescaler = Signal(max=self.clkspertick, reset=self.clkspertick)
        timer = Signal(bits)

        self.control = CSRStorage(2, fields=[
            CSRField("reset", description="Write a `1` to this bit to reset the count to 0", pulse=True),
            CSRField("pause", description="Write a `1` to this field to pause counting, 0 for free-run")
        ])
        self.time = CSRStatus(bits, name="time", description="""Elapsed time in systicks""")

        self.sync += [
            If(self.control.fields.reset,
                timer.eq(0),
                prescaler.eq(self.clkspertick),
            ).Else(
                If(prescaler == 0,
                    prescaler.eq(self.clkspertick),
                    If(self.control.fields.pause == 0,
                        timer.eq(timer + 1),
                       )
                   ).Else(
                    prescaler.eq(prescaler - 1),
                )
            )
        ]

        self.comb += self.time.status.eq(timer)

        self.msleep = ModuleDoc("""msleep extension

        The msleep extension is a Xous-specific add-on to aid the implementation of the msleep server.

        msleep fires an interrupt when the requested time is less than or equal to the current elapsed time in
        systicks. The interrupt remains active until a new target is set, or masked. 
        """)
        self.msleep_target = CSRStorage(size=bits, description="Target time in {}ms ticks".format(resolution_in_ms))
        self.submodules.ev = EventManager()
        alarm = Signal()
        self.ev.alarm = EventSourceLevel()
        self.comb += self.ev.alarm.trigger.eq(alarm)

        self.comb += alarm.eq(self.msleep_target.storage <= timer)


# a pared-down version of GitInfo to use less gates
class GitInfo(Module, AutoCSR, AutoDoc):
    def __init__(self):
        self.intro = ModuleDoc("""SoC Version Information

            This block contains various information about the state of the source code
            repository when this SoC was built.
            """)

        def makeint(i, base=10):
            try:
                return int(i, base=base)
            except:
                return 0
        def get_gitver():
            major = 0
            minor = 0
            rev = 0
            gitrev = 0
            gitextra = 0
            dirty = 0

            def decode_version(v):
                version = v.split(".")
                major = 0
                minor = 0
                rev = 0
                if len(version) >= 3:
                    rev = makeint(version[2])
                if len(version) >= 2:
                    minor = makeint(version[1])
                if len(version) >= 1:
                    major = makeint(version[0])
                return (major, minor, rev)
            git_rev_cmd = subprocess.Popen(["git", "describe", "--tags", "--long", "--dirty=+", "--abbrev=8"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE)
            (git_stdout, _) = git_rev_cmd.communicate()
            if git_rev_cmd.wait() != 0:
                print('unable to get git version')
                return (major, minor, rev, gitrev, gitextra, dirty)
            raw_git_rev = git_stdout.decode().strip()

            if raw_git_rev[-1] == "+":
                raw_git_rev = raw_git_rev[:-1]
                dirty = 1

            parts = raw_git_rev.split("-")

            if len(parts) >= 3:
                if parts[0].startswith("v"):
                    version = parts[0]
                    if version.startswith("v"):
                        version = parts[0][1:]
                    (major, minor, rev) = decode_version(version)
                gitextra = makeint(parts[1])
                if parts[2].startswith("g"):
                    gitrev = makeint(parts[2][1:], base=16)
            elif len(parts) >= 2:
                if parts[1].startswith("g"):
                    gitrev = makeint(parts[1][1:], base=16)
                version = parts[0]
                if version.startswith("v"):
                    version = parts[0][1:]
                (major, minor, rev) = decode_version(version)
            elif len(parts) >= 1:
                version = parts[0]
                if version.startswith("v"):
                    version = parts[0][1:]
                (major, minor, rev) = decode_version(version)

            return (major, minor, rev, gitrev, gitextra, dirty)

        (major, minor, rev, gitrev, gitextra, dirty) = get_gitver()

        self.gitrev = CSRStatus(32, reset=gitrev, description="First 32-bits of the git revision.  This documentation was built from git rev ``{:08x}``, so this value is {}, which should be enough to check out the exact git version used to build this firmware.".format(gitrev, gitrev))
        self.dirty = CSRStatus(fields=[
            CSRField("dirty", reset=dirty, description="Set to ``1`` if this device was built from a git repo with uncommitted modifications.")
        ])

        self.comb += [
            self.gitrev.status.eq(gitrev),
            self.dirty.fields.dirty.eq(dirty),
        ]

class BaseSoC(SoCCore):
    global clk_freq
    SoCCore.mem_map = {
        "rom":      0x00000000,  # (default shadow @0x80000000)
        "sram":     0x10000000,  # (default shadow @0xa0000000)
        "spiflash": 0x20000000,  # (default shadow @0xa0000000)
        "csr":      0xe0000000,  # (default shadow @0xe0000000)
    }

    def __init__(self, platform,
                 use_dsp=False, placer="heap", output_dir="build",
                 pnr_seed=0, sim=False,
                 **kwargs):

        self.output_dir = output_dir

        # Core -------------------------------------------------------------------------------------------
        SoCCore.__init__(self, platform, clk_freq,
            integrated_rom_size=0,
            integrated_rom_init=None,
            integrated_sram_size=0,
            ident = "USBC tester",
            with_uart=False,
            cpu_reset_address=self.mem_map["spiflash"]+GATEWARE_SIZE,
            csr_data_width=32, **kwargs)
        self.cpu.use_external_variant(
            "deps/pythondata-cpu-vexriscv/pythondata_cpu_vexriscv/verilog/VexRiscv_IMC.v")

        self.submodules.crg = platform.CRG(platform)
        self.add_csr("crg")

        # Version ----------------------------------------------------------------------------------------
        self.submodules.git = GitInfo()
        self.add_csr("git")

        # Debug ------------------------------------------------------------------------------------------
        self.submodules.uart_phy = uart.UARTPHY(
            pads=platform.request("serial"),
            clk_freq=clk_freq,
            baudrate=115200)
        self.submodules.uart = ResetInserter()(uart.UART(self.uart_phy,
        tx_fifo_depth=256,
        rx_fifo_depth=16))
        self.add_csr("uart_phy")
        self.add_csr("uart")
        self.add_interrupt("uart")

        # RAM/ROM/reset cluster --------------------------------------------------------------------------
        spram_size = 128*1024
        self.submodules.spram = ram.Up5kSPRAM(size=spram_size)
        self.register_mem("sram", self.mem_map["sram"], self.spram.bus, spram_size)

        # Add a simple bit-banged SPI Flash module
        spi_pads = platform.request("spiflash")
        self.submodules.picorvspi = PicoRVSpi(platform, spi_pads)
        self.register_mem("spiflash", self.mem_map["spiflash"],
            self.picorvspi.bus, size=SPI_FLASH_SIZE)
        self.add_csr("picorvspi")

        # High-resolution tick timer ---------------------------------------------------------------------
        self.submodules.ticktimer = TickTimer(1000, clk_freq, bits=40)
        self.add_csr("ticktimer")
        self.add_interrupt("ticktimer")

        # Dut --------------------------------------------------------------------------------------------
        self.submodules.dut = Dut(platform.request("dut"), platform.request("run"))
        self.add_csr("dut")

        # Adc --------------------------------------------------------------------------------------------
        self.submodules.adc = Adc(platform.request("adc"))
        self.add_csr("adc")

        # Scope ------------------------------------------------------------------------------------------
        serial_layout = [("tx", 1), ("rx", 1)]
        screen_pads = Record(serial_layout)
        screen = platform.request("screen")
        self.comb += [
            screen.eq(screen_pads.tx),
            screen_pads.rx.eq(1),
        ]
        self.submodules.screen_phy = uart.UARTPHY(
            pads=screen_pads,
            clk_freq=clk_freq,
            baudrate=9600)
        self.submodules.uart = ResetInserter()(uart.UART(self.screen_phy,
        tx_fifo_depth=16,
        rx_fifo_depth=16))
        self.add_csr("screen_phy")
        self.add_csr("screen")
        self.add_interrupt("screen")


        #### Platform config & build below ---------------------------------------------------------------
        # Override default LiteX's yosys/build templates
        assert hasattr(platform.toolchain, "yosys_template")
        assert hasattr(platform.toolchain, "build_template")
        platform.toolchain.yosys_template = [
            "{read_files}",
            "attrmap -tocase keep -imap keep=\"true\" keep=1 -imap keep=\"false\" keep=0 -remove keep=0",
            "synth_ice40 -json {build_name}.json -top {build_name}",
        ]
        platform.toolchain.build_template = [
            "yosys -q -l {build_name}.rpt {build_name}.ys",
            "nextpnr-ice40 --json {build_name}.json --pcf {build_name}.pcf --asc {build_name}.txt \
            --pre-pack {build_name}_pre_pack.py --{architecture} --package {package}",
            "icepack {build_name}.txt {build_name}.bin"
        ]

        # Add "-relut -dffe_min_ce_use 4" to the synth_ice40 command.
        # The "-reult" adds an additional LUT pass to pack more stuff in,
        # and the "-dffe_min_ce_use 4" flag prevents Yosys from generating a
        # Clock Enable signal for a LUT that has fewer than 4 flip-flops.
        # This increases density, and lets us use the FPGA more efficiently.
        platform.toolchain.yosys_template[2] += " -relut -abc2 -dffe_min_ce_use 4 -relut"
        if use_dsp:
            platform.toolchain.yosys_template[2] += " -dsp"

        # Disable final deep-sleep power down so firmware words are loaded
        # onto softcore's address bus.
        platform.toolchain.build_template[2] = "icepack -s {build_name}.txt {build_name}.bin"

        # Allow us to set the nextpnr seed
        platform.toolchain.build_template[1] += " --seed " + str(pnr_seed)

        if placer is not None:
            platform.toolchain.build_template[1] += " --placer {}".format(placer)

        # Allow loops for RNG placement
        platform.toolchain.build_template[1] += " --ignore-loops"

        if sim:
            class _WishboneBridge(Module):
                def __init__(self, interface):
                    self.wishbone = interface
            self.add_cpu(_WishboneBridge(self.platform.request("wishbone")))
            self.add_wb_master(self.cpu.wishbone)


    def copy_memory_file(self, src):
        import os
        from shutil import copyfile
        if not os.path.exists(self.output_dir):
            os.mkdir(self.output_dir)
        if not os.path.exists(os.path.join(self.output_dir, "gateware")):
            os.mkdir(os.path.join(self.output_dir, "gateware"))
        copyfile(os.path.join("rtl", src), os.path.join(self.output_dir, "gateware", src))


def make_multiboot_header(filename, boot_offsets=[160]):
    """
    ICE40 allows you to program the SB_WARMBOOT state machine by adding the following
    values to the bitstream, before any given image:

    [7e aa 99 7e]       Sync Header
    [92 00 k0]          Boot mode (k = 1 for cold boot, 0 for warmboot)
    [44 03 o1 o2 o3]    Boot address
    [82 00 00]          Bank offset
    [01 08]             Reboot
    [...]               Padding (up to 32 bytes)

    Note that in ICE40, the second nybble indicates the number of remaining bytes
    (with the exception of the sync header).

    The above construct is repeated five times:

    INITIAL_BOOT        The image loaded at first boot
    BOOT_S00            The first image for SB_WARMBOOT
    BOOT_S01            The second image for SB_WARMBOOT
    BOOT_S10            The third image for SB_WARMBOOT
    BOOT_S11            The fourth image for SB_WARMBOOT
    """
    while len(boot_offsets) < 5:
        boot_offsets.append(boot_offsets[0])

    with open(filename, 'wb') as output:
        for offset in boot_offsets:
            # Sync Header
            output.write(bytes([0x7e, 0xaa, 0x99, 0x7e]))

            # Boot mode
            output.write(bytes([0x92, 0x00, 0x00]))

            # Boot address
            output.write(bytes([0x44, 0x03,
                    (offset >> 16) & 0xff,
                    (offset >> 8)  & 0xff,
                    (offset >> 0)  & 0xff]))

            # Bank offset
            output.write(bytes([0x82, 0x00, 0x00]))

            # Reboot command
            output.write(bytes([0x01, 0x08]))

            for x in range(17, 32):
                output.write(bytes([0]))

def pad_file(pad_src, pad_dest, length):
    with open(pad_dest, "wb") as output:
        with open(pad_src, "rb") as b:
            output.write(b.read())
        output.truncate(length)

def merge_file(bios, gateware, dest):
    with open(dest, "wb") as output:
        count = 0
        with open(gateware, "rb") as gw:
            count = count + output.write(gw.read())
        with open(bios, "rb") as b:
            b.seek(count)
            output.write(b.read())



def main():
    if os.environ['PYTHONHASHSEED'] != "1":
        print( "PYTHONHASHEED must be set to 1 for consistent validation results. Failing to set this results in non-deterministic compilation results")
        exit()

    parser = argparse.ArgumentParser(description="Build the Betrusted Embedded Controller")
    parser.add_argument(
        "-D", "--document-only", default=False, action="store_true", help="Build docs only"
    )
    parser.add_argument(
        "--no-cpu", help="disable cpu generation for debugging purposes", action="store_true"
    )
    parser.add_argument(
        "--placer", choices=["sa", "heap"], help="which placer to use in nextpnr", default="heap",
    )
    parser.add_argument(
        "--seed", default=0, help="seed to use in nextpnr"
    )
    args = parser.parse_args()

    output_dir = 'build'

    compile_gateware = True
    compile_software = False # this is now done with Rust

    if args.document_only:
        compile_gateware = False
        compile_software = False

    cpu_type = "vexriscv"
    cpu_variant = "minimal"
    #cpu_variant = cpu_variant + "+debug"

    if args.no_cpu:
        cpu_type = None
        cpu_variant = None

    platform = BetrustedPlatform(io)

    soc = BaseSoC(platform, cpu_type=cpu_type, cpu_variant=cpu_variant,
                            use_dsp=True, placer=args.placer,
                            pnr_seed=args.seed,
                            output_dir=output_dir)
    builder = Builder(soc, output_dir=output_dir, csr_csv="build/csr.csv", compile_software=compile_software, compile_gateware=compile_gateware)
    # If we compile software, pull the code from somewhere other than
    # the built-in litex "bios" binary, which makes assumptions about
    # what peripherals are available.
    if compile_software:
        builder.software_packages = [
            ("bios", os.path.abspath(os.path.join(os.path.dirname(__file__), "bios")))
        ]

    try:
        vns = builder.build()
    except OSError:
        exit(1)

    soc.do_exit(vns)

    if not args.document_only:
        make_multiboot_header(os.path.join(output_dir, "gateware", "multiboot-header.bin"), [
            160,
            160,
            157696,
            262144,
            262144 + 32768,
        ])

        with open(os.path.join(output_dir, 'gateware', 'multiboot-header.bin'), 'rb') as multiboot_header_file:
            multiboot_header = multiboot_header_file.read()
            with open(os.path.join(output_dir, 'gateware', 'usbc_tester.bin'), 'rb') as top_file:
                top = top_file.read()
                with open(os.path.join(output_dir, 'gateware', 'usbc_tester_multiboot.bin'), 'wb') as top_multiboot_file:
                    top_multiboot_file.write(multiboot_header)
                    top_multiboot_file.write(top)
        pad_file(os.path.join(output_dir, 'gateware', 'usbc_tester.bin'), os.path.join(output_dir, 'gateware', 'usbc_tester_pad.bin'), 0x1a000)
        pad_file(os.path.join(output_dir, 'gateware', 'usbc_tester_multiboot.bin'), os.path.join(output_dir, 'gateware', 'usbc_tester_multiboot_pad.bin'), 0x1a000)

    lxsocdoc.generate_docs(soc, "build/documentation", note_pulses=True)
    lxsocdoc.generate_svd(soc, "build/software")

if __name__ == "__main__":
    from datetime import datetime
    start = datetime.now()
    main()
    print("Run completed in {}".format(datetime.now()-start))
