use failure;

#[allow(dead_code)]
pub mod cfg;

pub type CargoResult<T> = failure::Fallible<T>;
