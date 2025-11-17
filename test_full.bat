@echo off
setlocal

echo Bim-Core 完整测试脚本
echo ====================
echo.

echo 编译信息:
echo 服务器程序: target\release\bim.exe (大小: %~z0 bytes)
echo 客户端程序: target\release\bimc.exe (大小: %~z0 bytes)
echo.

echo 测试步骤:
echo 1. 启动测试服务器
echo    命令: target\release\bim.exe 127.0.0.1:8080
echo.
echo 2. 在另一个终端运行客户端测试
echo    命令: target\release\bimc.exe http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.
echo 3. 查看程序帮助信息
echo    服务器帮助: target\release\bim.exe --help
echo    客户端帮助: target\release\bimc.exe --help
echo.
echo 可选测试:
echo 使用不同客户端类型:
echo    target\release\bimc.exe -c http http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.
echo 启用多线程测试:
echo    target\release\bimc.exe -m 4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.
echo 测试IPv6 (如果系统支持):
echo    target\release\bimc.exe -6 http://[::1]:8080/download http://[::1]:8080/upload
echo.
echo 查看详细日志 (调试模式):
echo    设置环境变量 RUST_LOG=debug 后运行程序
echo.

pause