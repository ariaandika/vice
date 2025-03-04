use std::io;
use vice_rc::{
    http::{self, service::HttpService},
    runtime::listen_blocking,
};

fn main() -> io::Result<()> {
    env_logger::init();
    let service = HttpService::new(http::debug::Debug);
    listen_blocking("0.0.0.0:3000", service)
}

