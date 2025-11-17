mod base;
mod http;
mod tcp_speedtest_net;
mod async_http;

pub use base::Client;
pub use http::HTTPClient;
pub use tcp_speedtest_net::SpeedtestNetTcpClient;
pub use async_http::AsyncHTTPClient;