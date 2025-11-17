use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use log::debug;

use url::Url;

use crate::clients::base::{get_address_with_fallback, make_connection, request_tcp_ping, Client, LoadCounter};
use crate::utils::SpeedTestResult;

use std::io::{Read, Write};

pub struct SpeedtestNetTcpClient {
    threads: u8,

    address: SocketAddr,

    upload: f64,
    upload_status: String,
    download: f64,
    download_status: String,
    latency: f64,
    jitter: f64,
}

impl SpeedtestNetTcpClient {
    pub fn build(url: String, force_ipv4: bool, force_ipv6: bool, threads: u8) -> Option<Box<dyn Client>> {
        let url = Url::parse(&url).ok()?;

        // 获取地址，支持IPv4/IPv6容错
        let address = get_address_with_fallback(&url, force_ipv4, force_ipv6)?;

        #[cfg(debug_assertions)]
        debug!("IP address {address}");

        let r = "取消".to_owned();
        Some(Box::new(Self {
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
        let counter = Arc::new(LoadCounter::new(self.threads));
        let mut tasks = vec![];

        for _ in 0..self.threads {
            let a = self.address.clone();
            let c = counter.clone();

            let task = thread::spawn(move || {
                match load {
                    0 => Self::request_tcp_upload(a, c),
                    _ => Self::request_tcp_download(a, c),
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

    fn request_tcp_download(address: SocketAddr, counter: Arc<LoadCounter>) {
        let data_size = 15 * 1024 * 1024 * 1024 as u128;
        let mut buffer = [0; 65536];

        let url = Url::parse("http://bench.im").unwrap();
        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                log::debug!("Failed to connect to server: {}", address);
                return;
            }
        };

        counter.wait();

        #[cfg(debug_assertions)]
        debug!("Download Start");

        let request = format!("DOWNLOAD {data_size}\n").into_bytes();
        match stream.write_all(&request) {
            Ok(_) => {
                match stream.read(&mut buffer) {
                    Ok(size) => {
                        #[cfg(debug_assertions)]
                        debug!("Download Status: {size}");

                        if size == 0 {
                            #[cfg(debug_assertions)]
                            debug!("Download Error: Start failed");
                            return;
                        }

                        let count = size as u64;
                        counter.increase(count);
                    }
                    Err(_e) => {
                        log::debug!("Failed to read from server: {}", address);
                        return;
                    }
                }
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Download write error");
                return;
            }
        }

        while !counter.is_end() {
            match stream.read(&mut buffer) {
                Ok(size) => {
                    let count = size as u64;
                    counter.increase(count);
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Download read error");
                    return;
                }
            }
        }
    }

    fn request_tcp_upload(address: SocketAddr, counter: Arc<LoadCounter>) {
        let data_size = 15 * 1024 * 1024 * 1024 as u128;
        let request_chunk = "0123456789AaBbCcDdEeFfGgHhIiJjKkLlMmNnOoPpQqRrSsTtUuVvWwXxYyZz-="
            .repeat(1024)
            .into_bytes();

        let url = Url::parse("http://bench.im").unwrap();

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Connection error");
                counter.wait();
                return;
            }
        };

        counter.wait();

        #[cfg(debug_assertions)]
        debug!("Upload Start");

        let request = format!("UPLOAD {data_size} 0\n").into_bytes();
        match stream.write_all(&request) {
            Ok(_) => {
                let count = request.len() as u64;
                counter.increase(count);
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Upload write error");
                return;
            }
        }

        while !counter.is_end() {
            match stream.write(&request_chunk) {
                Ok(size) => {
                    let count = size as u64;
                    counter.increase(count);
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload write error");
                    return;
                }
            }
        }
    }
}

impl Client for SpeedtestNetTcpClient {
    fn ping(&mut self) -> bool {
        let mut count = 0;
        let mut pings = [0u128; 10]; // 增加测量点数量
        let mut ping_min = u128::MAX;
        let mut valid_pings = 0;

        while count < 10 {
            let ping = request_tcp_ping(&self.address);
            if ping > 0 {
                if ping < ping_min {
                    ping_min = ping
                }
                pings[count] = ping;
                valid_pings += 1;
            }
            thread::sleep(Duration::from_millis(500)); // 减少测量间隔时间
            count += 1;
        }

        // 如果没有成功的ping，则返回false
        if valid_pings == 0 {
            self.latency = 0.0;
            self.jitter = 0.0;
            return false;
        }

        // 对ping值进行排序并去除异常值
        let mut valid_pings_vec: Vec<u128> = pings[..valid_pings].to_vec();
        valid_pings_vec.sort();
        
        // 去除最高和最低的10%作为异常值
        let remove_count = (valid_pings as f64 * 0.1).ceil() as usize;
        let start = remove_count.min(valid_pings);
        let end = valid_pings.saturating_sub(remove_count);
        
        // 确保start < end
        let (start, end) = if start >= end { 
            (0, valid_pings) 
        } else { 
            (start, end) 
        };
        
        // 重新计算最小值（排除异常值后）
        if let Some(&new_min) = valid_pings_vec[start..end].iter().min() {
            ping_min = new_min;
        }

        let mut jitter_all = 0u128;
        let mut jitter_count = 0;
        
        // 计算抖动时也排除异常值
        for &p in &valid_pings_vec[start..end] {
            if p > 0 {
                jitter_all += p.abs_diff(ping_min);
                jitter_count += 1;
            }
        }

        self.latency = ping_min as f64 / 1_000.0; // 转换为毫秒
        self.jitter = if jitter_count > 0 {
            jitter_all as f64 / jitter_count as f64 / 1_000.0 // 转换为毫秒
        } else {
            0.0
        };

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