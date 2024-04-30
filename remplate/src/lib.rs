pub trait Remplate: core::fmt::Display {
    const ESTIMATED_SIZE: usize;

    fn render(&self) -> Result<String, ::core::fmt::Error> {
        use std::fmt::Write;

        let mut rendered = ::std::string::String::with_capacity(Self::ESTIMATED_SIZE);
        rendered.write_fmt(format_args!("{}", self))?;

        Ok(rendered)
    }
}

pub use remplate_macros::Remplate;
