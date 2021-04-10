//! Safe access to all CSRs and

#[macro_use]
mod macros;

pub mod satp;

csr_mod!(r, mvendorid, 0xF11);
csr_mod!(r, marchid, 0xF12);
csr_mod!(r, mimpid, 0xF13);
csr_mod!(r, mhartid, 0xF14);

csr_mod!(rw, misa, 0x301);

csr_mod!(rw, sstatus, 0x100);
csr_mod!(rw, sie, 0x104);
csr_mod!(rw, stvec, 0x105);
csr_mod!(rw, sip, 0x144);

csr_mod!(r, time, 0xC01);
