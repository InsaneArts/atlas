//! What the user desktop's own `atlas` commands do from the terminal:
//! setting it up (`init`, `setup`, `setup-helper`), what doctor sees of it,
//! switching this Mac to it, opening Atlas as an app, and an update's
//! steps in it. The commands' grammar and dispatch stay shared.
pub(crate) mod doctor;
pub(crate) mod init;
pub(crate) mod open;
pub(crate) mod setup;
pub(crate) mod switch;
pub(crate) mod update;
