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

    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let host_str = url.host_str().unwrap();
    let server_name = ServerName::try_from(host_str.to_string()).unwrap();
    let conn = rustls::ClientConnection::new(Arc::new(config), server_name).unwrap();

    while retry > 0 {
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_micros(1_000_000)) {
            #[cfg(debug_assertions)]
            debug!("TCP connected");

            let _r = stream.set_write_timeout(Some(Duration::from_secs(3)));
            let _r = stream.set_read_timeout(Some(Duration::from_secs(3)));
            if !ssl {
                return Ok(Box::new(stream));
            }

            let tls = rustls::StreamOwned::new(conn, stream);

            #[cfg(debug_assertions)]
            debug!("SSL connected");

            return Ok(Box::new(tls));
        }

        retry -= 1;
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
    results: std::sync::Mutex<Vec<(u64, u128)>>,
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
        
        // 确保有足够的数据点来计算速度
        if results.len() < 28 {
            return 0.0;
        }
        
        // 添加异常值过滤机制
        let mut speeds = Vec::new();
        
        // 计算多个时间窗口的速度
        for i in 10..results.len() - 5 {
            let (c1, t1) = results[i-5];
            let (c2, t2) = results[i];
            
            if t2 > t1 && c2 >= c1 {
                let speed = ((c2 - c1) * 8) as f64 / (t2 - t1) as f64;
                speeds.push(speed);
            }
        }
        
        if speeds.is_empty() {
            return 0.0;
        }
        
        // 对速度进行排序并去除异常值
        speeds.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        // 去除前5%和后5%的极值
        let remove_count = (speeds.len() as f64 * 0.05).ceil() as usize;
        let start = remove_count.min(speeds.len());
        let end = speeds.len().saturating_sub(remove_count);
        
        if start >= end {
            return speeds[speeds.len() / 2]; // 返回中位数
        }
        
        // 计算剩余值的平均值
        let sum: f64 = speeds[start..end].iter().sum();
        sum / (end - start) as f64
    }

    pub fn status(&self) -> String {
        let results = self.results.lock().unwrap().to_vec();

        #[cfg(debug_assertions)]
        debug!("Results {results:?}");

        // 检查是否有足够的结果数据
        if results.is_empty() {
            return String::from("无数据");
        }

        // 添加异常值过滤机制
        let mut stops = 0;
        let mut last = 0;
        
        // 检查数据变化情况，识别断流
        for (num, _) in results.iter().skip(5) { // 跳过前几个数据点
            if *num == last {
                stops += 1;
            }
            last = *num;
        }

        // 根据停止次数判断状态
        if stops < 3 {
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