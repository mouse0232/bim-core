@echo off
echo Bim-Core IPv4/IPv6 测试脚本
echo ========================
echo.

echo 1. 默认模式（自动选择IP版本）:
target\release\bimc.exe http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.

echo 2. 强制使用IPv4 (-4):
target\release\bimc.exe -4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.

echo 3. 强制使用IPv6 (-6):
target\release\bimc.exe -6 http://[::1]:8080/download http://[::1]:8080/upload
echo.

echo 4. 多线程 + IPv4:
target\release\bimc.exe -4 -m 4 http://127.0.0.1:8080/download http://127.0.0.1:8080/upload
echo.

echo 5. 多线程 + IPv6:
target\release\bimc.exe -6 -m 4 http://[::1]:8080/download http://[::1]:8080/upload
echo.

pause