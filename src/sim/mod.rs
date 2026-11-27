//! Engine-agnostic game rules. Nothing here touches rendering, windows or files;
//! the Bevy layer drives `World::step` at 60 Hz and reads its state and events.

pub mod data;
pub mod layout;
pub mod rng;
pub mod save;
pub mod world;
