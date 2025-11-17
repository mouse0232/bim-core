use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use log::debug;

use url::Url;
use tokio::time::timeout;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as AsyncTcpStream;
use tokio_rustls::{TlsConnector, rustls::{ClientConfig, pki_types::ServerName, RootCertStore}};

use crate::clients::base::{get_address_with_fallback, Client, LoadCounter};
use crate::utils::SpeedTestResult;

#[allow(dead_code)]
pub struct AsyncHTTPClient {
    download_url: Url,
    upload_url: Url,
    threads: u8,
    force_ipv4: bool,
    force_ipv6: bool,
    address: std::net::SocketAddr,
    upload: f64,
    upload_status: String,
    download: f64,
    download_status: String,
    latency: f64,
    jitter: f64,
}

impl AsyncHTTPClient {
    pub fn build(
        download_url: String,
        upload_url: String,
        force_ipv4: bool,
        force_ipv6: bool,
        threads: u8,
    ) -> Option<Box<dyn Client>> {
        let download_url = Url::parse(&download_url).ok()?;
        let upload_url = Url::parse(&upload_url).ok()?;

        let address = get_address_with_fallback(&download_url, force_ipv4, force_ipv6)?;

        #[cfg(debug_assertions)]
        debug!("IP address {address}");

        let r = "取消".to_owned();
        Some(Box::new(Self {
            download_url,
            upload_url,
            threads,
            force_ipv4,
            force_ipv6,
            address,
            upload: 0.0,
            upload_status: r.clone(),
            download: 0.0,
            download_status: r.clone(),
            latency: 0.0,
            jitter: 0.0,
        }))
    }

    async fn run_load(&mut self, load: u8) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let url = match load {
            0 => self.upload_url.clone(),
            _ => self.download_url.clone(),
        };
        
        let counter = Arc::new(LoadCounter::new(self.threads));
        let mut tasks = vec![];

        for _ in 0..self.threads {
            let a = self.address.clone();
            let u = url.clone();
            let c = counter.clone();

            let task = tokio::spawn(async move {
                match load {
                    0 => Self::request_http_upload(a, u, c).await,
                    _ => Self::request_http_download(a, u, c).await,
                };
            });
            tasks.push(task);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }

        let mut time_passed = 0;
        counter.wait();

        let start = Instant::now();
        while time_passed < 14_000_000 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            time_passed = start.elapsed().as_micros();

            counter.count(time_passed);
        }

        counter.end();
        for task in tasks {
            let _ = task.await;
        }

        match load {
            0 => {
                self.upload = counter.speed();
                self.upload_status = counter.status();
            }
            _ => {
                self.download = counter.speed();
                self.download_status = counter.status();
            }
        }

        Ok(true)
    }

    async fn request_http_download(
        address: std::net::SocketAddr,
        url: Url,
        counter: Arc<LoadCounter>,
    ) {
        let chunk_count = 50;
        let data_size = chunk_count * 1024 * 1024 as u64;
        let mut data_counter;
        let mut buffer = [0; 65536];

        let host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let path_str = url.path();

        let mut stream = match Self::make_connection(address, &url).await {
            Ok(s) => s,
            Err(_) => {
                counter.wait();
                return;
            }
        };

        counter.wait();

        'request: while !counter.is_end() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let path_query = format!(
                "{}?cors=true&r={}&ckSize={}&size={}",
                path_str, now, chunk_count, data_size
            );

            #[cfg(debug_assertions)]
            debug!("Download {path_query}");

            let request_head = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: bim/1.0\r\n\r\n",
                path_query, host_port,
            );

            match timeout(Duration::from_secs(10), stream.write_all(request_head.as_bytes())).await {
                Ok(Ok(_)) => {
                    match timeout(Duration::from_secs(10), stream.read(&mut buffer)).await {
                        Ok(Ok(size)) => {
                            #[cfg(debug_assertions)]
                            debug!("Download Status: {size}");

                            if size > 0 {
                                let count = size as u64;
                                data_counter = count;
                                counter.increase(count);
                            } else {
                                break 'request;
                            }
                        }
                        Ok(Err(_)) => break 'request,
                        Err(_) => break 'request,
                    }
                }
                Ok(Err(_)) => {
                    #[cfg(debug_assertions)]
                    debug!("Download Error: timeout or other error");
                    break 'request;
                }
                Err(_) => {
                    #[cfg(debug_assertions)]
                    debug!("Download Error: timeout");
                    break 'request;
                }
            }

            while data_counter < data_size && !counter.is_end() {
                match timeout(Duration::from_secs(10), stream.read(&mut buffer)).await {
                    Ok(Ok(size)) => {
                        let count = size as u64;
                        data_counter += count;
                        counter.increase(count);
                    }
                    Ok(Err(_)) => {
                        #[cfg(debug_assertions)]
                        debug!("Download Error: connection error");

                        break 'request;
                    }
                    Err(_) => {
                        #[cfg(debug_assertions)]
                        debug!("Download Error: timeout");

                        break 'request;
                    }
                }
            }
        }
    }

    async fn request_http_upload(
        address: std::net::SocketAddr,
        url: Url,
        counter: Arc<LoadCounter>,
    ) {
        let chunk_count = 50;
        let data_size = chunk_count * 1024 * 1024 as u64;
        let mut data_counter;

        let host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let url_path = url.path();
        let request_chunk = "0123456789AaBbCcDdEeFfGgHhIiJjKkLlMmNnOoPpQqRrSsTtUuVvWwXxYyZz-="
            .repeat(1024)
            .into_bytes();

        let mut stream = match Self::make_connection(address, &url).await {
            Ok(s) => s,
            Err(_) => {
                counter.wait();
                return;
            }
        };

        counter.wait();

        'request: while !counter.is_end() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let path_query = format!("{}?r={}", url_path, now);

            #[cfg(debug_assertions)]
            debug!("Upload {path_query} size {data_size}");

            let request_head = format!(
                "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: bim/1.0\r\nContent-Length: {}\r\n\r\n",
                path_query, host_port, data_size
            );

            match timeout(Duration::from_secs(10), stream.write_all(request_head.as_bytes())).await {
                Ok(Ok(_)) => {
                    let length = request_head.len() as u64;
                    data_counter = length;
                    counter.increase(length);
                }
                Ok(Err(_)) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload Error: connection error");
                    break 'request;
                }
                Err(_) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload Error: timeout");
                    break 'request;
                }
            }

            while data_counter < data_size && !counter.is_end() {
                match timeout(Duration::from_secs(10), stream.write(&request_chunk)).await {
                    Ok(Ok(size)) => {
                        let count = size as u64;
                        data_counter += size as u64;
                        counter.increase(count);
                    }
                    Ok(Err(_)) => {
                        #[cfg(debug_assertions)]
                        debug!("Upload Error: connection error");
                        break 'request;
                    }
                    Err(_) => {
                        #[cfg(debug_assertions)]
                        debug!("Upload Error: timeout");
                        break 'request;
                    }
                }
            }
        }
    }

    async fn make_connection(
        address: std::net::SocketAddr,
        url: &Url,
    ) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, Box<dyn std::error::Error + Send + Sync>> {
        let ssl = url.scheme() == "https";

        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        
        let tcp_stream = timeout(
            Duration::from_secs(5),
            AsyncTcpStream::connect(address)
        ).await??;

        tcp_stream.set_nodelay(true).ok();

        if !ssl {
            return Err("Non-SSL connections not supported in async client".into());
        }

        let domain = ServerName::try_from(url.host_str().unwrap().to_string())?;
        let tls_stream = timeout(
            Duration::from_secs(5),
            connector.connect(domain, tcp_stream)
        ).await??;

        Ok(tls_stream)
    }
}

impl Client for AsyncHTTPClient {
    fn ping(&mut self) -> bool {
        // 在实际的异步实现中，这里应该也是异步的
        // 为保持接口一致，暂时使用同步实现
        let mut count = 0;
        let mut pings = [0u128; 6];
        let mut ping_min = 10000000;

        while count < 6 {
            let ping = crate::clients::base::request_tcp_ping(&self.address);
            if ping > 0 {
                if ping < ping_min {
                    ping_min = ping
                }
                pings[count] = ping;
            }
            std::thread::sleep(Duration::from_millis(1000));
            count += 1;
        }

        if pings == [0, 0, 0, 0, 0, 0] {
            self.latency = 0.0;
            self.jitter = 0.0;
            return false;
        }

        let mut jitter_all = 0;
        for p in pings {
            if p > 0 {
                jitter_all += p - ping_min;
            }
        }

        self.latency = ping_min as f64 / 1_000.0;
        self.jitter = jitter_all as f64 / 5_000.0;

        #[cfg(debug_assertions)]
        debug!("Ping {} ms", self.latency);

        #[cfg(debug_assertions)]
        debug!("Jitter {} ms", self.jitter);

        true
    }

    fn download(&mut self) -> bool {
        // 在实际的异步实现中，这里应该也是异步的
        // 为保持接口一致，暂时使用同步实现调用异步函数
        let rt = tokio::runtime::Runtime::new().unwrap();
        match rt.block_on(self.run_load(1)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    fn upload(&mut self) -> bool {
        // 在实际的异步实现中，这里应该也是异步的
        // 为保持接口一致，暂时使用同步实现调用异步函数
        let rt = tokio::runtime::Runtime::new().unwrap();
        match rt.block_on(self.run_load(0)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    fn result(&self) -> SpeedTestResult {
        SpeedTestResult::build(
            self.upload,
            self.upload_status.clone(),
            self.download,
            self.download_status.clone(),
            self.latency,
            self.jitter,
        )
    }
}