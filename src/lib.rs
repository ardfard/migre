use serde::Deserialize;
#[allow(unused_imports)]
use std::io::{sink, Cursor, Read, Write};

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use threadpool::ThreadPool;

#[derive(Debug, Deserialize)]
pub struct Config {
    upstreams: Vec<String>,
    listen_addr: String,
    run_once: Option<bool>,
    worker_pool_size: Option<usize>,
}

impl Config {
    pub fn from_config_str(conf: &str) -> Self {
        toml::from_str(conf).unwrap()
    }
}

pub fn start(config: Config) {
    let listener = TcpListener::bind(config.listen_addr).expect("unable to bind address");
    println!("Start Listening on {}", listener.local_addr().unwrap());
    let num_worker = config
        .worker_pool_size
        .unwrap_or_else(|| num_cpus::get() + 1);
    let pool = ThreadPool::new(num_worker);
    loop {
        let (mut client, client_addr) = listener.accept().unwrap();
        println!("incoming connection from: {}", client_addr);
        let upstreams: Vec<TcpStream> = config
            .upstreams
            .iter()
            .map(|upstream| {
                let target = TcpStream::connect(upstream).expect("Error connecting to target!");
                println!("connected to {}", upstream);
                target
            })
            .collect();

        let mut ori_rx = client.try_clone().unwrap();
        let mut target_rx = (&upstreams[0]).try_clone().unwrap();
        pool.execute(move || {
            std::io::copy(&mut target_rx, &mut ori_rx).unwrap();
        });

        for upstream in &upstreams[1..] {
            let mut clone_tx = upstream.try_clone().unwrap();
            pool.execute(move || {
                std::io::copy(&mut clone_tx, &mut sink()).unwrap();
            });
        }
        pool.execute(move || loop {
            let mut buf = [0 as u8; 255];
            let len = match client.read(&mut buf) {
                Ok(0) => break,
                Ok(len) => len,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            for mut upstream in upstreams.iter() {
                upstream.write_all(&buf[..len]).unwrap();
            }
        });
        if config.run_once == Some(true) {
            break;
        }
    }
    pool.join();
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
        worker_pool_size = 8
    "#;

    #[test]
    fn can_create_config() {
        let config = Config::from_config_str(&CONF_STR);
        assert_eq!(config.listen_addr, "127.0.0.1:12345");
        assert_eq!(config.upstreams[0], "127.0.0.1:8000");
        assert_eq!(config.run_once, Some(true));
        assert_eq!(config.worker_pool_size, Some(8));
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
    }

    #[test]
    fn proxy_tcp_request() {
        let config = Config::from_config_str(&CONF_STR);
        let thr1 = std::thread::spawn(move || {
            dummy_tcp_service("127.0.0.1:8000");
        });
        let thr2 = std::thread::spawn(move || {
            dummy_tcp_service("127.0.0.1:8001");
        });
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
        thr1.join().unwrap();
        thr2.join().unwrap();
    }

    #[test]
    fn test_pointer() {
        let v = vec![String::from("horeee")];
        let p = &v[0];
        for v1 in v.iter() {
            println!("{:p}", p);
            println!("{:p}", v1);
        }
    }
}
