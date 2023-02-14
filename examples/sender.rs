#![allow(unused_imports)]
use std::{fs, time::Duration};

use serde::{Deserialize, Serialize};

use companion::{channel, socket_path, Response, Sender, Task};

fn main() {
    let path = socket_path();

    if path.exists() {
        let sender = match Sender::<Task>::connect(&path) {
            Ok(s) => s,
            Err(_) => {
                // restart from companion crash
                fs::remove_file(&path).ok();
                panic!("Some wrong with rust-companion")
            }
        };

        let (tx, rx) = channel::<Response>().unwrap();

        sender.send(Task::Get("key".into(), tx)).unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        let (tx, rx) = channel().unwrap();
        sender
            .send(Task::Set("key".into(), "data".into(), tx))
            .unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        let (tx, rx) = channel().unwrap();
        sender.send(Task::List(tx)).unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        let (tx, rx) = channel().unwrap();
        sender.send(Task::Get("key".into(), tx)).unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        // sender.send(Task::Shutdown).unwrap();
    } else {
        panic!("rust-companion cant start");
    }
}
