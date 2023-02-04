#![allow(unused_imports)]
use serde::{Deserialize, Serialize};

use rust_companion::{channel, ensure_companion, get_path, Response, Sender, Task};

fn main() {
    dotenv::dotenv().ok();

    let path = get_path();

    ensure_companion(&path).unwrap();

    if path.exists() {
        let sender = Sender::<Task>::connect(&path).unwrap();

        let (tx, rx) = channel::<Response>().unwrap();

        sender.send(Task::Sum(vec![23, 42], tx)).unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        let (tx, rx) = channel().unwrap();
        sender.send(Task::Sum((0..10).collect(), tx)).unwrap();
        println!("result: {:?}", rx.recv().unwrap());

        sender.send(Task::Shutdown).unwrap();
    } else {
        panic!("rust-companion cant start");
    }
}
