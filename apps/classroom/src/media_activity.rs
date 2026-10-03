//! Keep a live media session responsive when its window is hidden/minimized.
//! The activity ends with the session; idle agents retain normal power policy.

#[cfg(target_os = "macos")]
use objc2::{rc::Retained, runtime::ProtocolObject};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

pub(super) struct MediaActivity {
    #[cfg(target_os = "macos")]
    process: Retained<NSProcessInfo>,
    #[cfg(target_os = "macos")]
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl MediaActivity {
    pub(super) fn begin() -> Self {
        #[cfg(target_os = "macos")]
        {
            let process = NSProcessInfo::processInfo();
            let token = process.beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiatedAllowingIdleSystemSleep
                    | NSActivityOptions::LatencyCritical,
                &NSString::from_str("Classroom live screen media"),
            );
            tracing::info!("macOS latency-critical media activity started");
            Self { process, token }
        }
        #[cfg(not(target_os = "macos"))]
        Self {}
    }
}

#[cfg(target_os = "macos")]
impl Drop for MediaActivity {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: token was returned by beginActivity on this process and is
        // ended exactly once by this non-cloneable owner, including on errors.
        unsafe { self.process.endActivity(&self.token) };
        tracing::info!("macOS latency-critical media activity ended");
    }
}
