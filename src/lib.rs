#![allow(unused_imports)]
#![allow(unreachable_code)]
#![allow(unused_variables)]
//! This crate implements a minimal abstraction over Udp/UNIX domain sockets for
//! the purpose of IPC.  It lets you send both file handles and rust objects
//! between processes.
//!
//! ```
//! fn main() {
//!     match companion::bootstrap() {
//!         Ok(lock) => {
//!             println!("cargo:rerun-if-changed={:?}", lock);
//!         },
//!         Err(_) => println!("already launched"),
//!     }
//! }
//! ```
use std::{
    collections::HashMap,
    env, fs,
    net::UdpSocket,
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

impl Response {
    pub fn as_bytes(&self) -> Vec<u8> {
        bincode::serialize(&self).unwrap()
    }
}

impl From<&[u8]> for Response {
    fn from(bytes: &[u8]) -> Self {
        bincode::deserialize(bytes).unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Task<'a> {
    // Get data by name
    Get(&'a str),
    // Store data by name
    Set(&'a str, &'a str),
    // List stored names
    List,
    Sum(Vec<i64>),
    Shutdown,
}

impl<'a> Task<'a> {
    pub fn as_bytes(&self) -> Vec<u8> {
        bincode::serialize(&self).unwrap()
    }
}

pub fn companion_addr() -> String {
    if let Ok(addr) = env::var(ENV_VAR) {
        addr
    } else {
        // let mut dir = std::env::temp_dir();
        // dir.push(&format!("{}.sock", PROGRAM_NAME));
        // dir
        "[::]:2000".into()
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

pub fn launch<P>(path: P)
where
    P: AsRef<Path>,
{
    #[cfg(feature = "log")]
    setup_logger();

    let pid = std::process::id();

    fs::write(path, pid.to_string()).unwrap();

    let socket_path = companion_addr();

    let mut storage: HashMap<String, String> = HashMap::new();

    let sock = UdpSocket::bind(&socket_path).unwrap();

    'outer: loop {
        let mut buf = [0; 65507];
        let sock = sock.try_clone().expect("Failed to clone socket");

        let (len, src) = sock.recv_from(&mut buf).unwrap();
        let buf = &mut buf[..len];

        let task: Task = bincode::deserialize(&buf).unwrap();
        println!("{task:?}");
        match task {
            Task::Get(key) => {
                #[cfg(feature = "log")]
                log::info!("get {}", key);
                match storage.get(key.into()) {
                    Some(data) => {
                        let buf = bincode::serialize(&Response::String(data.clone())).unwrap();
                        sock.send_to(&buf, src).unwrap();
                    }
                    None => {
                        let buf = bincode::serialize(&Response::NotFound).unwrap();
                        sock.send_to(&buf, src).unwrap();
                    }
                }
            }
            Task::Set(key, data) => {
                #[cfg(feature = "log")]
                log::info!("set {}", key);
                storage.insert(key.into(), data.into());
                let buf = bincode::serialize(&Response::Ok).unwrap();
                sock.send_to(&buf, src).unwrap();
            }
            Task::List => {
                let keys: Vec<String> = storage.keys().map(Clone::clone).collect();
                let buf = bincode::serialize(&Response::List(keys)).unwrap();
                sock.send_to(&buf, src).unwrap();
            }
            Task::Sum(_values) => {
                #[cfg(feature = "log")]
                log::info!("shutdown");
                // tx.send(Response::NotFound).unwrap();
            }
            Task::Shutdown => {
                #[cfg(feature = "log")]
                log::info!("shutdown");
                break 'outer;
            }
        }
    }
}

pub fn lockfile() -> String {
    let mut path = PathBuf::new();
    // from outdir
    let source = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut prev = String::new();
    for part in source.iter() {
        if prev == "target" && (part == "debug" || part == "release") {
            break;
        }
        prev = part.to_string_lossy().into();
        path.push(part);
    }
    path.push("companion.lock");
    path.as_os_str().to_string_lossy().into()
}

pub fn bootstrap() -> std::result::Result<String, Box<dyn std::error::Error>> {
    let pid_path = pid_path();

    let lockfile = lockfile();

    if !check_started(&pid_path) {
        match env::args().nth(1) {
            Some(arg) => {
                if arg == "-d" {
                    launch(&pid_path);
                }
            }
            None => {
                match env::current_exe() {
                    Ok(exe) => {
                        let _child = std::process::Command::new(&exe)
                            .arg("-d")
                            .stderr(Stdio::null())
                            .stdout(Stdio::null())
                            .spawn()?;
                        std::thread::sleep(Duration::from_micros(50));
                        // write lock file
                        let exe = exe.as_os_str().to_string_lossy().to_string();
                        std::fs::write(&lockfile, format!("{exe}")).unwrap();
                    }
                    Err(e) => println!("failed to get current exe path: {e}"),
                };
            }
        }
    }

    Ok(lockfile)
}
