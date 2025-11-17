# Bim-Core - 网络测速核心组件

Bim-Core 是 [bench.im](https://bench.im) 的客户端核心组件，提供基础通信和网络功能支持。

## 项目改进

本版本在原始项目基础上进行了多项改进：

### 1. IPv4/IPv6 容错支持
- 添加 `-4` 参数强制使用 IPv4
- 保留 `-6` 参数强制使用 IPv6
- 默认模式下自动选择最优 IP 版本（优先 IPv4）

### 2. 异步支持
- 添加了异步 HTTP 客户端实现
- 使用 tokio 和 tokio-rustls 提供异步网络操作

### 3. 错误处理优化
- 减少了 unwrap() 的使用
- 添加了更详细的错误信息和日志记录

### 4. 性能优化
- 优化了 LoadCounter 实现，使用原子操作提高并发性能
- 改进了线程同步机制

## 使用方法

### 服务器端
```bash
# 启动服务器
./bim 127.0.0.1:8080
```

### 客户端
```bash
# 基本测试
./bimc http://127.0.0.1:8080/download http://127.0.0.1:8080/upload

# 强制使用 IPv4
./bimc -4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload

# 强制使用 IPv6
./bimc -6 http://[::1]:8080/download http://[::1]:8080/upload

# 多线程测试
./bimc -m 4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
```

## 编译

```bash
# 编译 release 版本
cargo build --release
```

## 许可证

GPL-2.0-only