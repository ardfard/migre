use futures_util::future::join_all;
use serde::Deserialize;
use std::future::Future;
use std::io::ErrorKind;
use std::sync::Arc;
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::task::JoinHandle;

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

async fn process_incoming(upstream_addrs: Arc<Vec<String>>, mut client_stream: TcpStream) {
    let mut join_handles = Vec::new();
    let (mut client_read, mut client_write) = client_stream.split();
    for addr in upstream_addrs.iter() {
        let upstream = String::from(addr);
        join_handles.push(tokio::spawn(async {
            let target = TcpStream::connect(upstream).await.unwrap();
            target
        }));
    }

    let mut write_halfes: Vec<TcpStream> = join_all(join_handles)
        .await
        .into_iter()
        .map(|x| x.unwrap())
        .collect();

    loop {
        let mut buf = [0 as u8; 255];
        let len = match client_read.read(&mut buf).await {
            Ok(0) => break,
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let mut handles = Vec::with_capacity(write_halfes.len());
        for upstream in write_halfes.iter_mut() {
            handles.push(upstream.write_all(&buf[..len]));
        }
        join_all(handles).await;
    }
}

pub async fn start(config: Config) {
    let mut listener = TcpListener::bind(config.listen_addr)
        .await
        .expect("unable to bind address");
    println!("Start Listening on {}", listener.local_addr().unwrap());
    let upstreams = Arc::new(config.upstreams);
    loop {
        let (client, client_addr) = listener.accept().await.unwrap();
        println!("incoming connection from: {}", client_addr);
        let cloned_upstreams = upstreams.clone();
        tokio::spawn(async move {
            process_incoming(cloned_upstreams, client).await;
        });
        if config.run_once == Some(true) {
            break;
        }
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
        let listener = TcpListener::bind(listen_addr);
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
