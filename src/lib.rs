use futures_util::future::join_all;
use serde::Deserialize;
use std::io::ErrorKind;
use std::sync::Arc;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    prelude::*,
};

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

async fn process_incoming(upstream_addrs: Arc<Vec<String>>, client_stream: TcpStream) {
    let join_handles: Vec<_> = upstream_addrs
        .iter()
        .map(|addr| {
            let addr_str = String::from(addr);
            tokio::spawn(async move {
                println!("connecting to {}", addr_str);
                let target = TcpStream::connect(addr_str)
                    .await
                    .expect("Error connection to target!");
                io::split(target)
            })
        })
        .collect();

    let (mut reads, mut writes): (Vec<_>, Vec<_>) = join_all(join_handles)
        .await
        .into_iter()
        .map(|u| u.unwrap())
        .unzip();

    let (mut client_read, mut client_write) = io::split(client_stream);
    let mut head = reads.swap_remove(0);

    tokio::spawn(async move { io::copy(&mut head, &mut client_write).await });

    for mut upstream in reads {
        tokio::spawn(async move { io::copy(&mut upstream, &mut io::sink()).await });
    }

    loop {
        let mut buf = [0 as u8; 255];
        let len = match client_read.read(&mut buf).await {
            Ok(0) => break,
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let handles: Vec<_> = writes
            .iter_mut()
            .map(|upstream| upstream.write_all(&buf[..len]))
            .collect();
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
    use std::net::Shutdown;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, BufReader};

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

    async fn dummy_tcp_service(listen_addr: &str) {
        let mut listener = TcpListener::bind(listen_addr).await.unwrap();
        let (mut stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.split();
        let mut buffer = BufReader::new(read);
        let mut payload = String::new();
        println!("start listening dummy");
        buffer.read_line(&mut payload).await.unwrap();
        println!("payload incoming: {}", payload);
        write.write_all(payload.as_bytes()).await.unwrap();
        write.flush().await.unwrap();
    }

    #[tokio::test]
    async fn proxy_tcp_request() {
        let config = Config::from_config_str(&CONF_STR);
        let thr1 = tokio::spawn(async {
            dummy_tcp_service("127.0.0.1:8000").await;
        });
        let thr2 = tokio::spawn(async {
            dummy_tcp_service("127.0.0.1:8001").await;
        });
        let thr = tokio::spawn(async { start(config).await });
        tokio::time::delay_for(Duration::from_millis(100)).await;

        let mut stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();
        stream.write_all("hello\n".as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut result = String::new();
        stream.read_to_string(&mut result).await.unwrap();
        assert_eq!(result, "hello\n");
        let _ = tokio::join!(thr, thr1, thr2);
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
