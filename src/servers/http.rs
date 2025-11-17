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
        let listener = TcpListener::bind(address).ok()?;

        Some(Box::new(Self { listener, name }))
    }
}

impl Server for HTTPServer {
    fn run(&mut self) -> bool {
        let name = self.name.clone();
        let name_just = justify_name(&name, 12, false);

        println!("HTTP Server listening on {}", self.listener.local_addr().unwrap());

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let n = name.clone();
                    let nj = name_just.clone();

                    thread::spawn(move || {
                        if let Err(_e) = handle_client(stream, n, nj) {
                            #[cfg(debug_assertions)]
                            debug!("Error handling client: {}", _e);
                        }
                    });
                }
                Err(_e) => {
                    #[cfg(debug_assertions)]
                    debug!("Error accepting connection: {}", _e);
                }
            }
        }
        true
    }
}

fn handle_client(mut stream: TcpStream, name: String, name_just: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer)?;

    let request_str = String::from_utf8_lossy(&buffer[..]);
    let request_lines: Vec<&str> = request_str.lines().collect();

    if request_lines.is_empty() {
        return Ok(());
    }

    let request_line = request_lines[0];
    let request_parts: Vec<&str> = request_line.split_whitespace().collect();

    if request_parts.len() < 2 {
        return Ok(());
    }

    let path = request_parts[1];
    let path_parts: Vec<&str> = path.split('?').collect();
    let path_segments: Vec<&str> = path_parts[0].trim_start_matches('/').split('/').collect();

    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;

    match path_segments.as_slice() {
        [""] => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                name.len(),
                name
            );
            stream.write_all(response.as_bytes())?;
        }
        ["nj"] => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                name_just.len(),
                name_just
            );
            stream.write_all(response.as_bytes())?;
        }
        ["download"] => {
            let response_head = "HTTP/1.1 200 OK\r\nContent-Length: 16777216\r\n\r\n";
            stream.write_all(response_head.as_bytes())?;

            // 创建一些数据供下载
            let data = vec![0u8; 1024];
            for _ in 0..16384 {
                stream.write_all(&data)?;
            }
        }
        ["upload"] => {
            // 读取上传的数据
            let mut buffer = [0; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break, // 连接关闭
                    Ok(_) => {
                        // 处理接收到的数据，这里我们只是简单地读取并丢弃
                    }
                    Err(_) => break,
                }
            }
            
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
            stream.write_all(response.as_bytes())?;
        }
        _ => {
            let response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found";
            stream.write_all(response.as_bytes())?;
        }
    }

    stream.flush()?;
    Ok(())
}