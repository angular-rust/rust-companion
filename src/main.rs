use std::{
    collections::HashMap,
    fs,
    os::unix::io::{FromRawFd, IntoRawFd},
    os::unix::net::UnixListener,
};

use rust_companion::{get_path, Response, Task};

#[allow(unused_imports)]
use rust_companion::PROGRAM_NAME;

#[cfg(feature = "log")]
use log::*;
#[cfg(feature = "log")]
use syslog::{BasicLogger, Facility, Formatter3164};

use rust_companion::{RawReceiver, Receiver};

#[cfg(feature = "log")]
fn setup_logger() {
    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: PROGRAM_NAME.into(),
        pid: 0,
    };

    let logger = syslog::unix(formatter).expect("could not connect to syslog");
    log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("could not register logger");
}

fn main() {
    dotenv::dotenv().ok();

    #[cfg(feature = "log")]
    setup_logger();

    let path = get_path();

    #[allow(unused_variables)]
    let storage: HashMap<String, Vec<u8>> = HashMap::new();

    println!("{:?}", path);

    fs::remove_file(&path).ok();
    let listener = UnixListener::bind(&path).unwrap();
    'outer: loop {
        let (sock, _) = listener.accept().unwrap();
        let receiver: Receiver<Task> =
            unsafe { RawReceiver::from_raw_fd(sock.into_raw_fd()).into() };

        std::thread::sleep(std::time::Duration::from_millis(50));
        loop {
            let task = receiver.recv().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            match task {
                Task::Get(_name, _tx) => {
                    println!("Get")
                }
                Task::Set(_name, _data, _tx) => {
                    println!("Set")
                }
                Task::Sum(_values, tx) => {
                    // tx.send(values.into_iter().sum::<i64>()).await.unwrap();
                    println!("Sum");
                    tx.send(Response::NotFound).unwrap();
                }
                Task::Shutdown => {
                    println!("shutdown");
                    break 'outer;
                }
            }
        }
    }
}
