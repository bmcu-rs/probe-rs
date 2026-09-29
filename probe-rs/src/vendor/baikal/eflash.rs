//! Host-side programmer for the BE-U1000 main eFlash array.

use std::{
    thread,
    time::{Duration, Instant},
};

use crate::{Core, Error, MemoryInterface, Session, flashing::DebugFlashSequence};

const CORE_INDEX: usize = 0;

const SYS_CTL0: u64 = 0x1400_0018;
const SYS_CTL0_NVR: u32 = 1 << 10;
const SYS_CTL0_EFLASH_NVR_DISABLE: u32 = 1 << 26;
const SYS_CTL0_DMA_MODE_DISABLE: u32 = 1 << 31;

const FLASH_CONTROL: u64 = 0xa100_0000;
const FLASH_ADDRESS: u64 = 0xa100_0004;
const FLASH_WRITE_DATA: u64 = 0xa100_0008;
const FLASH_STATUS: u64 = 0xa100_0010;
const FLASH_STATUS_BUSY: u32 = 1;
const FLASH_PROGRAM_MAIN: u32 = 0x3;
const FLASH_ERASE_MAIN: u32 = 0x7;

const EFLASH_START: u64 = 0xa000_0000;
const EFLASH_END: u64 = 0xa004_0000;
const ERASE_TIMEOUT: Duration = Duration::from_secs(10);
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(1);

/// Main-array-only eFlash programmer for the BE-U1000.
#[derive(Debug, Default)]
pub struct BeU1000Eflash;

impl BeU1000Eflash {
    fn validate_range(address: u64, data_len: usize) -> Result<(), Error> {
        if data_len == 0 {
            return Err(Error::Other(
                "BE-U1000 eFlash writes must not be empty".into(),
            ));
        }

        let end = address
            .checked_add(data_len as u64)
            .ok_or_else(|| Error::Other("BE-U1000 eFlash address overflow".into()))?;

        if address < EFLASH_START || end > EFLASH_END {
            return Err(Error::Other(format!(
                "BE-U1000 eFlash write outside main array: {address:#010x}..{end:#010x}"
            )));
        }
        if !address.is_multiple_of(4) || !data_len.is_multiple_of(4) {
            return Err(Error::Other(format!(
                "BE-U1000 eFlash writes must be 32-bit aligned (address {address:#010x}, length {data_len})"
            )));
        }

        Ok(())
    }

    fn closed_control_value(value: u32) -> u32 {
        (value | SYS_CTL0_EFLASH_NVR_DISABLE | SYS_CTL0_DMA_MODE_DISABLE) & !SYS_CTL0_NVR
    }

    fn write_sys_ctl0(core: &mut Core<'_>, value: u32) -> Result<u32, Error> {
        core.write_word_32(SYS_CTL0, value)?;
        core.read_word_32(SYS_CTL0)
    }

    fn close_program_window(core: &mut Core<'_>) -> Result<(), Error> {
        let value = core.read_word_32(SYS_CTL0)?;
        let readback = Self::write_sys_ctl0(core, Self::closed_control_value(value))?;
        let mask = SYS_CTL0_EFLASH_NVR_DISABLE | SYS_CTL0_DMA_MODE_DISABLE | SYS_CTL0_NVR;
        let expected = SYS_CTL0_EFLASH_NVR_DISABLE | SYS_CTL0_DMA_MODE_DISABLE;
        if readback & mask != expected {
            return Err(Error::Other(format!(
                "failed to close BE-U1000 eFlash programming window: SYS_CTL0={readback:#010x}"
            )));
        }
        Ok(())
    }

    fn protect_nvr(core: &mut Core<'_>) -> Result<(), Error> {
        let value = core.read_word_32(SYS_CTL0)?;
        let readback = Self::write_sys_ctl0(core, Self::closed_control_value(value))?;
        let mask = SYS_CTL0_EFLASH_NVR_DISABLE | SYS_CTL0_DMA_MODE_DISABLE | SYS_CTL0_NVR;
        let expected = SYS_CTL0_EFLASH_NVR_DISABLE | SYS_CTL0_DMA_MODE_DISABLE;
        if readback & mask != expected {
            return Err(Error::Other(format!(
                "failed to protect BE-U1000 eFlash NVR: SYS_CTL0={readback:#010x}"
            )));
        }
        Ok(())
    }

    fn wait_until_idle(core: &mut Core<'_>, timeout: Duration) -> Result<(), Error> {
        let start = Instant::now();
        loop {
            let status = core.read_word_32(FLASH_STATUS)?;
            if status & FLASH_STATUS_BUSY == 0 {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(Error::Other(format!(
                    "timed out waiting for BE-U1000 eFlash: STATUS={status:#010x}"
                )));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn programming_words(address: u64, data: &[u8]) -> impl Iterator<Item = (u32, u32)> + '_ {
        debug_assert!(address.is_multiple_of(4));
        debug_assert!(address >= EFLASH_START);
        debug_assert!(data.len().is_multiple_of(4));

        data.chunks_exact(4)
            .enumerate()
            .filter_map(move |(index, bytes)| {
                let controller_address = ((address - EFLASH_START) / 4) as u32 + index as u32;
                let value = u32::from_le_bytes(bytes.try_into().expect("four-byte chunk"));
                (value != u32::MAX).then_some((controller_address, value))
            })
    }
}

impl DebugFlashSequence for BeU1000Eflash {
    fn prepare_flash(&self, session: &mut Session) -> Result<(), Error> {
        let mut core = session.core(CORE_INDEX)?;
        if !core.core_halted()? {
            core.halt(Duration::from_millis(500))?;
        }
        Self::close_program_window(&mut core)?;
        Self::wait_until_idle(&mut core, Duration::from_secs(1))
    }

    fn erase_all(&self, session: &mut Session) -> Result<(), Error> {
        let mut core = session.core(CORE_INDEX)?;
        Self::protect_nvr(&mut core)?;
        Self::wait_until_idle(&mut core, Duration::from_secs(1))?;

        tracing::info!("Erasing BE-U1000 main eFlash array (NVR is protected)");
        core.write_word_32(FLASH_CONTROL, FLASH_ERASE_MAIN)?;
        thread::sleep(Duration::from_millis(1));
        Self::wait_until_idle(&mut core, ERASE_TIMEOUT)
    }

    fn program(&self, session: &mut Session, address: u64, data: &[u8]) -> Result<(), Error> {
        Self::validate_range(address, data.len())?;

        let mut core = session.core(CORE_INDEX)?;
        Self::protect_nvr(&mut core)?;
        Self::wait_until_idle(&mut core, PROGRAM_TIMEOUT)?;

        // Keep the direct-write aperture disabled and use the controller's
        // single-word command, matching the vendor SDK's EFLASH_ProgramWord.
        // Bulk writes to the direct aperture can make RISC-V SBA set
        // `sbbusyerror` between flash operations and retry forever.
        // The host flasher has just erased the complete main array, so the
        // programming plan omits words that already have the erased value.
        let write_result =
            Self::programming_words(address, data).try_for_each(|(controller_address, word)| {
                core.write_word_32(FLASH_ADDRESS, controller_address)?;
                core.write_word_32(FLASH_WRITE_DATA, word)?;
                // Bit 4 (NVR) is deliberately clear in this command.
                core.write_word_32(FLASH_CONTROL, FLASH_PROGRAM_MAIN)?;
                Self::wait_until_idle(&mut core, PROGRAM_TIMEOUT)
            });
        let close_result = Self::close_program_window(&mut core);

        match (write_result, close_result) {
            (_, Err(close_error)) => Err(close_error),
            (Err(write_error), Ok(())) => Err(write_error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn verify(&self, session: &mut Session, address: u64, data: &[u8]) -> Result<bool, Error> {
        Self::validate_range(address, data.len())?;

        let mut core = session.core(CORE_INDEX)?;
        Self::close_program_window(&mut core)?;
        let mut readback = vec![0; data.len()];
        core.read(address, &mut readback)?;
        Ok(readback == data)
    }

    fn supports_sector_erase(&self) -> bool {
        false
    }

    fn finish_flash(&self, session: &mut Session) -> Result<(), Error> {
        let mut core = session.core(CORE_INDEX)?;
        Self::close_program_window(&mut core)?;
        if core.read_word_32(SYS_CTL0)? & SYS_CTL0_EFLASH_NVR_DISABLE != 0 {
            tracing::debug!(
                "BE-U1000 eFlash NVR protection remains active until the next external reset"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_aligned_main_array_ranges() {
        assert!(
            BeU1000Eflash::validate_range(EFLASH_START, (EFLASH_END - EFLASH_START) as usize)
                .is_ok()
        );
        assert!(BeU1000Eflash::validate_range(EFLASH_START, 0x100).is_ok());
        assert!(BeU1000Eflash::validate_range(EFLASH_END - 4, 4).is_ok());
        assert!(BeU1000Eflash::validate_range(EFLASH_START - 4, 4).is_err());
        assert!(BeU1000Eflash::validate_range(EFLASH_END, 4).is_err());
        assert!(BeU1000Eflash::validate_range(EFLASH_END - 4, 8).is_err());
        assert!(BeU1000Eflash::validate_range(u64::MAX - 3, 8).is_err());
        assert!(BeU1000Eflash::validate_range(EFLASH_START + 1, 4).is_err());
        assert!(BeU1000Eflash::validate_range(EFLASH_START, 3).is_err());
        assert!(BeU1000Eflash::validate_range(EFLASH_START, 0).is_err());
    }

    #[test]
    fn controller_values_always_select_and_protect_the_main_array() {
        for initial in [
            0,
            SYS_CTL0_NVR,
            SYS_CTL0_EFLASH_NVR_DISABLE,
            SYS_CTL0_DMA_MODE_DISABLE,
            u32::MAX,
        ] {
            let closed = BeU1000Eflash::closed_control_value(initial);
            assert_eq!(closed & SYS_CTL0_NVR, 0);
            assert_ne!(closed & SYS_CTL0_EFLASH_NVR_DISABLE, 0);
            assert_ne!(closed & SYS_CTL0_DMA_MODE_DISABLE, 0);
        }
    }

    #[test]
    fn erase_command_targets_only_the_main_array() {
        // The controller's main-array-only command is deliberately used instead of
        // an all-array/NVR erase operation.
        assert_eq!(FLASH_ERASE_MAIN, 0x7);
        assert_eq!(FLASH_ERASE_MAIN & 0x10, 0);
        assert_eq!(FLASH_PROGRAM_MAIN, 0x3);
        assert_eq!(FLASH_PROGRAM_MAIN & 0x10, 0);
    }

    #[test]
    fn programming_plan_uses_little_endian_aligned_words() {
        let words = BeU1000Eflash::programming_words(
            EFLASH_START + 4,
            &[
                0x11, 0x22, 0x33, 0x44, 0xff, 0xff, 0xff, 0xff, 0xaa, 0xbb, 0xcc, 0xdd,
            ],
        )
        .collect::<Vec<_>>();

        assert_eq!(words, vec![(1, 0x4433_2211), (3, 0xddcc_bbaa),]);
    }
}
