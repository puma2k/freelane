use std::io::{Read,Write};
use std::time::Duration;
fn main() {
    let Some(mode)=std::env::var_os("FREELANE_WORKER_FIXTURE") else{return};
    if mode=="unrelated" {loop{std::thread::park_timeout(Duration::from_secs(30));}}
    let _socket=std::env::var("FREELANE_FIXTURE_SOCKET").ok().map(|address|std::net::TcpStream::connect(address).unwrap());
    let mut input=std::io::stdin();let mut setup=[0;80];input.read_exact(&mut setup).unwrap();
    if mode=="stall" {loop{std::thread::park_timeout(Duration::from_secs(30));}}
    if mode=="early" {std::process::exit(0);}
    let mut output=std::io::stdout();output.write_all(if mode=="wrong" {b"Q"}else{b"R"}).unwrap();output.flush().unwrap();
    if mode=="wrong" {std::process::exit(0);}
    let mut byte=[0];input.read_exact(&mut byte).unwrap();assert_eq!(&byte,b"G");
    output.write_all(b"S").unwrap();output.flush().unwrap();
    // Never acknowledge K: coalescing must leave only one byte in the pipe.
    input.read_exact(&mut byte).unwrap();assert_eq!(&byte,b"K");
    loop{let n=input.read(&mut byte).unwrap();if n==0{break;}assert_ne!(&byte,b"K","duplicate pending K");}
    std::process::exit(0);
}
