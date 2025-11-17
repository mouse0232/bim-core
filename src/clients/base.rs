use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use log::debug;

use rustls::{RootCertStore, pki_types::ServerName};
use url::Url;

use crate::utils::SpeedTestResult;

pub trait GenericStream: Read + Write {}

impl<T: Read + Write> GenericStream for T {}

/// 获取地址，支持IPv4/IPv6容错
/// 
/// 参数:
/// - url: 目标URL
/// - force_ipv4: 是否强制使用IPv4
/// - force_ipv6: 是否强制使用IPv6
/// 
/// 返回值:
/// - Some(SocketAddr): 成功获取到地址
/// - None: 无法获取到合适的地址
pub fn get_address_with_fallback(url: &Url, force_ipv4: bool, force_ipv6: bool) -> Option<SocketAddr> {
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;

    let host_port = format!("{host}:{port}");
    let addresses = host_port.to_socket_addrs().ok()?;

    // 如果强制使用IPv4
    if force_ipv4 {
        return addresses.into_iter().find(|addr| addr.is_ipv4());
    }

    // 如果强制使用IPv6
    if force_ipv6 {
        return addresses.into_iter().find(|addr| addr.is_ipv6());
    }

    // 如果没有强制指定IP版本，则实现容错机制
    // 首先尝试IPv4，如果失败再尝试IPv6
    let addr_vec: Vec<SocketAddr> = addresses.into_iter().collect();
    
    // 优先选择IPv4地址（大多数网络环境下更稳定）
    if let Some(addr) = addr_vec.iter().find(|addr| addr.is_ipv4()) {
        return Some(*addr);
    }
    
    // 如果没有IPv4地址，则选择IPv6地址
    if let Some(addr) = addr_vec.iter().find(|addr| addr.is_ipv6()) {
        return Some(*addr);
    }
    
    // 如果都没有，则返回第一个地址（如果存在）
    addr_vec.first().copied()
}

pub fn make_connection(address: &SocketAddr, url: &Url) -> Result<Box<dyn GenericStream>, String> {
    let ssl = if url.scheme() == "https" { true } else { false };
    let mut retry = 3;

    while retry > 0 {
        match TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            Ok(stream) => {
                #[cfg(debug_assertions)]
                debug!("TCP connected");

                let _r = stream.set_write_timeout(Some(Duration::from_secs(30)));
                let _r = stream.set_read_timeout(Some(Duration::from_secs(30)));
                if !ssl {
                    return Ok(Box::new(stream));
                }

                let mut root_store = RootCertStore::empty();
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

                let config = rustls::ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth();

                let host_str = url.host_str().unwrap();
                let server_name = ServerName::try_from(host_str.to_string()).unwrap();
                let conn = rustls::ClientConnection::new(Arc::new(config), server_name).unwrap();

                let tls = rustls::StreamOwned::new(conn, stream);

                #[cfg(debug_assertions)]
                debug!("SSL connected");

                return Ok(Box::new(tls));
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Failed to connect, retries left: {}", retry);
                
                retry -= 1;
                if retry == 0 {
                    return Err("无法建立TCP连接".into());
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
    
    Err(String::from("连接失败"))
}

pub fn request_tcp_ping(address: &SocketAddr) -> u128 {
    let now = Instant::now();
    let r = TcpStream::connect_timeout(&address, Duration::from_micros(1_000_000));
    let used = now.elapsed().as_micros();
    match r {
        Ok(_) => used,
        Err(_e) => {
            #[cfg(debug_assertions)]
            debug!("Ping {_e}");

            0
        }
    }
}

/// 优化的负载计数器，用于跟踪网络测试过程中的数据传输
pub struct LoadCounter {
    /// 原子计数器，避免锁竞争
    counter: std::sync::atomic::AtomicU64,
    /// 同步屏障，确保所有线程同时开始
    stater: Barrier,
    /// 原子布尔值，标记测试是否结束
    ender: std::sync::atomic::AtomicBool,
    /// 结果存储，使用向量而非RwLock包装的向量
    pub results: std::sync::Mutex<Vec<(u64, u128)>>,
}

impl LoadCounter {
    pub fn new(threads: u8) -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
            stater: Barrier::new((threads + 1) as usize),
            ender: std::sync::atomic::AtomicBool::new(false),
            results: std::sync::Mutex::new(vec![]),
        }
    }

    pub fn wait(&self) {
        self.stater.wait();
    }

    pub fn end(&self) {
        self.ender.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_end(&self) -> bool {
        self.ender.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn increase(&self, count: u64) {
        self.counter.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn count(&self, time_passed: u128) {
        let c = self.counter.load(std::sync::atomic::Ordering::Relaxed);
        
        let mut results = self.results.lock().unwrap();
        results.push((c, time_passed));
    }

    pub fn speed(&self) -> f64 {
        let results = self.results.lock().unwrap();
        
        #[cfg(debug_assertions)]
        debug!("Calculating speed with {} data points", results.len());
        
        // 记录所有数据点用于调试
        #[cfg(debug_assertions)]
        for (i, (bytes, time)) in results.iter().enumerate() {
            debug!("Data point {}: {} bytes at {} microseconds", i, bytes, time);
        }
        
        // 至少需要2个数据点来计算速度
        if results.len() < 2 {
            #[cfg(debug_assertions)]
            debug!("Not enough data points to calculate speed, returning 0.0");
            return 0.0;
        }
        
        // 使用所有数据点或最后10个数据点来计算速度
        let (start_index, end_index) = if results.len() >= 10 {
            // 使用最后10个数据点
            (results.len() - 10, results.len() - 1)
        } else {
            // 使用所有数据点
            (0, results.len() - 1)
        };
        
        let (c_start, t_start) = results[start_index];
        let (c_end, t_end) = results[end_index];
        
        #[cfg(debug_assertions)]
        debug!("Speed calculation: start={} bytes at {} μs, end={} bytes at {} μs", 
               c_start, t_start, c_end, t_end);

        // 检查时间差是否为0，避免除以0
        if t_end <= t_start {
            #[cfg(debug_assertions)]
            debug!("Time difference is zero or negative, returning 0.0");
            return 0.0;
        }

        let byte_diff = c_end.saturating_sub(c_start); // 使用饱和减法避免下溢
        let time_diff = t_end - t_start; // 时间差不应该为负数
        
        #[cfg(debug_assertions)]
        debug!("Byte difference: {}, Time difference: {} μs", byte_diff, time_diff);

        // 检查是否有数据传输
        if byte_diff == 0 {
            #[cfg(debug_assertions)]
            debug!("No data transferred, returning 0.0");
            return 0.0;
        }

        // 计算速度: (字节差 * 8) / (时间差 微秒) = Mbps
        // 注意单位转换: 
        // - 字节转比特: * 8
        // - 微秒转秒: / 1_000_000
        // - 结果单位: Mbps (兆比特每秒)
        let speed_mbps = (byte_diff as f64 * 8.0) / (time_diff as f64 / 1_000_000.0) / 1_000_000.0;
        
        #[cfg(debug_assertions)]
        debug!("Calculated speed: {} Mbps", speed_mbps);
        
        // 确保返回值不是负数或NaN
        if speed_mbps.is_nan() || speed_mbps.is_sign_negative() {
            #[cfg(debug_assertions)]
            debug!("Calculated speed is invalid (NaN or negative), returning 0.0");
            return 0.0;
        }
        
        // 对于非常小的速度值，返回一个最小的非零值以表明有数据传输
        if speed_mbps > 0.0 && speed_mbps < 0.001 {
            #[cfg(debug_assertions)]
            debug!("Calculated speed is very small, returning minimum displayable value");
            return 0.001; // 返回最小显示值0.001 Mbps而不是0.0
        }

        speed_mbps
    }

    pub fn status(&self) -> String {
        let mut stop = 0;
        let mut last = 0;
        let results = self.results.lock().unwrap().to_vec();

        #[cfg(debug_assertions)]
        debug!("Results {results:?}");

        // 检查是否有足够的结果数据
        if results.is_empty() {
            return String::from("无数据");
        }

        // 检查是否所有数据都已完成传输（最后几次计数相同表示传输完成）
        let mut consecutive_same = 0;
        for (num, _) in results.iter().rev().take(3) {
            if *num == last || last == 0 {
                consecutive_same += 1;
            }
            last = *num;
        }
        
        // 如果最后几次计数相同，说明传输已完成，不应该是断流
        if consecutive_same >= 3 {
            return String::from("正常");
        }

        // 原始断流检测逻辑
        last = 0;
        for (num, _) in results.iter() {
            if *num == last {
                stop += 1;
            }
            last = *num;
        }

        if stop < 6 {
            String::from("正常")
        } else {
            String::from("断流")
        }
    }
}

pub trait Client {
    fn result(&self) -> SpeedTestResult;

    fn ping(&mut self) -> bool;

    fn upload(&mut self) -> bool;

    fn download(&mut self) -> bool;

    fn run(&mut self) -> bool {
        let r = self.ping();
        if r {
            thread::sleep(Duration::from_secs(2));
            if !self.upload() {
                return false;
            }
            thread::sleep(Duration::from_secs(3));
            if !self.download() {
                return false;
            }
            return true;
        }
        false
    }
}