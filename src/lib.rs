//! Granny vs. The Neighborhood: a 3D zombie survival brawler for Omarchy.

// Bevy systems routinely take many parameters and filtered queries.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod game;
pub mod palette;
pub mod sim;

pub use game::run as run_game;
