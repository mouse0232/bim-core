use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::net::SocketAddr;

#[cfg(debug_assertions)]
use log::debug;

use crate::clients::base::{get_address_with_fallback, make_connection, Client, LoadCounter};
use crate::utils::SpeedTestResult;

use std::io::{Read, Write};
use url::Url;

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

        for _i in 0..self.threads {
            let a = self.address.clone();
            let u = url.clone();
            let c = counter.clone();

            let task = thread::spawn(move || {
                #[cfg(debug_assertions)]
                debug!("Thread {} started for {:?}", _i, if load == 0 { "upload" } else { "download" });
                
                if load == 0 {
                    Self::request_http_upload(a, u, c);
                } else {
                    Self::request_http_download(a, u, c);
                }
                
                #[cfg(debug_assertions)]
                debug!("Thread {} finished for {:?}", _i, if load == 0 { "upload" } else { "download" });
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

        for (_i, task) in tasks.into_iter().enumerate() {
            if let Err(_e) = task.join() {
                #[cfg(debug_assertions)]
                debug!("{} thread panicked", if load == 0 { "Upload" } else { "Download" });
            }
        }

        // 添加更多日志来检查结果
        #[cfg(debug_assertions)]
        {
            let results = counter.results.lock().unwrap();
            debug!("Collected {} results for {}", results.len(), if load == 0 { "upload" } else { "download" });
            for (i, (bytes, time)) in results.iter().enumerate() {
                debug!("Result {}: {} bytes at {} microseconds", i, bytes, time);
            }
        }

        // 确保在测试结束后再记录一次最终数据点
        counter.count(now.elapsed().as_micros());
        
        let speed = counter.speed();
        let status = counter.status();
        
        #[cfg(debug_assertions)]
        debug!("Calculated {} speed: {}, status: {}", if load == 0 { "upload" } else { "download" }, speed, status);

        match load {
            0 => {
                self.upload = speed;
                self.upload_status = status;
                #[cfg(debug_assertions)]
                debug!("Upload speed: {}, status: {}", self.upload, self.upload_status);
            }
            _ => {
                self.download = speed;
                self.download_status = status;
                #[cfg(debug_assertions)]
                debug!("Download speed: {}, status: {}", self.download, self.download_status);
            }
        }

        Ok(true)
    }

    fn request_http_download(address: SocketAddr, url: Url, counter: Arc<LoadCounter>) {
        let chunk_count = 50;
        let data_size = chunk_count * 1024 * 1024 as u64;
        let mut data_counter = 0u64;
        let mut buffer = [0; 65536];

        let host_port = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        let path_str = url.path();

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Failed to connect to proxy: {} - Error: {}", url, _e);
                return;
            }
        };

        counter.wait();

        'request: while !counter.is_end() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
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
                    #[cfg(debug_assertions)]
                    debug!("Download request sent: {}", String::from_utf8_lossy(&request_head));
                    
                    if let Ok(size) = stream.read(&mut buffer) {
                        #[cfg(debug_assertions)]
                        debug!("Download Status: {size}");

                        if size > 0 {
                            let count = size as u64;
                            data_counter = count;
                            counter.increase(count);
                        } else {
                            break 'request;
                        }
                    } else {
                        break 'request;
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Download write error: {}", _e);

                    break 'request;
                }
            }

            while data_counter < data_size && !counter.is_end() {
                match stream.read(&mut buffer) {
                    Ok(size) => {
                        let count = size as u64;
                        data_counter += count;
                        counter.increase(count);
                        
                        #[cfg(debug_assertions)]
                        if data_counter % (1024 * 1024) < count as u64 { // 每MB输出一次日志
                            debug!("Downloaded {} bytes so far", data_counter);
                        }
                    }
                    Err(_e) => {
                        #[cfg(debug_assertions)]
                        debug!("Download read error: {}", _e);

                        break 'request;
                    }
                }
            }
        }
        
        #[cfg(debug_assertions)]
        debug!("Download test completed with {} bytes read", data_counter);
    }

    fn request_http_upload(address: SocketAddr, url: Url, counter: Arc<LoadCounter>) {
        let data_size = 50 * 1024 * 1024;
        let path_query = if url.query().is_some() {
            format!("{}?{}", url.path(), url.query().unwrap())
        } else {
            url.path().to_string()
        };
        let host_port = format!("{}:{}", url.host_str().unwrap_or(""), url.port_or_known_default().unwrap_or(80));

        let mut stream = match make_connection(&address, &url) {
            Ok(s) => s,
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Failed to connect to proxy: {} - Error: {}", url, _e);
                return;
            }
        };

        counter.wait();

        let request_head = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36\r\nContent-Length: {}\r\n\r\n",
            path_query, host_port, data_size
        );

        match stream.write_all(request_head.as_bytes()) {
            Ok(_) => {
                #[cfg(debug_assertions)]
                debug!("Upload request sent");
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Upload write error: {}", _e);
                return;
            }
        }

        let mut buffer = [0x42; 1024];
        let mut _data_counter: u64 = 0;
        let mut size: usize = 1024;

        #[cfg(debug_assertions)]
        debug!("Starting to send upload data, target size: {}", data_size);

        while _data_counter < data_size && !counter.is_end() {
            if _data_counter + size as u64 > data_size {
                size = (data_size - _data_counter) as usize;
                buffer = [0x42; 1024][0..size].try_into().unwrap();
            }

            match stream.write(&buffer) {
                Ok(_size) => {
                    _data_counter += size as u64;
                    
                    #[cfg(debug_assertions)]
                    if _data_counter % (1024 * 1024) < size as u64 { // 每MB输出一次日志
                        debug!("Uploaded {} bytes so far", _data_counter);
                    }
                    
                    counter.increase(size as u64);

                    // 确保数据被发送
                    if let Err(_e) = stream.flush() {
                        #[cfg(debug_assertions)]
                        debug!("Error flushing upload stream: {} - Total bytes: {}", _e, _data_counter);
                        break;
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Upload write error: {} - Total bytes: {}", _e, _data_counter);
                    break;
                }
            }
        }
        
        #[cfg(debug_assertions)]
        debug!("Upload finished. Total bytes: {}, Target size: {}", _data_counter, data_size);
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

        for _i in 0..10 {
            #[cfg(debug_assertions)]
            debug!("Ping attempt {}", _i);
            
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
        
        // 改进抖动计算方式：使用标准差或者相邻差值的平均值
        let sum: u128 = times.iter().sum();
        let avg = sum as f64 / times.len() as f64;
        
        // 计算相邻测量值之间差值的绝对值的平均值作为抖动
        let mut jitter_sum = 0.0;
        for i in 1..times.len() {
            let diff = (times[i] as f64 - times[i-1] as f64).abs();
            jitter_sum += diff;
        }
        
        // 如果有至少2个测量值，计算平均差值作为抖动
        if times.len() > 1 {
            self.jitter = (jitter_sum / (times.len() - 1) as f64) / 1000.0;
        } else {
            // 如果只有一个测量值，使用与平均值的差作为抖动
            self.jitter = ((avg - self.latency).abs()) / 1000.0;
        }

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
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Upload test failed: {}", _e);
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
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Download test failed with error: {}", _e);
                false
            }
        }
    }
}