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
                debug!("Upload thread panicked");
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

        let mut buffer = [0; 1024];
        let mut _data_counter: u64 = 0;
        let mut data_size: u64 = 50 * 1024 * 1024; // 默认大小

        let request_head = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: bim/1.0\r\n\r\n",
            path_query, host_port,
        )
        .into_bytes();

        match stream.write_all(&request_head) {
            Ok(_) => {
                #[cfg(debug_assertions)]
                debug!("Download request sent: {}", String::from_utf8_lossy(&request_head));
                
                // 读取并解析HTTP响应头
                let mut headers_buf = Vec::new();
                let mut header_finished = false;
                let mut read_buf = [0u8; 1];
                
                while !header_finished && !counter.is_end() {
                    match stream.read(&mut read_buf) {
                        Ok(1) => {
                            headers_buf.push(read_buf[0]);
                            // 检查是否到达响应头结尾(\r\n\r\n)
                            if headers_buf.len() >= 4 && 
                               headers_buf[headers_buf.len()-4] == b'\r' &&
                               headers_buf[headers_buf.len()-3] == b'\n' &&
                               headers_buf[headers_buf.len()-2] == b'\r' &&
                               headers_buf[headers_buf.len()-1] == b'\n' {
                                header_finished = true;
                            }
                        }
                        Ok(0) => {
                            // 连接已关闭
                            #[cfg(debug_assertions)]
                            debug!("Connection closed while reading headers");
                            return;
                        }
                        Ok(_) => {
                            // 不应该发生的情况
                            #[cfg(debug_assertions)]
                            debug!("Unexpected read size while reading headers");
                            return;
                        }
                        Err(_e) => {
                            #[cfg(debug_assertions)]
                            debug!("Failed to read response headers: {} - Error: {}", url, _e);
                            return;
                        }
                    }
                }
                
                // 解析响应头
                let headers_str = String::from_utf8_lossy(&headers_buf);
                #[cfg(debug_assertions)]
                debug!("HTTP Response headers: {}", headers_str);
                
                // 检查HTTP状态码
                let status_line = headers_str.lines().next().unwrap_or("");
                #[cfg(debug_assertions)]
                debug!("HTTP Status line: {}", status_line);
                
                if !status_line.contains("200") && !status_line.contains("206") {
                    #[cfg(debug_assertions)]
                    debug!("Non-success HTTP status code detected");
                    return;
                }

                // 解析Content-Length头部
                for line in headers_str.lines().skip(1) {
                    if line.is_empty() {
                        break;
                    }
                    // 处理可能的大小写变化和额外空格
                    let trimmed_line = line.trim();
                    if trimmed_line.to_lowercase().starts_with("content-length:") {
                        if let Ok(len) = trimmed_line["Content-Length:".len()..].trim().parse::<u64>() {
                            data_size = len;
                            #[cfg(debug_assertions)]
                            debug!("Content-Length header found: {}", data_size);
                        } else if let Ok(len) = trimmed_line.split(':').nth(1).unwrap_or("0").trim().parse::<u64>() {
                            data_size = len;
                            #[cfg(debug_assertions)]
                            debug!("Content-Length header found (alternative parse): {}", data_size);
                        }
                    }
                }
                
                #[cfg(debug_assertions)]
                debug!("Final target data size: {}", data_size);
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Download write error: {}", _e);
                return;
            }
        }

        #[cfg(debug_assertions)]
        debug!("Starting to read download data, target size: {}", data_size);

        let mut _read_count = 0;
        #[cfg(debug_assertions)]
        let mut total_bytes_read = 0u64;
        
        while (_data_counter < data_size || data_size == 0) && !counter.is_end() {
            match stream.read(&mut buffer) {
                Ok(size) => {
                    _read_count += 1;
                    #[cfg(debug_assertions)]
                    {
                        total_bytes_read += size as u64;
                    }
                    
                    if size == 0 {
                        // 连接已关闭，传输完成
                        #[cfg(debug_assertions)]
                        debug!("Download completed, connection closed. Total bytes: {}, read operations: {}", _data_counter, _read_count);
                        break;
                    }
                    
                    let _count = size as u64;
                    _data_counter += _count;
                    counter.increase(_count);

                    #[cfg(debug_assertions)]
                    if _data_counter % (10 * 1024) < _count as u64 { // 每10KB输出一次日志
                        debug!("Downloaded {} bytes so far, current read size: {}, read operations: {}", _data_counter, size, _read_count);
                    }
                    
                    #[cfg(debug_assertions)]
                    if _read_count <= 5 {
                        // 记录前几次读取的详细信息
                        debug!("Read #{}: {} bytes, buffer sample: {:?}", _read_count, size, &buffer[..size.min(50)]);
                    }
                    
                    #[cfg(debug_assertions)]
                    if _read_count % 100 == 0 { // 每100次读取输出一次汇总信息
                        debug!("Download progress: {} bytes read in {} operations", total_bytes_read, _read_count);
                    }
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Download read error: {} - Total bytes: {}, read operations: {}", _e, _data_counter, _read_count);
                    break;
                }
            }
        }
        
        #[cfg(debug_assertions)]
        debug!("Download finished. Total bytes: {}, Target size: {}, Total read operations: {}", _data_counter, data_size, _read_count);
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
            "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: bim/1.0\r\nContent-Length: {}\r\n\r\n",
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