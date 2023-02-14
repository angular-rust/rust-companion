//! This crate implements a minimal abstraction over UNIX domain sockets for
//! the purpose of IPC.  It lets you send both file handles and rust objects
//! between processes.
//!
//! ```
//! fn main() {
//!     match companion::bootstrap() {
//!         Ok(val) => println!("launched"),
//!         Err(_) => println!("already launched"),
//!     }
//! }
//! ```
use std::{
    collections::HashMap,
    env, fs,
    os::unix::{
        io::{FromRawFd, IntoRawFd},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use sysinfo::{Pid, PidExt, SystemExt};

#[cfg(feature = "log")]
use log::*;
#[cfg(feature = "log")]
use syslog::{BasicLogger, Facility, Formatter3164};

use serde::{Deserialize, Serialize};

mod raw_channel;
pub use self::raw_channel::*;

mod serialize;
pub use self::serialize::*;

mod typed_channel;
pub use self::typed_channel::*;

pub(crate) const ENV_VAR: &str = "RUST_COMPANION";
pub(crate) const PROGRAM_NAME: &str = "rust-companion";

#[cfg(feature = "log")]
fn setup_logger() {
    use companion::PROGRAM_NAME;

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

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    String(String),
    List(Vec<String>),
    Ok,
    NotFound,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Task {
    // Get data by name
    Get(String, Sender<Response>),
    // Store data by name
    Set(String, String, Sender<Response>),
    // List stored names
    List(Sender<Response>),
    Sum(Vec<i64>, Sender<Response>),
    Shutdown,
}

pub fn socket_path() -> PathBuf {
    if let Ok(path) = env::var(ENV_VAR) {
        path.into()
    } else {
        let mut dir = std::env::temp_dir();
        dir.push(&format!("{}.sock", PROGRAM_NAME));
        dir
    }
}

pub fn pid_path() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(&format!("{}.pid", PROGRAM_NAME));
    dir
}

fn check_started<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    match fs::read_to_string(&path) {
        Ok(pids) => {
            // println!("pids: {pids}");
            let sys = sysinfo::System::new_all();
            let processes = sys.processes();
            let pids: Vec<u32> = pids.lines().filter_map(|s| s.parse::<u32>().ok()).collect();
            let mut started = false;
            let mut new_pids = vec![];
            for pid in pids.iter() {
                if processes.contains_key(&Pid::from_u32(*pid)) {
                    started = true;
                    new_pids.push(*pid);
                }
            }

            if started {
                let contents = new_pids
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join("\n");

                fs::write(&path, contents).unwrap();
                return true;
            }
        }
        Err(_) => {}
    }
    false
}

fn launch<P>(path: P)
where
    P: AsRef<Path>,
{
    #[cfg(feature = "log")]
    setup_logger();

    let pid = std::process::id();

    fs::write(path, pid.to_string()).unwrap();

    let socket_path = socket_path();

    let mut storage: HashMap<String, String> = HashMap::new();

    // println!("{:?}", socket_path);

    fs::remove_file(&socket_path).ok();
    let listener = UnixListener::bind(&socket_path).unwrap();
    'outer: loop {
        let (sock, _) = listener.accept().unwrap();
        let receiver: Receiver<Task> =
            unsafe { RawReceiver::from_raw_fd(sock.into_raw_fd()).into() };

        std::thread::sleep(std::time::Duration::from_millis(50));
        loop {
            let task = match receiver.recv() {
                Ok(task) => task,
                Err(_) => {
                    // break to wait a new connection
                    break;
                }
            };
            std::thread::sleep(std::time::Duration::from_millis(50));
            match task {
                Task::Get(key, tx) => {
                    #[cfg(feature = "log")]
                    log::info!("get {}", key);
                    match storage.get(&key) {
                        Some(data) => tx.send(Response::String(data.clone())).unwrap(),
                        None => tx.send(Response::NotFound).unwrap(),
                    }
                }
                Task::Set(key, data, tx) => {
                    #[cfg(feature = "log")]
                    log::info!("set {}", key);
                    storage.insert(key, data);
                    tx.send(Response::Ok).unwrap();
                }
                Task::List(tx) => {
                    let keys: Vec<String> = storage.keys().map(Clone::clone).collect();
                    tx.send(Response::List(keys)).unwrap();
                }
                Task::Sum(_values, tx) => {
                    #[cfg(feature = "log")]
                    log::info!("shutdown");
                    tx.send(Response::NotFound).unwrap();
                }
                Task::Shutdown => {
                    #[cfg(feature = "log")]
                    log::info!("shutdown");
                    break 'outer;
                }
            }
        }
    }
    fs::remove_file(&socket_path).ok();
}

pub fn bootstrap() -> std::result::Result<bool, Box<dyn std::error::Error>> {
    let pid_path = pid_path();

    if !check_started(&pid_path) {
        match env::args().nth(1) {
            Some(arg) => {
                if arg == "-d" {
                    launch(&pid_path);
                }
            }
            None => {
                match env::current_exe() {
                    Ok(exe_path) => {
                        let _child = std::process::Command::new(&exe_path)
                            .arg("-d")
                            .stderr(Stdio::null())
                            .stdout(Stdio::null())
                            .spawn()?;
                        std::thread::sleep(Duration::from_micros(50));
                    }
                    Err(e) => println!("failed to get current exe path: {e}"),
                };
            }
        }
    }

    Ok(true)
}
