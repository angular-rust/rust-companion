#![allow(unused_variables)]
use companion::{launch, pid_path};

fn main() {
    let pid_path = pid_path();
    launch(&pid_path)
}
