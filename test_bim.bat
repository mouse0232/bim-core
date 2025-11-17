@echo off
echo Bim-Core 测试脚本
echo =================
echo.

echo 1. 启动服务器 (在端口8080上):
echo    在另一个命令行窗口中运行:
echo    target\release\bim.exe 127.0.0.1:8080
echo.
echo 2. 运行客户端测试:
echo    target\release\bimc.exe http://127.0.0.1:8080/nj http://127.0.0.1:8080/upload
echo.
echo 3. 查看帮助:
echo    target\release\bim.exe --help
echo    target\release\bimc.exe --help
echo.
echo 请按照以上步骤测试程序。
pause