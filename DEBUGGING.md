# 项目调试指南

## 前提条件

在开始调试之前，确保已满足以下条件：

1. Rust 工具链已正确安装
2. 项目能够成功构建
3. VS Code 中已安装必要的扩展

## 安装 Rust 工具链

### Windows 系统安装方法

#### 方法一：使用安装程序（推荐）

1. 访问 [https://www.rust-lang.org/zh-CN/tools/install](https://www.rust-lang.org/zh-CN/tools/install)
2. 下载 `rustup-init.exe`
3. 运行安装程序并按照提示完成安装
4. 重启终端（命令提示符或 PowerShell）

#### 方法二：使用 winget 包管理器

在 PowerShell（以管理员身份运行）中执行：
```powershell
winget install Rustlang.Rustup
```

#### 方法三：使用 Git Bash

如果你安装了 Git for Windows，可以使用 Git Bash 执行：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 验证安装

打开新的终端窗口并运行：
```bash
rustc --version
cargo --version
```

如果能正常显示版本信息，说明安装成功。

## 构建项目

在项目根目录中执行：
```bash
cd c:\Users\mouse_0232\Downloads\bim-core-main

# 构建 debug 版本
cargo build

# 或构建 release 版本
cargo build --release
```

## 在 VS Code 中调试

### 安装必要扩展

1. 打开 VS Code
2. 安装以下扩展：
   - Rust Analyzer
   - CodeLLDB

### 配置调试环境

在项目根目录创建 `.vscode` 文件夹，并添加以下配置文件：

#### 1. 创建 `.vscode/launch.json`

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "name": "Debug bimc (client)",
            "type": "lldb",
            "request": "launch",
            "program": "${workspaceFolder}/target/debug/bimc",
            "args": [],
            "cwd": "${workspaceFolder}",
            "sourceLanguages": ["rust"]
        },
        {
            "name": "Debug bim (server)",
            "type": "lldb",
            "request": "launch",
            "program": "${workspaceFolder}/target/debug/bim",
            "args": [],
            "cwd": "${workspaceFolder}",
            "sourceLanguages": ["rust"]
        }
    ]
}
```

#### 2. 创建 `.vscode/tasks.json`

```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "build-debug",
            "type": "shell",
            "command": "cargo build",
            "group": "build",
            "presentation": {
                "echo": true,
                "reveal": "always",
                "focus": false,
                "panel": "shared"
            },
            "problemMatcher": {
                "owner": "rust",
                "fileLocation": ["relative", "${workspaceFolder}"],
                "pattern": {
                    "regexp": "^(.+):(\\d+):(\\d+):\\s+(warning|error):\\s+(.*)$",
                    "file": 1,
                    "line": 2,
                    "column": 3,
                    "severity": 4,
                    "message": 5
                }
            }
        },
        {
            "label": "build-release",
            "type": "shell",
            "command": "cargo build --release",
            "group": "build",
            "presentation": {
                "echo": true,
                "reveal": "always",
                "focus": false,
                "panel": "shared"
            },
            "problemMatcher": {
                "owner": "rust",
                "fileLocation": ["relative", "${workspaceFolder}"],
                "pattern": {
                    "regexp": "^(.+):(\\d+):(\\d+):\\s+(warning|error):\\s+(.*)$",
                    "file": 1,
                    "line": 2,
                    "column": 3,
                    "severity": 4,
                    "message": 5
                }
            }
        }
    ]
}
```

### 开始调试

1. 首先构建项目：
   ```bash
   cargo build
   ```

2. 在 VS Code 中设置断点：
   - 打开要调试的源文件（如 [client.rs](file:///c%3A/Users/mouse_0232/Downloads/bim-core-main/src/client.rs) 或 [server.rs](file:///c%3A/Users/mouse_0232/Downloads/bim-core-main/src/server.rs)）
   - 点击行号左侧区域设置断点

3. 启动调试：
   - 按 `F5` 或点击 VS Code 中的"运行与调试"面板
   - 选择要调试的配置（"Debug bimc (client)" 或 "Debug bim (server)"）
   - 程序将在断点处暂停，可以查看变量值、调用栈等信息

## 命令行调试

如果希望在命令行中进行调试，可以使用以下方法：

### 使用 GDB（需要配置）

1. 安装 MSYS2 或类似的工具集
2. 安装 gdb 工具
3. 使用以下命令调试：
   ```bash
   gdb target/debug/bimc
   ```

### 使用 LLDB（推荐）

如果已安装 CodeLLDB 扩展，也可以在命令行中使用 LLDB：
```bash
lldb target/debug/bim
```

## 常见问题解决

### 1. "未能加载模块"错误

确保在调试前已成功构建项目：
```bash
cargo build
```

### 2. 断点未命中

检查是否在正确的代码位置设置了断点，某些优化代码可能无法命中断点。

### 3. 调试信息缺失

确保使用 debug 模式构建项目，release 模式可能缺少调试信息：
```bash
cargo build  # 而不是 cargo build --release
```

## 调试技巧

1. 使用 `println!` 宏输出变量值进行简单调试
2. 在复杂逻辑中添加日志输出
3. 利用 VS Code 的变量监视功能
4. 使用调用栈查看函数调用关系
5. 在循环或条件语句中设置条件断点