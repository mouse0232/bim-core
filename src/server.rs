use getopts::Options;
use std::env;

use bim_core::servers::{Server, HTTPServer};

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} HOST:PORT [options]", program);
    print!("{}", opts.usage(&brief));
}

fn get_server(server_name: &str, address: &str) -> Option<Box<dyn Server>> {
    println!("Trying to create server: {}", server_name); // 调试信息
    match server_name.to_lowercase().as_str() {
        "http" => HTTPServer::build(address.to_string(), "test".to_string()),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("s", "server", "set test server", "NAME");
    opts.optflag("h", "help", "print this help menu");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            panic!("{}", f.to_string())
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    let address = if !matches.free.is_empty() {
        matches.free[0].clone()
    } else {
        print_usage(&program, opts);
        return;
    };

    #[cfg(debug_assertions)]
    env_logger::init();

    // 简化服务器名称处理逻辑
    let server_name = matches.opt_str("s").unwrap_or("http".to_string());
    println!("Server name from args: {}", server_name); // 调试信息
    
    let mut server = match get_server(&server_name, &address) {
        Some(server) => server,
        None => {
            eprintln!("Error: Unknown server '{}'", server_name);
            return;
        }
    };

    server.run();
}