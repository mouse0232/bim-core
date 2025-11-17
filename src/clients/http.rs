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

        #[cfg(debug_assertions)]
        debug!("Starting {} threads for {:?}", self.threads, if load == 0 { "upload" } else { "download" });

        for i in 0..self.threads {
            let a = self.address.clone();
            let u = url.clone();
            let c = counter.clone();

            let task = thread::spawn(move || {
                #[cfg(debug_assertions)]
                debug!("Thread {} started for {:?}", i, if load == 0 { "upload" } else { "download" });
                
                match load {
                    0 => Self::request_http_upload(a, u, c),
                    _ => Self::request_http_download(a, u, c),
                };
                
                #[cfg(debug_assertions)]
                debug!("Thread {} finished for {:?}", i, if load == 0 { "upload" } else { "download" });
            });
            tasks.push(task);
            thread::sleep(Duration::from_millis(100));
        }

        let mut time_passed = 0;
        counter.wait();

        #[cfg(debug_assertions)]
        debug!("Load test started");

        let now = Instant::now();
        while time_passed < 14_000_000 {
            thread::sleep(Duration::from_millis(500));
            time_passed = now.elapsed().as_micros();

            counter.count(time_passed);
            
            #[cfg(debug_assertions)]
            debug!("Time passed: {} microseconds", time_passed);
        }

        counter.end();
        
        #[cfg(debug_assertions)]
        debug!("Load test ended, collecting results");

        for (i, task) in tasks.into_iter().enumerate() {
            if let Err(_e) = task.join() {
                #[cfg(debug_assertions)]
                debug!("Task {} join error", i);
            }
        }

        // 添加更多日志来检查结果
        #[cfg(debug_assertions)]
        debug!("Checking results for {}", if load == 0 { "upload" } else { "download" });

        match load {
            0 => {
                self.upload = counter.speed();
                self.upload_status = counter.status();
                #[cfg(debug_assertions)]
                debug!("Upload speed: {}, status: {}", self.upload, self.upload_status);
            }
            _ => {
                self.download = counter.speed();
                self.download_status = counter.status();
                #[cfg(debug_assertions)]
                debug!("Download speed: {}, status: {}", self.download, self.download_status);
            }
        }

        Ok(true)
    }

    fn request_http_download(address: SocketAddr, url: Url, counter: Arc<LoadCounter>) {
        let chunk_count = 350;
        let data_size = chunk_count * 128 * 1024 as u64;
        let host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let path_str = url.path();
        let _host_str = url.host_str().unwrap();

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let path_query = format!("{}?x={}&chunk={}&size={}", path_str, now, chunk_count, data_size);

        #[cfg(debug_assertions)]
        debug!("Download {path_query}");

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Failed to connect to proxy: {} - Error: {}", url, e);
                return;
            }
        };

        counter.wait();

        let mut buffer = [0; 1024];
        let mut data_counter: u64; // 修复：移除初始化值

        let request_head = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: bim/1.0\r\n\r\n",
            path_query, host_port,
        )
        .into_bytes();

        match stream.write_all(&request_head) {
            Ok(_) => {
                #[cfg(debug_assertions)]
                debug!("Download request sent");
                
                match stream.read(&mut buffer) {
                    Ok(size) => {
                        #[cfg(debug_assertions)]
                        debug!("Download Status: {size}");

                        if size > 0 {
                            data_counter = size as u64;
                            counter.increase(data_counter);
                        } else {
                            #[cfg(debug_assertions)]
                            debug!("Download read returned 0 bytes");
                            return;
                        }
                    }
                    Err(e) => {
                        #[cfg(debug_assertions)]
                        debug!("Download read error: {}", e);
                        return;
                    }
                }
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Download write error: {}", e);
                return;
            }
        }

        #[cfg(debug_assertions)]
        debug!("Starting to read download data, target size: {}", data_size);

        while data_counter < data_size && !counter.is_end() {
            match stream.read(&mut buffer) {
                Ok(size) => {
                    if size == 0 {
                        // 连接已关闭，传输完成
                        #[cfg(debug_assertions)]
                        debug!("Download completed, connection closed. Total bytes: {}", data_counter);
                        break;
                    }
                    
                    let _count = size as u64;
                    data_counter += _count;
                    counter.increase(_count);

                    #[cfg(debug_assertions)]
                    if data_counter % (1024 * 1024) < _count as u64 { // 每MB输出一次日志
                        debug!("Downloaded {} bytes so far", data_counter);
                    }
                }
                Err(e) => {
                    #[cfg(debug_assertions)]
                    debug!("Failed to read response: {} - Error: {}", url, e);
                    return;
                }
            }
        }
        
        #[cfg(debug_assertions)]
        debug!("Download finished, total bytes: {}", data_counter);
        
        // 确保所有数据都被处理
        if let Err(e) = stream.flush() {
            #[cfg(debug_assertions)]
            debug!("Error flushing download stream: {}", e);
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
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Failed to connect to proxy: {} - Error: {}", url, e);
                return;
            }
        };

        counter.wait();

        let mut data_counter: u64;  // 修复：移除初始化值
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

        #[cfg(debug_assertions)]
        debug!("Upload request head size: {}", request_head.len());
        #[cfg(debug_assertions)]
        debug!("Upload target size: {}", data_size);

        match stream.write_all(&request_head) {
            Ok(_) => {
                data_counter = request_head.len() as u64;
                #[cfg(debug_assertions)]
                debug!("Upload request head sent, {} bytes", data_counter);
                counter.increase(data_counter);
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Upload write error: {}", e);
                return;
            }
        }

        #[cfg(debug_assertions)]
        debug!("Starting to upload data");

        while data_counter < data_size && !counter.is_end() {
            // 计算还需要发送多少数据
            let remaining = data_size - data_counter;
            let chunk_size = std::cmp::min(remaining, request_chunk.len() as u64) as usize;
            
            match stream.write(&request_chunk[..chunk_size]) {
                Ok(size) => {
                    if size == 0 {
                        // 连接已关闭
                        #[cfg(debug_assertions)]
                        debug!("Upload connection closed");
                        break;
                    }
                    
                    data_counter += size as u64;
                    
                    #[cfg(debug_assertions)]
                    if data_counter % (1024 * 1024) < size as u64 { // 每MB输出一次日志
                        debug!("Uploaded {} bytes so far", data_counter);
                    }
                    
                    counter.increase(size as u64);

                    // 确保数据被发送
                    if let Err(e) = stream.flush() {
                        #[cfg(debug_assertions)]
                        debug!("Error flushing upload stream: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload write error: {}", e);
                    break;
                }
            }
        }
        
        // 等待服务器响应
        let mut buffer = [0; 1024];
        match stream.read(&mut buffer) {
            Ok(size) => {
                #[cfg(debug_assertions)]
                debug!("Received server response: {} bytes", size);
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Error reading server response: {}", e);
            }
        }
        
        #[cfg(debug_assertions)]
        debug!("Upload completed, total bytes: {}", data_counter);
    }
}

impl Client for HTTPClient {
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

    fn ping(&mut self) -> bool {
        #[cfg(debug_assertions)]
        debug!("Starting ping test");
        
        let mut min = u128::MAX;
        let mut times = Vec::new();

        for i in 0..10 {
            #[cfg(debug_assertions)]
            debug!("Ping attempt {}", i);
            
            let r = crate::clients::base::request_tcp_ping(&self.address);
            
            #[cfg(debug_assertions)]
            debug!("Ping result: {}", r);
            
            if r > 0 && r < 1_000_000 {
                times.push(r);
                if r < min {
                    min = r;
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        if times.is_empty() {
            #[cfg(debug_assertions)]
            debug!("No successful ping attempts");
            return false;
        }

        times.sort();
        // 去除前5%和后5%的极值
        let remove_count = (times.len() as f64 * 0.05).round() as usize;
        let times = &times[remove_count..times.len() - remove_count];

        if times.is_empty() {
            #[cfg(debug_assertions)]
            debug!("No valid ping times after filtering");
            return false;
        }

        self.latency = *times.first().unwrap() as f64 / 1000.0;
        let avg = times.iter().sum::<u128>() as f64 / times.len() as f64;
        self.jitter = (avg - self.latency) / 1000.0;

        #[cfg(debug_assertions)]
        debug!("Ping test completed - latency: {}, jitter: {}", self.latency, self.jitter);
        
        true
    }

    fn upload(&mut self) -> bool {
        #[cfg(debug_assertions)]
        debug!("Starting upload test");
        
        match self.run_load(0) {
            Ok(_) => {
                #[cfg(debug_assertions)]
                debug!("Upload test completed successfully");
                true
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Upload test failed with error: {}", e);
                false
            }
        }
    }

    fn download(&mut self) -> bool {
        #[cfg(debug_assertions)]
        debug!("Starting download test");
        
        match self.run_load(1) {
            Ok(_) => {
                #[cfg(debug_assertions)]
                debug!("Download test completed successfully");
                true
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                debug!("Download test failed with error: {}", e);
                false
            }
        }
    }
}