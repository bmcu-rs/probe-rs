//! Debug sequences for Baikal Electronics RISC-V microcontrollers.

use std::{sync::Arc, thread, time::Duration};

use crate::{
    Error, Session,
    architecture::riscv::{
        communication_interface::RiscvCommunicationInterface, sequences::RiscvDebugSequence,
    },
    flashing::DebugFlashSequence,
    probe::DebugProbeError,
};

use super::eflash::BeU1000Eflash;

/// Debug sequence for the Baikal BE-U1000.
#[derive(Debug)]
pub struct BeU1000Sequence;

impl BeU1000Sequence {
    /// Creates the debug sequence.
    pub fn create() -> Arc<dyn RiscvDebugSequence> {
        Arc::new(Self)
    }
}

impl RiscvDebugSequence for BeU1000Sequence {
    fn debug_flash_sequence(&self) -> Option<Arc<dyn DebugFlashSequence>> {
        Some(Arc::new(BeU1000Eflash))
    }

    fn direct_start_address(&self, elf_entry_point: u64) -> Option<u64> {
        Some(elf_entry_point)
    }

    fn reset_system_and_halt(
        &self,
        interface: &mut RiscvCommunicationInterface,
        timeout: Duration,
    ) -> Result<(), Error> {
        match interface.target_reset_assert() {
            Ok(()) => {
                thread::sleep(Duration::from_millis(100));
                interface.target_reset_deassert()?;
                thread::sleep(Duration::from_millis(500));
                interface.enter_debug_mode()?;
            }
            Err(DebugProbeError::NotImplemented { .. }) => {}
            Err(error) => return Err(error.into()),
        }

        // The BE-U1000 debug module never asserts the optional havereset
        // status bits. allhalted is its acknowledgement that reset-and-halt
        // has completed.
        interface.reset_hart_and_halt_allow_missing_havereset(timeout)?;
        Ok(())
    }

    fn prepare_running_on_ram(
        &self,
        session: &mut Session,
        vector_table_addr: u64,
        core_id: usize,
    ) -> Result<(), Error> {
        let mut core = session.core(core_id)?;
        let program_counter = core.program_counter().id;
        core.write_core_reg(program_counter, vector_table_addr)?;
        Ok(())
    }
}
