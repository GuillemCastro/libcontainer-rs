use libcontainer_rs::{container::Container, filesystem::OverlayDriver};
use color_eyre::{Result};
use simple_logger::SimpleLogger;

fn main() -> Result<()> {
    SimpleLogger::new().init()?;

    let fs = OverlayDriver::new(vec![&String::from("tests/alpine-3.15.3")], &String::from("./alpine-rootfs"));
    let mut container = Container::new(Box::new(fs))?;
    println!("Starting container");
    container.start()?;
    println!("Execute sh in container");
    container.execute_in_container(String::from("/bin/sh"), vec![], None, None)?;
    container.wait_for_container()?;
    Ok(())
}