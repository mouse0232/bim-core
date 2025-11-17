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
        // 增加重试机制
        let max_retries = 3;
        let mut retries = 0;
        
        while retries < max_retries {
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

            // 检查结果是否有效
            let speed = counter.speed();
            let status = counter.status();
            
            // 如果测试成功且速度不为0，或者状态不是"无数据"，则认为测试有效
            if speed > 0.0 || status != "无数据" {
                match load {
                    0 => {
                        self.upload = speed;
                        self.upload_status = status;
                    }
                    _ => {
                        self.download = speed;
                        self.download_status = status;
                    }
                }
                return Ok(true);
            }
            
            // 如果测试失败，增加重试计数
            retries += 1;
            if retries < max_retries {
                #[cfg(debug_assertions)]
                debug!("Test failed, retrying... ({}/{})", retries, max_retries);
                thread::sleep(Duration::from_secs(1));
            }
        }
        
        // 所有重试都失败了
        match load {
            0 => {
                self.upload = 0.0;
                self.upload_status = "失败".to_string();
            }
            _ => {
                self.download = 0.0;
                self.download_status = "失败".to_string();
            }
        }
        
        Ok(false)
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

        let mut data_counter: u64;
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
                    // 读取并解析HTTP响应头
                    let mut header_buffer = String::new();
                    let mut byte_buffer = [0; 1];
                    let header_complete = false;
                    let start_time = std::time::Instant::now();
                    
                    while !header_complete && start_time.elapsed().as_secs() < 10 {
                        match stream.read(&mut byte_buffer) {
                            Ok(0) => break, // 连接关闭
                            Ok(_) => {
                                header_buffer.push(byte_buffer[0] as char);
                                
                                // 检查是否读取到了完整的HTTP响应头
                                if header_buffer.len() >= 4 {
                                    if header_buffer.ends_with("\r\n\r\n") || header_buffer.ends_with("\n\n") {
                                        // 不再需要设置header_complete = true，因为我们直接break
                                        break;
                                    }
                                }
                            }
                            Err(_) => {
                                #[cfg(debug_assertions)]
                                debug!("Download header read error");
                                break;
                            }
                        }
                    }
                    
                    // 检查响应状态码
                    if !header_buffer.starts_with("HTTP/1.1 200") && !header_buffer.starts_with("HTTP/1.0 200") {
                        #[cfg(debug_assertions)]
                        debug!("Download error: {}", header_buffer);
                        break;
                    }

                    data_counter = 0;
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

        let mut data_counter: u64 = request_head.len() as u64;

        // 发送请求头
        if let Err(_e) = stream.write_all(&request_head) {
            #[cfg(debug_assertions)]
            debug!("Upload write error: {}", _e);
            return;
        }

        // 发送数据直到达到指定大小
        while data_counter < data_size && !counter.is_end() {
            // 计算还需要发送多少数据
            let remaining = data_size - data_counter;
            let chunk_size = std::cmp::min(remaining as usize, request_chunk.len());
            
            match stream.write(&request_chunk[..chunk_size]) {
                Ok(size) => {
                    data_counter += size as u64;
                    counter.increase(size as u64);
                    
                    if size == 0 {
                        #[cfg(debug_assertions)]
                        debug!("Upload Error: Write failed");
                        break;
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload write error: {}", _e);
                    break;
                }
            }
        }
        
        // 确保所有数据都被刷新
        if let Err(_e) = stream.flush() {
            #[cfg(debug_assertions)]
            debug!("Upload flush error: {}", _e);
        }
        
        // 读取服务器响应
        let mut response_buffer = Vec::new();
        let mut byte_buffer = [0; 1024];
        let start_time = std::time::Instant::now();
        
        // 设置一个超时时间，避免无限等待
        while start_time.elapsed().as_secs() < 10 {
            match stream.read(&mut byte_buffer) {
                Ok(0) => break, // 连接关闭
                Ok(size) => {
                    response_buffer.extend_from_slice(&byte_buffer[..size]);
                    // 检查是否读取到了完整的HTTP响应头
                    if response_buffer.len() > 4 && 
                       (response_buffer.windows(4).any(|w| w == b"\r\n\r\n") || 
                        response_buffer.windows(2).any(|w| w == b"\n\n")) {
                        break;
                    }
                }
                Err(_) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload response read error");
                    break;
                }
            }
        }
        
        // 检查响应状态码
        let response_str = String::from_utf8_lossy(&response_buffer);
        if !response_str.starts_with("HTTP/1.1 200") && !response_str.starts_with("HTTP/1.0 200") {
            #[cfg(debug_assertions)]
            debug!("Upload completed with response: {}", response_str);
        }
    }
}

impl Client for HTTPClient {
    fn ping(&mut self) -> bool {
        let mut count = 0;
        let mut pings = [0u128; 10]; // 增加测量点数量
        let mut ping_min = u128::MAX;
        let mut valid_pings = 0;

        while count < 10 {
            // 使用TCP连接时间作为ping值，这更接近网络延迟而不是HTTP请求时间
            let ping = crate::clients::base::request_tcp_ping(&self.address);
            if ping > 0 {
                if ping < ping_min {
                    ping_min = ping;
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