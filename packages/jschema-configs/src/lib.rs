pub mod pyproject {
    include!(concat!(env!("OUT_DIR"), "/pyproject.rs"));
}
pub mod deno {
    include!(concat!(env!("OUT_DIR"), "/deno.rs"));
}
pub mod package {
    include!(concat!(env!("OUT_DIR"), "/package.rs"));
}
