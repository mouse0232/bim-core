use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use log::debug;

use url::Url;

use crate::clients::base::{get_address_with_fallback, make_connection, Client, LoadCounter};
use crate::utils::SpeedTestResult;

use std::io::{Read, Write};
use std::time::SystemTime;

pub struct HTTPClient {
    download_url: Url,
    upload_url: Url,
    threads: u8,

    address: SocketAddr,

    upload: f64,
    upload_status: String,
    download: f64,
    download_status: String,
    latency: f64,
    jitter: f64,
}

impl HTTPClient {
    pub fn build(
        download_url: String,
        upload_url: String,
        force_ipv4: bool,
        force_ipv6: bool,
        threads: u8,
    ) -> Option<Box<dyn Client>> {
        let download_url = Url::parse(&download_url).ok()?;
        let upload_url = Url::parse(&upload_url).ok()?;

        // 获取地址，支持IPv4/IPv6容错
        let address = get_address_with_fallback(&download_url, force_ipv4, force_ipv6)?;

        #[cfg(debug_assertions)]
        debug!("IP address {address}");

        let r = "取消".to_owned();
        Some(Box::new(Self {
            download_url,
            upload_url,
            threads,
            address,
            upload: 0.0,
            upload_status: r.clone(),
            download: 0.0,
            download_status: r.clone(),
            latency: 0.0,
            jitter: 0.0,
        }))
    }

    fn run_load(&mut self, load: u8) -> Result<bool, Box<dyn std::error::Error>> {
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

            let task = thread::spawn(move || {
                match load {
                    0 => Self::request_http_upload(a, u, c),
                    _ => Self::request_http_download(a, u, c),
                };
            });
            tasks.push(task);
            thread::sleep(Duration::from_millis(250));
        }

        let mut time_passed = 0;
        counter.wait();

        let now = Instant::now();
        while time_passed < 14_000_000 {
            thread::sleep(Duration::from_millis(500));
            time_passed = now.elapsed().as_micros();

            counter.count(time_passed);
        }

        counter.end();
        for task in tasks {
            if let Err(_e) = task.join() {
                #[cfg(debug_assertions)]
                debug!("Task join error");
            }
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

    fn request_http_download(address: SocketAddr, url: Url, counter: Arc<LoadCounter>) {
        let chunk_count = 350;
        let data_size = chunk_count * 1024 * 1024 as u64;
        let host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let path_str = url.path();
        let _host_str = url.host_str().unwrap();

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                log::debug!("Failed to connect to proxy: {}", url);
                return;
            }
        };

        counter.wait();

        let mut data_counter: u64 = 0;
        let mut buffer = [0; 131072];

        while !counter.is_end() {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
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
            )
            .into_bytes();

            match stream.write_all(&request_head) {
                Ok(_) => {
                    match stream.read(&mut buffer) {
                        Ok(size) => {
                            #[cfg(debug_assertions)]
                            debug!("Download Status: {size}");

                            if size > 0 {
                                data_counter = size as u64;
                                counter.increase(data_counter);
                            } else {
                                break;
                            }
                        }
                        Err(_) => {
                            #[cfg(debug_assertions)]
                            debug!("Download read error");
                            break;
                        }
                    }
                }
                Err(_) => {
                    #[cfg(debug_assertions)]
                    debug!("Download write error");
                    break;
                }
            }

            while data_counter < data_size && !counter.is_end() {
                match stream.read(&mut buffer) {
                    Ok(size) => {
                        let _count = size as u64;
                        data_counter += _count;
                        counter.increase(_count);

                        if size == 0 {
                            #[cfg(debug_assertions)]
                            debug!("Download Error: Read failed");
                            break;
                        }
                    }
                    Err(_e) => {
                        log::debug!("Failed to read response: {}", url);
                        return;
                    }
                }
            }
        }
    }

    fn request_http_upload(address: SocketAddr, url: Url, counter: Arc<LoadCounter>) {
        let chunk_count = 50;
        let data_size = chunk_count * 1024 * 1024 as u64;
        let _host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let path_str = url.path();
        let _host_str = url.host_str().unwrap();

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                log::debug!("Failed to connect to proxy: {}", url);
                return;
            }
        };

        counter.wait();

        let mut data_counter: u64 = 0;
        let request_chunk = vec![b'O'; 131072]; // 创建一个128KB的缓冲区填充值

        let request_head = format!(
            "POST {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: bimc/0.17.1\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            path_str,
            _host_str,
            data_size
        )
        .into_bytes();

        match stream.write_all(&request_head) {
            Ok(_) => {
                let length = request_head.len() as u64;
                data_counter = length;
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Upload write error");
                return;
            }
        }

        while data_counter < data_size && !counter.is_end() {
            match stream.write(&request_chunk) {
                Ok(size) => {
                    data_counter += size as u64;
                    
                    if size == 0 {
                        #[cfg(debug_assertions)]
                        debug!("Upload Error: Write failed");
                        break;
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload write error");
                    break;
                }
            }
        }
    }
}

impl Client for HTTPClient {
    fn ping(&mut self) -> bool {
        let mut count = 0;
        let mut pings = [0u128; 6];
        let mut ping_min = 10000000;

        while count < 6 {
            let start = Instant::now();
            let ping = start.elapsed().as_micros();
            if ping < ping_min {
                ping_min = ping;
            }
            pings[count] = ping;
            thread::sleep(Duration::from_millis(1000));
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
        self.jitter = jitter_all as f64 / 6.0 / 1_000.0;

        #[cfg(debug_assertions)]
        debug!("Ping {} ms", self.latency);

        #[cfg(debug_assertions)]
        debug!("Jitter {} ms", self.jitter);

        true
    }

    fn download(&mut self) -> bool {
        match self.run_load(1) {
            Ok(_) => true,
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Download error: {}", _e);
                false
            }
        }
    }

    fn upload(&mut self) -> bool {
        match self.run_load(0) {
            Ok(_) => true,
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Upload error: {}", _e);
                false
            }
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