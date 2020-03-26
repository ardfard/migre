use serde::Deserialize;
#[allow(unused_imports)]
use std::io::{Read, Write};

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Deserialize)]
pub struct Config {
    upstreams: Vec<String>,
    listen_addr: String,
    run_once: Option<bool>,
}

impl Config {
    pub fn from_config_str(conf: &str) -> Self {
        toml::from_str(conf).unwrap()
    }
}

pub fn start(config: Config) {
    let listener = TcpListener::bind(config.listen_addr).expect("unable to bind address");
    println!("Start Listening on {}", listener.local_addr().unwrap());
    let upstreams_arc = Arc::new(config.upstreams.clone());
    loop {
        let (client, client_addr) = listener.accept().unwrap();
        let upstreams = upstreams_arc.clone();
        println!("incoming connection from: {}", client_addr);
        thread::spawn(move || {
            handle_conn(client, &upstreams);
        });
        if config.run_once == Some(true) {
            break;
        }
    }
}

fn handle_conn(from: TcpStream, upstreams: &Vec<String>) {
    let from_arc = Arc::new(from);
    let mut connections = Vec::with_capacity(upstreams.len() * 2);
    for upstream in upstreams.iter() {
        let to_arc = match TcpStream::connect(upstream) {
            Ok(stream) => Arc::new(stream),
            Err(err) => {
                println!("Failed to connect to {}: {}", upstream, err.to_string());
                continue;
            }
        };
        println!("connected to {}", upstream);

        let (mut lhs_tx, mut lhs_rx) =
            (from_arc.try_clone().unwrap(), from_arc.try_clone().unwrap());
        let (mut rhs_tx, mut rhs_rx) = (to_arc.try_clone().unwrap(), to_arc.try_clone().unwrap());

        connections.push(thread::spawn(move || {
            std::io::copy(&mut lhs_tx, &mut rhs_rx).unwrap()
        }));
        connections.push(thread::spawn(move || {
            std::io::copy(&mut rhs_tx, &mut lhs_rx).unwrap()
        }));
    }

    for t in connections {
        t.join().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::Shutdown;

    static CONF_STR: &str = r#"
        listen_addr = "127.0.0.1:12345"
        upstreams = ["127.0.0.1:8000", "127.0.0.1:8001"]
        run_once = true
    "#;

    #[test]
    fn can_create_config() {
        let config = Config::from_config_str(&CONF_STR);
        assert_eq!(config.listen_addr, "127.0.0.1:12345");
        assert_eq!(config.upstreams[0], "127.0.0.1:8000");
        assert_eq!(config.run_once, Some(true));
    }

    fn dummy_tcp_service(listen_addr: &str) {
        let listener = TcpListener::bind(listen_addr).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = BufReader::new(&stream);
        let mut payload = String::new();
        println!("start listening dummy");
        buffer.read_line(&mut payload).unwrap();
        println!("payload incoming: {}", payload);
        stream.write_all(payload.as_bytes()).unwrap();
        stream.flush().unwrap();
        stream.shutdown(Shutdown::Both).unwrap();
    }

    #[test]
    fn proxy_tcp_request() {
        let config = Config::from_config_str(&CONF_STR);
        let thr2 = std::thread::spawn(move || {
            dummy_tcp_service("127.0.0.1:8000");
        });
        std::thread::sleep(std::time::Duration::from_millis(10));
        let thr = std::thread::spawn(move || start(config));
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut stream = TcpStream::connect("127.0.0.1:12345").unwrap();
        stream.write_all("hello\n".as_bytes()).unwrap();
        stream.flush().unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut result = String::new();
        stream.read_to_string(&mut result).unwrap();
        assert_eq!(result, "hello\n");
        thr.join().unwrap();
        thr2.join().unwrap();
    }
}
