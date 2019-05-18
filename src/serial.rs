//! Serial interface
//!
//! You can use the `Serial` interface with these UART instances:
//! * [`UARTHS`](crate::pac::UARTHS)
//! * [`UART1`](crate::pac::UART1)
//! * [`UART2`](crate::pac::UART2)
//! * [`UART3`](crate::pac::UART3)

use core::mem;
use core::ops::Deref;

use embedded_hal::serial;
use nb;
use void::Void;

use crate::pac::{UARTHS,uart1,UART1,UART2,UART3};
use crate::clock::Clocks;
use crate::time::Bps;

const UART_RECEIVE_FIFO_1: u32 = 0;
const UART_SEND_FIFO_8: u32 = 3;

/// Extension trait that constrains UART peripherals
pub trait SerialExt: Sized {
    /// Constrains UART peripheral so it plays nicely with the other abstractions
    fn constrain(self, baud_rate: Bps, clocks: &Clocks) -> Serial<Self>;
}

impl SerialExt for UARTHS {
    fn constrain(self, baud_rate: Bps, clocks: &Clocks) -> Serial<UARTHS> {
        Serial::<UARTHS>::new(self, baud_rate, clocks)
    }
}

/// Trait to be able to generalize over UART1/UART2/UART3
pub trait UartX: Deref<Target = uart1::RegisterBlock> { }
impl UartX for UART1 { }
impl UartX for UART2 { }
impl UartX for UART3 { }

impl<UART: UartX> SerialExt for UART {
    fn constrain(self, baud_rate: Bps, clocks: &Clocks) -> Serial<UART> {
        Serial::<UART>::new(self, baud_rate, clocks)
    }
}

/// Serial abstraction
pub struct Serial<UART> {
    uart: UART,
}

/// Serial receiver
pub struct Rx<UART> {
    uart: UART,
}

/// Serial transmitter
pub struct Tx<UART> {
    uart: UART,
}

impl<UART> Rx<UART> {
    /// Forms `Serial` abstraction from a transmitter and a
    /// receiver half
    pub fn join(self, _tx: Tx<UART>) -> Serial<UART> {
        Serial { uart: self.uart }
    }
}

impl<UART> Serial<UART> {
    /// Splits the `Serial` abstraction into a transmitter and a
    /// receiver half
    pub fn split(self) -> (Tx<UART>, Rx<UART>) {
        (
            Tx {
                uart: unsafe { mem::zeroed() }
            },
            Rx {
                uart: self.uart
            }
        )
    }

    /// Releases the UART peripheral
    pub fn free(self) -> UART {
        self.uart
    }
}

impl Serial<UARTHS> {
    /// Configures a UART peripheral to provide serial communication
    pub fn new(uart: UARTHS, baud_rate: Bps, clocks: &Clocks) -> Self {
        let div = clocks.cpu().0 / baud_rate.0 - 1;
        unsafe {
            uart.div.write(|w| w.bits(div));
        }

        uart.txctrl.write(|w| w.txen().bit(true));
        uart.rxctrl.write(|w| w.rxen().bit(true));

        Serial { uart }
    }

    /// Starts listening for an interrupt event
    pub fn listen(self) -> Self {
        self.uart.ie.write(|w| w.txwm().bit(false).rxwm().bit(true));
        self
    }

    /// Stops listening for an interrupt event
    pub fn unlisten(self) -> Self {
        self.uart
            .ie
            .write(|w| w.txwm().bit(false).rxwm().bit(false));
        self
    }
}

impl serial::Read<u8> for Rx<UARTHS> {
    type Error = Void;

    fn read(&mut self) -> nb::Result<u8, Void> {
        let rxdata = self.uart.rxdata.read();

        if rxdata.empty().bit_is_set() {
            Err(::nb::Error::WouldBlock)
        } else {
            Ok(rxdata.data().bits() as u8)
        }
    }
}

impl serial::Write<u8> for Tx<UARTHS> {
    type Error = Void;

    fn write(&mut self, byte: u8) -> nb::Result<(), Void> {
        let txdata = self.uart.txdata.read();

        if txdata.full().bit_is_set() {
            Err(::nb::Error::WouldBlock)
        } else {
            unsafe {
                (*UARTHS::ptr()).txdata.write(|w| w.data().bits(byte));
            }
            Ok(())
        }
    }

    fn flush(&mut self) -> nb::Result<(), Void> {
        let txdata = self.uart.txdata.read();

        if txdata.full().bit_is_set() {
            Err(nb::Error::WouldBlock)
        } else {
            Ok(())
        }
    }
}


impl<UART: UartX> Serial<UART> {
    /// Configures a UART peripheral to provide serial communication
    pub fn new(uart: UART, baud_rate: Bps, clocks: &Clocks) -> Self {
        // Hardcode these for now:
        let data_width = 8; // 8 data bits
        let stopbit_val = 0; // 1 stop bit
        let parity_val = 0; // No parity
        // Note: need to make sure that UARTx clock is enabled through sysctl before here
        let divisor = clocks.apb0().0 / baud_rate.0;
        let dlh = ((divisor >> 12) & 0xff) as u8;
        let dll = ((divisor >> 4) & 0xff) as u8;
        let dlf = (divisor & 0xf) as u8;
        unsafe {
            // Set Divisor Latch Access Bit (enables DLL DLH) to set baudrate
            uart.lcr.write(|w| w.bits(1 << 7));
            uart.dlh_ier.write(|w| w.bits(dlh.into()));
            uart.rbr_dll_thr.write(|w| w.bits(dll.into()));
            uart.dlf.write(|w| w.bits(dlf.into()));
            // Clear Divisor Latch Access Bit after setting baudrate
            uart.lcr.write(|w| w.bits((data_width - 5) | (stopbit_val << 2) | (parity_val << 3)));
            // Write IER
            uart.dlh_ier.write(|w| w.bits(0x80)); /* THRE */
            // Write FCT
            uart.fcr_iir.write(|w| w.bits(UART_RECEIVE_FIFO_1 << 6 | UART_SEND_FIFO_8 << 4 | 0x1 << 3 | 0x1));
        }

        Serial { uart }
    }

    /// Starts listening for an interrupt event
    pub fn listen(self) -> Self {
        self
    }

    /// Stops listening for an interrupt event
    pub fn unlisten(self) -> Self {
        self
    }
}

impl<UART: UartX> serial::Read<u8> for Rx<UART> {
    type Error = Void;

    fn read(&mut self) -> nb::Result<u8, Void> {
        let lsr = self.uart.lsr.read();

        if (lsr.bits() & (1<<0)) == 0 { // Data Ready bit
            Err(::nb::Error::WouldBlock)
        } else {
            let rbr = self.uart.rbr_dll_thr.read();
            Ok((rbr.bits() & 0xff) as u8)
        }
    }
}

impl<UART: UartX> serial::Write<u8> for Tx<UART> {
    type Error = Void;

    fn write(&mut self, byte: u8) -> nb::Result<(), Void> {
        let lsr = self.uart.lsr.read();

        if (lsr.bits() & (1<<5)) != 0 { // Transmit Holding Register Empty bit
            Err(::nb::Error::WouldBlock)
        } else {
            unsafe {
                self.uart.rbr_dll_thr.write(|w| w.bits(byte.into()));
            }
            Ok(())
        }
    }

    fn flush(&mut self) -> nb::Result<(), Void> {
        // TODO
        Ok(())
    }
}
