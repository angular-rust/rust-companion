#![allow(unused_imports)]
use std::{fs, net::UdpSocket, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use companion::{companion_addr, Response, Task};

fn main() {
    let addr = companion_addr();

    let socket = UdpSocket::bind("[::]:0").unwrap();
    socket.connect(addr).unwrap();

    let mut buf = [0; 65507];

    socket.send(&Task::List.as_bytes()).unwrap();

    let (len, _src) = socket.recv_from(&mut buf).unwrap();
    let resp = Response::from(&buf[..len]);

    println!("{resp:?}")
}
