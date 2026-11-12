//! The shared Atlas command on this disk, as the user desktop runs it: the
//! folder beside it where builds and environment plans are staged.
use crate::client::Client;
use atlas_core::error::Result;

impl Client {
    /// [`Client::writable`]'s check for the second login `login`: builds and
    /// environment plans are staged beside the shared Atlas command for it to
    /// read; a missing folder is setup's concern, not this check's.
    pub(crate) fn staging_writable_in(&self, _login: &atlas_core::user::User) -> Result<()> {
        if let Some(shared) = self.config.identity_probe.parent()
            && shared.is_dir()
        {
            atlas_core::files::probe_write(shared)?;
        }

        Ok(())
    }
}
