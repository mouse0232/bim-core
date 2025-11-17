# 开发环境设置与项目构建指南

## 环境要求

- Rust 工具链 (推荐使用最新稳定版)
- Windows 10/11 或更高版本 (当前为 Windows 22H2)
- Visual Studio 2017 或更高版本，或 Visual Studio Build Tools（包含 C++ 组件）
- CMake 3.12 或更高版本
- NASM (可选，但推荐安装以获得最佳性能)

## 安装 Rust 工具链

### Windows 系统

1. 访问 [https://www.rust-lang.org/zh-CN/tools/install](https://www.rust-lang.org/zh-CN/tools/install)
2. 下载 `rustup-init.exe`
3. 运行下载的程序并按照提示完成安装
4. 重启命令行或 PowerShell 窗口使环境变量生效

或者在 PowerShell 中运行以下命令安装：
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
irm https://sh.rustup.rs | iex
```

## 安装构建工具

### 安装 Visual Studio 构建工具

为了在 Windows 上编译 Rust 项目，你需要安装 Visual Studio 或 Build Tools：

1. 访问 [https://visualstudio.microsoft.com/downloads/](https://visualstudio.microsoft.com/downloads/)
2. 下载 "Build Tools for Visual Studio"
3. 运行安装程序
4. 在工作负载中选择 "C++ build tools"
5. 确保选择了以下组件：
   - MSVC v143 - VS 2022 C++ x64/x86 生成工具
   - Windows 10/11 SDK

### 安装 CMake

某些依赖项需要 CMake 来构建：

1. 访问 [https://cmake.org/download/](https://cmake.org/download/)
2. 下载适用于 Windows 的最新版本安装程序
3. 运行安装程序并按照提示完成安装
4. 确保在安装过程中选择 "Add to PATH" 选项

### 安装 NASM（可选）

为了获得最佳性能，建议安装 NASM 汇编器：

1. 访问 [https://www.nasm.us/](https://www.nasm.us/)
2. 下载最新版本的 Windows 安装程序
3. 运行安装程序并按照提示完成安装
4. 将 NASM 添加到系统 PATH 环境变量中

## 构建项目

### 使用命令行

1. 打开新的命令提示符或 PowerShell 窗口
2. 进入项目目录：
   ```cmd
   cd c:\Users\mouse_0232\Downloads\bim-core-main
   ```
3. 构建项目：
   ```cmd
   cargo build
   ```
4. 构建优化版本：
   ```cmd
   cargo build --release
   ```

### 使用 PowerShell

1. 打开新的 PowerShell 窗口
2. 进入项目目录：
   ```powershell
   cd "c:\Users\mouse_0232\Downloads\bim-core-main"
   ```
3. 构建项目：
   ```powershell
   cargo build
   ```

## 运行程序

### 运行客户端 (bimc)
```cmd
cargo run --bin bimc
```

或者运行已构建的二进制文件：
```cmd
# Debug 版本
target\debug\bimc.exe

# Release 版本
target\release\bimc.exe
```

### 运行服务器 (bim)
```cmd
cargo run --bin bim
```

或者运行已构建的二进制文件：
```cmd
# Debug 版本
target\debug\bim.exe

# Release 版本
target\release\bim.exe
```

## 常见问题解决

### 1. 'cargo' 不是内部或外部命令

这表示 Rust 工具链未正确安装或未添加到 PATH 中。

解决方法：
1. 确认已安装 Rust：在 PowerShell 中运行 `rustc --version`
2. 如果未安装，请按照上面的安装说明操作
3. 如果已安装但仍然无法识别，请重启终端或手动添加到 PATH：
   - 将 `C:\Users\[用户名]\.cargo\bin` 添加到系统 PATH 环境变量中

### 2. PowerShell 执行策略错误

如果遇到执行策略错误，可以运行以下命令：
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### 3. 编译错误

如果遇到编译错误，请确保：
1. 使用的是兼容的 Rust 版本（edition 2021 需要 Rust 1.56+）
2. 所有依赖项都正确下载
3. 网络连接正常（下载依赖项需要）
4. 已安装 Visual Studio 或 Build Tools 并包含 C++ 组件

### 4. 链接器错误 (link.exe 未找到)

这是最常见的 Windows 构建问题，解决方法：
1. 安装 Visual Studio 或 Build Tools（如上所述）
2. 确保选择了 C++ 开发工具
3. 重启终端后重试

### 5. CMake 未找到错误

某些依赖项需要 CMake 来构建：
1. 安装 CMake（如上所述）
2. 确保 CMake 已添加到 PATH 环境变量中
3. 重启终端后重试

### 6. NASM 未找到警告

虽然不是致命错误，但建议安装 NASM 以获得最佳性能：
1. 安装 NASM（如上所述）
2. 确保 NASM 已添加到 PATH 环境变量中

### 7. 权限问题

在 Windows 上运行时，如果遇到权限问题：
1. 确保在用户目录下运行命令
2. 或者以管理员身份运行命令提示符/PowerShell

## 测试项目

运行测试（如果有）：
```cmd
cargo test
```

格式化代码：
```cmd
cargo fmt
```

检查代码质量：
```cmd
cargo clippy
```