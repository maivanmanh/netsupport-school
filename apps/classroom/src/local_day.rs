//! The local calendar day as `yyyymmdd`, used to expire classroom rules at
//! midnight: a rule stamped with one day is void on any other, so the next
//! class (possibly another teacher and other students) starts unrestricted.
//! A plain integer comparison — no time-zone arithmetic — so a clock or time
//! zone shift can never leave a rule alive for longer than one local day.

#[cfg(windows)]
#[allow(unsafe_code)]
pub fn today() -> u32 {
    // SAFETY: plain query with no arguments.
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    u32::from(now.wYear) * 10_000 + u32::from(now.wMonth) * 100 + u32::from(now.wDay)
}

#[cfg(unix)]
#[allow(unsafe_code)]
pub fn today() -> u32 {
    // SAFETY: `localtime_r` writes into the zeroed `tm` we own.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut local).is_null() {
            return 0;
        }
        u32::try_from(local.tm_year + 1900).unwrap_or(0) * 10_000
            + u32::try_from(local.tm_mon + 1).unwrap_or(0) * 100
            + u32::try_from(local.tm_mday).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn today_looks_like_a_calendar_date() {
        let day = super::today();
        assert!((20_200_101..21_000_101).contains(&day), "{day}");
        assert!((1..=12).contains(&(day / 100 % 100)));
        assert!((1..=31).contains(&(day % 100)));
    }
}
