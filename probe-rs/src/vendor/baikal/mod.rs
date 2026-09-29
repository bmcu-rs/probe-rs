//! Baikal Electronics vendor support.

use probe_rs_target::Chip;

use crate::{config::DebugSequence, vendor::Vendor};

mod eflash;
pub mod sequences;

/// Baikal Electronics.
#[derive(docsplay::Display)]
pub struct Baikal;

impl Vendor for Baikal {
    fn try_create_debug_sequence(&self, chip: &Chip) -> Option<DebugSequence> {
        if chip.name == "BE-U1000" {
            Some(DebugSequence::Riscv(sequences::BeU1000Sequence::create()))
        } else {
            None
        }
    }
}
