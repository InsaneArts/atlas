//! The helper login's own preferences, applied once the daemon has proved
//! where it runs.
use crate::{
    error::{Code, Error, Result},
    native::cf,
};
use std::process::{Command, Stdio};

/// Typing in the helper arrives exactly as sent: macOS's autocorrect,
/// automatic capitalisation, smart punctuation and predictions are turned
/// off in the helper's own preferences (apps read them when they start).
/// The person's account is never touched.
pub(super) fn type_exactly() {
    for key in [
        "NSAutomaticSpellingCorrectionEnabled",
        "NSAutomaticCapitalizationEnabled",
        "NSAutomaticPeriodSubstitutionEnabled",
        "NSAutomaticDashSubstitutionEnabled",
        "NSAutomaticQuoteSubstitutionEnabled",
        "NSAutomaticTextReplacementEnabled",
        "NSAutomaticTextCompletionEnabled",
        "NSAutomaticInlinePredictionEnabled",
        "WebAutomaticSpellingCorrectionEnabled",
    ] {
        let _ = Command::new("/usr/bin/defaults")
            .args(["write", "-g", key, "-bool", "false"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Keep a wallpaper click from hiding every helper window. Called only after
/// the daemon has proved it runs as the configured helper in its Aqua login.
pub(super) fn keep_desktop() -> Result<()> {
    use objc2::AllocAnyThread;
    use objc2_foundation::{NSString, NSUserDefaults};
    let domain = NSString::from_str("com.apple.WindowManager");
    let key = NSString::from_str("EnableStandardClickToShowDesktop");
    let failed = || Error::new(Code::Native, "Could not set the helper desktop preference");
    let defaults = NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain))
        .ok_or_else(failed)?;
    defaults.setBool_forKey(false, &key);
    if !defaults.synchronize() || !cf::notify_preferences("com.apple.ec.WindowManager.preferences")
    {
        return Err(failed());
    }
    Ok(())
}
