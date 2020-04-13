use async_std::{
    io,
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use futures_util::future::join_all;
use serde::Deserialize;
use std::io::ErrorKind;
use std::sync::Arc;

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

async fn process_incoming(upstream_addrs: Arc<Vec<String>>, mut client_stream: TcpStream) {
    let join_handles = upstream_addrs.iter().map(|addr| {
        let addr_str = String::from(addr);
        async move {
            println!("connecting to {}", addr_str);
            let target = TcpStream::connect(addr_str)
                .await
                .expect("Error connection to target!");
            target
        }
    });

    let mut upstreams: Vec<TcpStream> = join_all(join_handles).await.into_iter().collect();

    let mut clone_client = client_stream.clone();
    let mut clone_target = upstreams[0].clone();

    task::spawn(async move { io::copy(&mut clone_target, &mut clone_client).await });

    for upstream in &upstreams[1..] {
        let mut clone_tx = upstream.clone();
        task::spawn(async move { io::copy(&mut clone_tx, &mut io::sink()).await });
    }

    loop {
        let mut buf = [0 as u8; 255];
        let len = match client_stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let handles: Vec<_> = upstreams
            .iter_mut()
            .map(|upstream| upstream.write_all(&buf[..len]))
            .collect();
        join_all(handles).await;
    }
}

pub async fn start(config: Config) {
    let listener = TcpListener::bind(config.listen_addr)
        .await
        .expect("unable to bind address");
    println!("Start Listening on {}", listener.local_addr().unwrap());
    let upstreams = Arc::new(config.upstreams);
    loop {
        let (client, client_addr) = listener.accept().await.unwrap();
        println!("incoming connection from: {}", client_addr);
        let cloned_upstreams = upstreams.clone();
        task::spawn(async move {
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
    use async_std::{net::Shutdown, task};
    use futures_util::future;
    use futures_util::io::BufReader;
    use std::time::Duration;

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
        println!("start listening dummy: {}", listen_addr);
        let listener = TcpListener::bind(listen_addr).await.unwrap();
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = BufReader::new(&stream);
        let mut payload = String::new();
        buffer.read_line(&mut payload).await.unwrap();
        println!("payload incoming: {}", payload);
        stream.write_all(payload.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    }

    #[test]
    fn proxy_tcp_request() {
        task::block_on(async {
            let config = Config::from_config_str(&CONF_STR);
            let thr1 = task::spawn(async {
                dummy_tcp_service("127.0.0.1:8000").await;
            });
            let thr2 = task::spawn(async {
                dummy_tcp_service("127.0.0.1:8001").await;
            });
            let thr = task::spawn(async { start(config).await });
            task::sleep(Duration::from_millis(100)).await;
            let mut stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();
            stream.write_all("hello\n".as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut result = String::new();
            stream.read_to_string(&mut result).await.unwrap();
            assert_eq!(result, "hello\n");
            future::join3(thr, thr1, thr2).await;
        });
    }
}
