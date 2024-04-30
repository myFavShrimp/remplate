pub trait Remplate: core::fmt::Display {
    fn render(&self) -> String {
        format!("{}", self)
    }
}

pub use remplate_macros::{remplate, Remplate};
