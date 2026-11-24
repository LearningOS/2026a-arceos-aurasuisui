#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[cfg(feature = "axstd")]
use axstd::println;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    // ANSI SGR: bright green text, reset afterwards.
    println!("\x1b[1;32m[WithColor]: Hello, Arceos!\x1b[0m");
}
