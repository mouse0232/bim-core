use getopts::Options;
use std::env;

use bim_core::clients::{Client, HTTPClient, SpeedtestNetTcpClient};
use bim_core::utils::{justify_name, SpeedTestResult};

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} DOWNLOAD_URL UPLOAD_URL [options]", program);
    print!("{}", opts.usage(&brief));
}

fn get_client(
    client_name: &str,
    download_url: String,
    upload_url: String,
    ipv4: bool,
    ipv6: bool,
    threads: u8,
) -> Option<Box<dyn Client>> {
    match client_name {
        "http" => HTTPClient::build(download_url, upload_url, ipv4, ipv6, threads),
        "tcp" => SpeedtestNetTcpClient::build(upload_url, ipv4, ipv6, threads),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("c", "client", "set test client", "NAME");
    opts.optflagopt("m", "multi", "enable multi threads", "NUM");
    opts.optflag("4", "ipv4", "force ipv4");
    opts.optflag("6", "ipv6", "force ipv6");
    opts.optflag("n", "name", "print justified name");
    opts.optflag("h", "help", "print this help menu");
    
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("Error parsing arguments: {}\n", f);
            print_usage(&program, opts);
            std::process::exit(1);
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return Ok(());
    }

    let (dl, ul) = match matches.free.as_slice() {
        [first, second, ..] => (Some(first), Some(second)),
        _ => {
            eprintln!("Error: DOWNLOAD_URL and UPLOAD_URL are required\n");
            print_usage(&program, opts);
            std::process::exit(1);
        }
    };

    if matches.opt_present("n") {
        if let Some(name) = dl {
            print!("{}", justify_name(name, 12, true));
        } else {
            eprintln!("Error: DOWNLOAD_URL is required for -n option\n");
            print_usage(&program, opts);
            std::process::exit(1);
        }
        return Ok(());
    }

    if ul.is_none() {
        eprintln!("Error: UPLOAD_URL is required\n");
        print_usage(&program, opts);
        std::process::exit(1);
    }

    let download_url = dl.unwrap().clone();
    let upload_url = ul.unwrap().clone();
    
    // 处理IPv4/IPv6参数
    let force_ipv4 = matches.opt_present("4");
    let force_ipv6 = matches.opt_present("6");
    
    // 如果同时指定了-4和-6，则报错
    if force_ipv4 && force_ipv6 {
        eprintln!("Error: -4 and -6 cannot be used together\n");
        print_usage(&program, opts);
        std::process::exit(1);
    }

    let threads = matches
        .opt_str("m")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    #[cfg(debug_assertions)]
    env_logger::init();

    let client_name = matches.opt_str("c").unwrap_or("http".to_string());
    let result = match get_client(&client_name, download_url, upload_url, force_ipv4, force_ipv6, threads) {
        Some(mut client) => {
            match client.run() {
                true => {
                    println!("Test completed successfully");
                    client.result()
                },
                false => {
                    eprintln!("Error: Failed to run speed test");
                    SpeedTestResult::build(0.0, "失败".to_string(), 0.0, "失败".to_string(), 0.0, 0.0)
                }
            }
        }
        None => {
            eprintln!("Error: Unknown client '{}'", client_name);
            SpeedTestResult::build(0.0, "失败".to_string(), 0.0, "失败".to_string(), 0.0, 0.0)
        }
    };

    println!("{}", result.text());
    Ok(())
}