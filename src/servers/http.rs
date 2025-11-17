use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

#[cfg(debug_assertions)]
use log::debug;

use crate::servers::base::Server;
use crate::utils::justify_name;

pub struct HTTPServer {
    listener: TcpListener,
    name: String,
}

impl HTTPServer {
    pub fn build(address: String, name: String) -> Option<Box<dyn Server>> {
        println!("Trying to bind to address: {}", address);
        
        // 尝试绑定地址，如果失败则打印详细错误信息
        let listener = match TcpListener::bind(&address) {
            Ok(listener) => {
                println!("Successfully bound to address: {}", address);
                listener
            },
            Err(e) => {
                eprintln!("Failed to bind to address {}: {}", address, e);
                return None;
            }
        };

        Some(Box::new(Self { listener, name }))
    }
}

impl Server for HTTPServer {
    fn run(&mut self) -> bool {
        let name = self.name.clone();
        let name_just = justify_name(&name, 12, false);

        println!("HTTP Server listening on {}", self.listener.local_addr().unwrap());
        
        // 设置监听器为非阻塞模式
        if let Err(e) = self.listener.set_nonblocking(false) {
            eprintln!("Failed to set listener to blocking mode: {}", e);
            return false;
        }

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    #[cfg(debug_assertions)]
                    debug!("New incoming connection");
                    
                    let n = name.clone();
                    let nj = name_just.clone();

                    thread::spawn(move || {
                        #[cfg(debug_assertions)]
                        debug!("Handling client in spawned thread");
                        
                        if let Err(e) = handle_client(stream, n, nj) {
                            #[cfg(debug_assertions)]
                            debug!("Error handling client: {}", e);
                        }
                        
                        #[cfg(debug_assertions)]
                        debug!("Client handling thread finished");
                    });
                }
                Err(e) => {
                    #[cfg(debug_assertions)]
                    debug!("Error accepting connection: {}", e);
                    
                    // 如果是非阻塞错误，继续循环
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                }
            }
        }
        true
    }
}

fn handle_client(mut stream: TcpStream, name: String, name_just: String) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    debug!("New client connection from: {}", stream.peer_addr()?);
    
    // 读取并解析HTTP请求头
    let mut request_str = String::new();
    let mut header_buffer = [0; 1];
    
    loop {
        match stream.read(&mut header_buffer) {
            Ok(0) => {
                #[cfg(debug_assertions)]
                debug!("Connection closed by client");
                break; // 连接关闭
            },
            Ok(_) => {
                request_str.push(header_buffer[0] as char);
                
                // 检查是否读取到了完整的HTTP请求头
                if request_str.len() >= 4 {
                    if request_str.ends_with("\r\n\r\n") {
                        #[cfg(debug_assertions)]
                        debug!("Received complete HTTP request header");
                        break;
                    }
                }
                
                // 防止无限读取
                if request_str.len() > 8192 {
                    #[cfg(debug_assertions)]
                    debug!("Request header too large, dropping connection");
                    return Ok(());
                }
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                debug!("Error reading request: {}", _e);
                break;
            }
        }
    }

    #[cfg(debug_assertions)]
    debug!("Request:\n{}", request_str);

    let request_lines: Vec<&str> = request_str.lines().collect();

    if request_lines.is_empty() {
        #[cfg(debug_assertions)]
        debug!("Empty request");
        return Ok(());
    }

    let request_line = request_lines[0];
    let request_parts: Vec<&str> = request_line.split_whitespace().collect();

    if request_parts.len() < 2 {
        #[cfg(debug_assertions)]
        debug!("Malformed request line");
        return Ok(());
    }

    let path = request_parts[1];
    let path_parts: Vec<&str> = path.split('?').collect();
    let path_segments: Vec<&str> = path_parts[0].trim_start_matches('/').split('/').collect();

    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;

    #[cfg(debug_assertions)]
    debug!("Processing path: {:?}", path_segments);

    match path_segments.as_slice() {
        [""] => {
            #[cfg(debug_assertions)]
            debug!("Serving root path");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                name.len(),
                name
            );
            stream.write_all(response.as_bytes())?;
        }
        ["nj"] => {
            #[cfg(debug_assertions)]
            debug!("Serving /nj path");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                name_just.len(),
                name_just
            );
            stream.write_all(response.as_bytes())?;
        }
        ["download"] => {
            #[cfg(debug_assertions)]
            debug!("Serving download request");
            
            // 客户端期望接收16MB数据用于下载测速
            let data_size = 16 * 1024 * 1024; // 16MB
            let response_head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", data_size);
            
            #[cfg(debug_assertions)]
            debug!("Sending download response header, content-length: {}", data_size);
            
            // 先发送响应头
            if let Err(_e) = stream.write_all(response_head.as_bytes()) {
                #[cfg(debug_assertions)]
                debug!("Error writing download header: {}", _e);
                return Ok(());
            }

            // 发送16MB数据用于下载测试
            let chunk = [b'X'; 1024]; // 1KB数据块，使用可识别的字符
            let chunks = data_size / chunk.len(); // 需要发送的块数
            
            #[cfg(debug_assertions)]
            debug!("Sending {} chunks of data", chunks);
            
            let mut sent_bytes = 0;
            for i in 0..chunks {
                if let Err(_e) = stream.write_all(&chunk) {
                    #[cfg(debug_assertions)]
                    debug!("Error sending download data at chunk {}: {}", i, _e);
                    break;
                }
                
                sent_bytes += chunk.len();
                
                // 每发送1MB数据输出一次日志
                #[cfg(debug_assertions)]
                if sent_bytes % (1024 * 1024) == 0 {
                    debug!("Sent {} MB of download data", sent_bytes / (1024 * 1024));
                }
                
                // 确保数据被刷新到网络
                if let Err(_e) = stream.flush() {
                    #[cfg(debug_assertions)]
                    debug!("Error flushing download stream: {}", _e);
                    break;
                }
            }
            
            #[cfg(debug_assertions)]
            debug!("Download completed, total sent bytes: {}", sent_bytes);
        }
        ["upload"] => {
            #[cfg(debug_assertions)]
            debug!("Serving upload request");
            
            // 处理客户端上传的数据（客户端会发送50MB数据）
            let mut buffer = [0; 8192]; // 使用更大的缓冲区
            let mut total_received = 0u64;
            
            // 解析Content-Length头部以确定需要读取的数据量
            let content_length = parse_content_length(&request_str);
            let expected_bytes = content_length.unwrap_or(50 * 1024 * 1024); // 默认50MB
            
            #[cfg(debug_assertions)]
            debug!("Expected to receive {} bytes", expected_bytes);
            
            // 读取所有上传的数据
            while total_received < expected_bytes {
                match stream.read(&mut buffer) {
                    Ok(0) => {
                        #[cfg(debug_assertions)]
                        debug!("Connection closed by client during upload");
                        break; // 连接关闭
                    },
                    Ok(size) => {
                        total_received += size as u64;
                        
                        // 每接收1MB数据输出一次日志
                        #[cfg(debug_assertions)]
                        if total_received % (1024 * 1024) < size as u64 {
                            debug!("Received {} MB of upload data so far", total_received / (1024 * 1024));
                        }
                        
                        // 每接收一定数据就刷新一次，确保连接保持活跃
                        if total_received % (8192 * 100) == 0 {
                            if let Err(_e) = stream.flush() {
                                #[cfg(debug_assertions)]
                                debug!("Error flushing upload stream: {}", _e);
                                break;
                            }
                        }
                    }
                    Err(_e) => {
                        #[cfg(debug_assertions)]
                        debug!("Error reading upload data: {}", _e);
                        break;
                    }
                }
            }
            
            #[cfg(debug_assertions)]
            debug!("Upload completed, received {} bytes", total_received);
            
            // 返回响应
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
            stream.write_all(response.as_bytes())?;
            
            if let Err(_e) = stream.flush() {
                #[cfg(debug_assertions)]
                debug!("Error flushing upload stream: {}", _e);
            }
        }
        _ => {
            #[cfg(debug_assertions)]
            debug!("Serving 404 for unknown path: {:?}", path_segments);
            let response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found";
            stream.write_all(response.as_bytes())?;
        }
    }

    if let Err(_e) = stream.flush() {
        #[cfg(debug_assertions)]
        debug!("Error flushing stream: {}", _e);
    }
    
    Ok(())
}

// 解析HTTP请求中的Content-Length头部
fn parse_content_length(request: &str) -> Option<u64> {
    for line in request.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                return parts[1].trim().parse().ok();
            }
        }
    }
    None
}