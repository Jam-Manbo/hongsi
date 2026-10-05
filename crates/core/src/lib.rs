
pub mod api_error;
pub mod attendance;
pub mod calendar;
pub mod classroom;
pub mod error;
pub mod food;
pub mod models;
pub mod seats;
mod session;
pub mod submission;
pub mod timetable;
mod util;

pub use error::{CoreError, Result};
pub use session::{public_client, SchoolSession, SchoolSessionSnapshot};
