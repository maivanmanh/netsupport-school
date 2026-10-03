use anyhow::{Context, Result};

/// Process-unique presentation IDs. The random process epoch prevents an existing
/// Student from confusing a restarted Teacher's first stream with its last one;
/// the counter keeps IDs distinct within one process.
pub(super) struct PresentationIds {
    last: u64,
}

impl PresentationIds {
    pub(super) fn new() -> Result<Self> {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes)
            .map_err(|error| anyhow::anyhow!("generate Teacher presentation epoch: {error}"))?;
        // Keep the random epoch below MAX so the first checked increment always
        // succeeds. The low 32 bits remain random for presentation_ssrc().
        let last = u64::from_le_bytes(bytes) & 0x7fff_ffff_ffff_fffe;
        Ok(Self { last })
    }

    #[cfg(test)]
    fn with_seed(last: u64) -> Self {
        Self { last }
    }

    pub(super) fn next(&mut self) -> Result<u64> {
        self.last = self
            .last
            .checked_add(1)
            .context("presentation identifier exhausted")?;
        Ok(self.last)
    }
}

#[cfg(test)]
mod tests {
    use super::PresentationIds;

    #[test]
    fn ids_advance_within_a_teacher_process() {
        let mut ids = PresentationIds::with_seed(0x1234_5678_0000_0040);
        assert_eq!(ids.next().unwrap(), 0x1234_5678_0000_0041);
        assert_eq!(ids.next().unwrap(), 0x1234_5678_0000_0042);
    }

    #[test]
    fn ids_do_not_wrap_and_reuse_an_old_stream() {
        let mut ids = PresentationIds::with_seed(u64::MAX - 1);
        assert_eq!(ids.next().unwrap(), u64::MAX);
        assert!(ids.next().is_err());
    }
}
